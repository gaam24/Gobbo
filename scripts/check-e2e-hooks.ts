import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import en from "../src/locales/en";

/// The E2E build reads test-only globals; a production build must not contain them. Vite replaces `import.meta.env.MODE`
/// at build time, so this check catches a regression that keeps a test seam in the release bundle.
const root = join(import.meta.dir, "..");
const dist = join(root, "dist");
const assets = join(dist, "assets");
const markers = ["__GOBBO_E2E_DIALOGS__", "__GOBBO_E2E_THROW__", "gobbo.e2e.throw", "__GOBBO_E2E_TASKS__"];

const problems: string[] = [];

/// Strips comments before the registration check, so a spec named in a comment is not counted as registered. Quotes are
/// tracked because a URL inside a string must not be mistaken for the start of a comment.
function withoutComments(source: string): string {
	let out = "";
	let quote: string | null = null;

	for (let at = 0; at < source.length; at += 1) {
		const char = source[at];
		const next = source[at + 1];

		if (quote !== null) {
			out += char;

			if (char === "\\") {
				out += next ?? "";
				at += 1;
			} else if (char === quote) {
				quote = null;
			}
			continue;
		}

		if (char === '"' || char === "'" || char === "`") {
			quote = char;
			out += char;
			continue;
		}

		if (char === "/" && next === "/") {
			while (at < source.length && source[at] !== "\n") at += 1;
			out += "\n";
			continue;
		}

		if (char === "/" && next === "*") {
			at += 2;
			while (at < source.length && !(source[at] === "*" && source[at + 1] === "/")) at += 1;
			at += 1;
			continue;
		}

		out += char;
	}

	return out;
}

/// A spec that is not in the runner's arrays never runs. The screenshot spec is invoked explicitly with `E2E_SPECS`
/// (see e2e/README.md), so it is the one documented exception. Only array literals count: a comment naming a spec does
/// not register it.
const specDir = join(root, "e2e", "specs");
const manualSpecs = new Set(["98-readme-screenshots.e2e.ts"]);
const config = withoutComments(readFileSync(join(root, "e2e", "wdio.conf.ts"), "utf8"));
const specArrays = [...config.matchAll(/=\s*\[([\s\S]*?)\]/g)].map((entry) => entry[1] ?? "").join("\n");
const registered = new Set([...specArrays.matchAll(/\.\/specs\/([\w-]+\.e2e\.ts)/g)].map((match) => match[1] ?? ""));

for (const name of readdirSync(specDir)) {
	if (!name.endsWith(".e2e.ts") || manualSpecs.has(name) || registered.has(name)) continue;
	problems.push(`spec not registered in e2e/wdio.conf.ts: ${name}`);
}

/// The helpers compare against the English window. A literal that is in no locale is a stale assertion that only fails
/// when the suite is run by hand. The English bundle is the source of truth; flatten its leaves once instead of
/// searching the raw source (where keys and comments would count as text).
interface LocalePhrase {
	/// The phrase with slots replaced by one sentinel. Interpolated literals use the same sentinel.
	plain: string;
	/// The whole phrase with each slot matching a concrete value, and its `startsWith` form.
	exact: RegExp;
	prefix: RegExp;
}

const PLACEHOLDER = /\{\{\s*\w+\s*\}\}/g;
/// A slot in a literal (`${name}`) and a slot in a locale value (`{{name}}`) become the same sentinel, so both forms
/// compare equal after normalization. A concrete value still matches the wildcard form of the locale phrase.
const SLOT = "\u0000";
const escapeRegex = (text: string) => text.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&");

function flatten(tree: unknown, into: string[] = []): string[] {
	if (typeof tree === "string") {
		into.push(tree);
		return into;
	}

	if (tree && typeof tree === "object") {
		for (const value of Object.values(tree)) flatten(value, into);
	}

	return into;
}

function phraseOf(value: string): LocalePhrase {
	const pattern: string[] = [];
	let last = 0;

	for (const match of value.matchAll(PLACEHOLDER)) {
		pattern.push(escapeRegex(value.slice(last, match.index)), ".+?");
		last = match.index + match[0].length;
	}

	pattern.push(escapeRegex(value.slice(last)));
	const body = pattern.join("");
	return {
		plain: value.replace(PLACEHOLDER, SLOT),
		exact: new RegExp(`^${body}$`),
		prefix: new RegExp(`^${body}`),
	};
}

const phrases = flatten(en).map((value) => phraseOf(value));
/// Source text that is fixture data, a selector, a key or an id is not interface prose. Identifiers are lower-case
/// tokens with `-`, `_` or digits (`remember-window`, `list-${id}-a` after interpolation is removed).
const selectorText = /^[#.[]/;
const e2eMarker = /\be2e\b/i;
const digitText = /\d/;
const nonAscii = /\P{ASCII}/u;
const identifierText = /^[a-z][a-z0-9_-]*$/;

function isTestData(text: string): boolean {
	return selectorText.test(text) || e2eMarker.test(text) || digitText.test(text) || nonAscii.test(text) || identifierText.test(text);
}

/// The checked helpers come from `ui.ts` itself: every exported function parameter named like text is a string the suite
/// matches against the window. Adding a text-taking helper to the support file brings it under the guard without a
/// change here. The match mode follows the helper's real assertion (exact equality, `startsWith` or `includes`).
const TEXT_PARAM = /^(?:text|label|title|prefix|name|rowText|buttonText|optionText|labelOrSelector|view)$/;
const EXACT = new Set([
	"waitForPlaceholder",
	"waitForHeading",
	"clickByLabel",
	"openTooltipByLabel",
	"openContextMenuByLabel",
	"setSwitch",
	"clickMenuTrigger",
	"clickGame",
	"waitForGame",
]);
const PREFIX = new Set(["clickByLabelPrefix"]);
const PARAM_SEPARATOR = /[:=]/;
const REST_PARAMETER = /^\.\.\./;

const ui = readFileSync(join(root, "e2e", "support", "ui.ts"), "utf8");
const checked = [...ui.matchAll(/export (?:async )?function (\w+)\(([\s\S]*?)\)\s*:/g)].flatMap((entry) => {
	const name = entry[1];
	const params = entry[2];

	if (name === undefined || params === undefined) return [];

	return params
		.split(",")
		.map((param, index) => ({
			name: param.split(PARAM_SEPARATOR)[0]?.trim().replace(REST_PARAMETER, "") ?? "",
			index,
		}))
		.filter((param) => TEXT_PARAM.test(param.name))
		.map((param) => ({ helper: name, index: param.index }));
});

/// Extracts the arguments of one call by balancing parentheses, so a second text argument (a row's button text or a
/// select's option text) is checked with the same rules as the first.
function argumentsOf(source: string, open: number): string[] {
	const args: string[] = [];
	let depth = 0;
	let quote: string | null = null;
	let start = open + 1;

	for (let at = open + 1; at < source.length; at += 1) {
		const char = source[at];

		if (quote !== null) {
			if (char === "\\") at += 1;
			else if (char === quote) quote = null;
			continue;
		}

		if (char === '"' || char === "'" || char === "`") {
			quote = char;
			continue;
		}

		if (char === "(" || char === "[" || char === "{") depth += 1;
		else if (char === ")" && depth === 0) {
			args.push(source.slice(start, at));
			return args;
		} else if (char === ")" || char === "]" || char === "}") depth -= 1;
		else if (char === "," && depth === 0) {
			args.push(source.slice(start, at));
			start = at + 1;
		}
	}

	return args;
}

/// A fixture string that is not prose carries an explicit annotation in its owning spec. The reason is required, so a
/// future reader can tell a deliberate exception from a stale assertion.
const EXCEPTION = /\/\/\s*e2e-locale-exception:\s*\S/;

function excepted(text: string, line: number): boolean {
	const lines = text.split("\n");
	return [line, line - 1].some((number) => number > 0 && EXCEPTION.test(lines[number - 1] ?? ""));
}

/// A backtick call fills an interpolated slot with a value, and a locale phrase fills the same slot with `{{name}}`.
/// Removing both kinds of slot makes the two forms comparable; a concrete value in the literal also matches the
/// locale's wildcard form.
const interpolation = /\$\{[^}]*\}/g;
const wordChar = /[\p{L}\p{N}_]/u;

/// A substring is only a match at a token boundary, so a short literal does not pass as part of a longer word in a locale
/// phrase (for example "High" inside "Highest priority number").
function includesToken(haystack: string, needle: string): boolean {
	if (needle.length === 0) return haystack.includes(needle);
	let from = 0;
	let at = haystack.indexOf(needle, from);

	while (at >= 0) {
		const before = haystack[at - 1];
		const after = haystack[at + needle.length];
		if ((before === undefined || !wordChar.test(before)) && (after === undefined || !wordChar.test(after))) return true;
		from = at + 1;
		at = haystack.indexOf(needle, from);
	}

	return false;
}

function inLocale(literal: string, interpolated: boolean, mode: "exact" | "prefix" | "contains"): boolean {
	const normalized = interpolated ? literal.replace(interpolation, SLOT) : literal;

	return phrases.some((phrase) => {
		if (mode === "exact") return phrase.plain === normalized || phrase.exact.test(literal);
		if (mode === "prefix") return phrase.plain.startsWith(normalized) || phrase.prefix.test(literal);
		return includesToken(phrase.plain, normalized) || phrase.exact.test(literal);
	});
}

for (const name of readdirSync(specDir)) {
	if (!name.endsWith(".e2e.ts")) continue;
	const text = readFileSync(join(specDir, name), "utf8");

	for (const { helper, index } of checked) {
		const call = new RegExp(`\\b${helper}\\s*\\(`, "g");

		for (const found of text.matchAll(call)) {
			const open = found.index + found[0].length - 1;
			const argument = argumentsOf(text, open)[index];
			if (argument === undefined) continue;
			const literal = /^\s*(["'`])([\s\S]*?)\1\s*$/.exec(argument);
			if (literal?.[2] === undefined) continue;

			const value = literal[2];
			const line = text.slice(0, open).split("\n").length;

			const interpolated = literal[1] === "`";
			const fixtureText = value.replace(interpolation, "");
			let mode: "exact" | "prefix" | "contains" = "contains";
			if (EXACT.has(helper)) mode = "exact";
			else if (PREFIX.has(helper)) mode = "prefix";

			if (isTestData(value) || (interpolated && isTestData(fixtureText)) || excepted(text, line) || inLocale(value, interpolated, mode)) {
				continue;
			}

			problems.push(`${name}:${line}: text ${JSON.stringify(value)} is in no locale`);
		}
	}
}

if (problems.length > 0) {
	console.error("E2E suite drift:");

	for (const problem of problems) console.error(`  ${problem}`);
	process.exit(1);
}

if (!existsSync(dist)) {
	process.stdout.write("No dist build; E2E hook check skipped.\n");
	process.exit(0);
}

if (!existsSync(assets)) {
	console.error("dist/ exists but dist/assets is missing; the E2E hook check cannot run.");
	process.exit(1);
}

const hits: string[] = [];

for (const name of readdirSync(assets)) {
	const path = join(assets, name);

	if (!/\.(?:js|css|html)$/.test(name)) continue;
	const text = readFileSync(path, "utf8");

	for (const marker of markers) {
		if (text.includes(marker)) hits.push(`${name}: ${marker}`);
	}
}

if (hits.length > 0) {
	console.error("Production bundle contains E2E test hooks:");

	for (const hit of hits) console.error(`  ${hit}`);
	process.exit(1);
}

process.stdout.write("Production bundle has no E2E test hooks.\n");
