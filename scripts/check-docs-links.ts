/// <reference types="bun" />
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

/// Checks the links in the Markdown that a fresh clone can follow. External links are not fetched: the check is about
/// a moved file, a dead anchor or an image without alt text, not about the network.

const ROOT = join(import.meta.dir, "..");
const SKIP = new Set([".git", "node_modules", "target", "dist", "dist-release", "plans", "gen", "overlay-hook"]);
const STALE = ["src/service/", "service/target.rs", "service/run/"];
const EXTERNAL = /^(?:https?|mailto|tel|data):/i;
const FENCE = /```[\s\S]*?```/g;
const MARKDOWN_LINK = /(!?)\[([^\]]*)\]\(\s*(<[^>]*>|[^)\s]+)(?:\s+"[^"]*")?\s*\)/g;
const HTML_TAG = /<(a|img)\b([^>]*)>/gi;
const HTML_ATTR = /\b(href|src|alt)\s*=\s*(?:"([^"]*)"|'([^']*)')/gi;
const HEADING = /^#{1,6}\s+(.+?)\s*#*\s*$/;

function markdownFiles(dir: string): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		if (entry.isDirectory()) {
			if (SKIP.has(entry.name)) return [];
			return markdownFiles(join(dir, entry.name));
		}

		return entry.name.endsWith(".md") ? [join(dir, entry.name)] : [];
	});
}

/// Code fences contain examples, not links. Blank the content but keep the newlines so reported lines stay correct.
function withoutFences(text: string): string {
	return text.replace(FENCE, (block) => block.replace(/[^\n]/g, " "));
}

function lineAt(text: string, index: number): number {
	return text.slice(0, index).split("\n").length;
}

/// GitHub's heading anchors: lower case, markup removed, spaces to hyphens, punctuation dropped. Duplicate headings
/// get `-1`, `-2`, and so on.
function headingAnchors(text: string): Set<string> {
	const anchors = new Set<string>();
	const counts = new Map<string, number>();
	let inFence = false;

	for (const line of text.split("\n")) {
		if (line.trim().startsWith("```")) {
			inFence = !inFence;
			continue;
		}

		if (inFence) continue;
		const match = HEADING.exec(line);

		if (!match) continue;
		const heading = (match[1] ?? "")
			.replace(/<[^>]*>/g, "")
			.replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
			.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
			.replace(/`([^`]*)`/g, "$1")
			.replace(/[*~]/g, "")
			.trim();
		const base = heading
			.toLowerCase()
			.replace(/[^\p{L}\p{N}\s_-]/gu, "")
			.trim()
			.replace(/\s+/g, "-");
		if (base === "") continue;
		const count = counts.get(base) ?? 0;
		anchors.add(count === 0 ? base : `${base}-${count}`);
		counts.set(base, count + 1);
	}
	for (const match of text.matchAll(/<a\s+(?:name|id)="([^"]+)"/gi)) anchors.add(match[1] ?? "");
	return anchors;
}

function cleanTarget(raw: string): string | null {
	const target = raw.trim().replace(/^<|>$/g, "");
	if (target === "" || EXTERNAL.test(target)) return null;
	if (target.includes("{{") || target.includes("*")) return null;
	return target;
}

type LinkKind = "markdown" | "html";

interface Link {
	raw: string;
	image: boolean;
	kind: LinkKind;
	alt: string | null;
	line: number;
}

function linksIn(text: string): Link[] {
	const links: Link[] = [];
	const body = withoutFences(text);
	for (const match of body.matchAll(MARKDOWN_LINK)) {
		const raw = match[3];
		if (raw === undefined) continue;
		links.push({
			raw,
			image: match[1] === "!",
			kind: "markdown",
			alt: match[2] ?? null,
			line: lineAt(body, match.index),
		});
	}
	for (const tag of body.matchAll(HTML_TAG)) {
		const name = tag[1]?.toLowerCase();
		const attributes = new Map<string, string>();
		for (const attribute of (tag[2] ?? "").matchAll(HTML_ATTR)) {
			const key = attribute[1]?.toLowerCase();
			if (key === undefined) continue;
			attributes.set(key, attribute[2] ?? attribute[3] ?? "");
		}
		const raw = name === "img" ? attributes.get("src") : attributes.get("href");
		if (raw === undefined) continue;
		links.push({
			raw,
			image: name === "img",
			kind: "html",
			alt: name === "img" ? (attributes.get("alt") ?? null) : null,
			line: lineAt(body, tag.index),
		});
	}
	return links;
}

const anchorCache = new Map<string, Set<string>>();
function anchorsOf(path: string): Set<string> {
	const cached = anchorCache.get(path);
	if (cached) return cached;
	const anchors = headingAnchors(readFileSync(path, "utf8"));
	anchorCache.set(path, anchors);
	return anchors;
}

const problems: string[] = [];
const warnings: string[] = [];
for (const path of markdownFiles(ROOT)) {
	const text = readFileSync(path, "utf8");
	const shown = relative(ROOT, path);
	for (const stale of STALE) {
		if (text.includes(stale)) problems.push(`${shown}: stale path \`${stale}\``);
	}

	for (const link of linksIn(text)) {
		if (link.image) {
			if (link.alt === null) {
				problems.push(`${shown}:${link.line}: image without an alt attribute \`${link.raw}\``);
			} else if (link.kind === "markdown" && link.alt === "") {
				problems.push(`${shown}:${link.line}: Markdown image without alt text \`${link.raw}\``);
			}
		}

		const target = cleanTarget(link.raw);

		if (target === null) continue;
		// Plans are local and git-ignored, so a fresh clone cannot resolve them. The backlog may still point at one;
		// report it without failing the gate until the file moves out of the repository.
		if (/(^|\/)plans\//.test(target)) {
			warnings.push(`${shown}:${link.line}: local plan \`${target}\``);
			continue;
		}

		const hash = target.indexOf("#");
		const pathPart = hash === -1 ? target : target.slice(0, hash);
		const fragment = hash === -1 ? "" : target.slice(hash + 1);
		const cleanPath = pathPart.split("?")[0] ?? "";
		const base = cleanPath.startsWith("/") ? ROOT : dirname(path);
		let resolved = path;

		if (cleanPath !== "") {
			resolved = isAbsolute(cleanPath) ? cleanPath : resolve(base, cleanPath);
		}

		if (!existsSync(resolved)) {
			problems.push(`${shown}:${link.line}: missing \`${link.raw}\``);
			continue;
		}

		if (fragment === "" || !resolved.endsWith(".md") || /^L\d+/.test(fragment)) continue;
		let wanted = fragment;

		try {
			wanted = decodeURIComponent(fragment);
		} catch {
			// A malformed escape is left as it is and fails the anchor check below.
		}

		if (!(anchorsOf(resolved).has(wanted) || anchorsOf(resolved).has(wanted.toLowerCase()))) {
			problems.push(`${shown}:${link.line}: missing anchor \`#${fragment}\` in \`${relative(ROOT, resolved)}\``);
		}
	}
}

if (warnings.length > 0) {
	console.warn(`Docs links: ${warnings.length} local plan link(s), skipped.`);

	for (const warning of warnings) console.warn(`  ${warning}`);
}

if (problems.length > 0) {
	console.error(`Docs links: ${problems.length} problem(s).`);

	for (const problem of problems) console.error(`  ${problem}`);
	process.exit(1);
}

process.stdout.write("Docs links: every relative link, anchor and image alt resolves.\n");
