/// Finds locale keys that nothing appears to use. A diagnostic tool, not a merge gate: keys built at run time
/// (dynamic prefixes, backend message codes) need a human to confirm them before removal.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import en from "../src/locales/en.ts";

interface Messages {
	[key: string]: string | Messages;
}

const PLURAL = /_(zero|one|two|few|many|other)$/;
const ROOT = join(import.meta.dir, "..");
const ESCAPE = /[.*+?^${}()|[\]\\]/g;
const SOURCE_EXT = /\.(ts|tsx|js|jsx)$/;
const LOCALE_FILE = /locales\/(en|pl)\.ts$/;

function flatten(tree: Messages, prefix = "", into = new Map<string, string>()): Map<string, string> {
	for (const [key, value] of Object.entries(tree)) {
		const path = prefix + key;

		if (typeof value === "string") into.set(path, value);
		else flatten(value, `${path}.`, into);
	}

	return into;
}

function sources(dir: string, extension: RegExp): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		const path = join(dir, entry.name);

		if (entry.isDirectory()) return sources(path, extension);
		return extension.test(entry.name) ? [path] : [];
	});
}

const frontend = [join(ROOT, "src"), join(ROOT, "e2e"), join(ROOT, "scripts")]
	.flatMap((dir) => sources(dir, SOURCE_EXT))
	.filter((path) => !LOCALE_FILE.test(path));
const corpus = frontend.map((path) => readFileSync(path, "utf8")).join("\n");

// Codes from the backend: messages are translated by code, sometimes through a variable or a helper.
const backend = sources(join(ROOT, "src-tauri/src"), /\.rs$/)
	.map((path) => readFileSync(path, "utf8"))
	.join("\n");
const backendCodes = new Set([...backend.matchAll(/"([a-z][a-zA-Z0-9_]*\.[a-zA-Z0-9_.]+)"/g)].map((match) => match[1] ?? ""));

// Prefixes of typed template-literal keys, such as t(`mods.col.${id}`).
const prefixes = new Set<string>();

for (const match of corpus.matchAll(/`([a-zA-Z0-9_.]+\$\{)/g)) {
	const prefix = match[1]?.replace(/\$\{$/, "");
	if (prefix) prefixes.add(prefix);
}

const bases = new Set([...flatten(en as Messages).keys()].map((key) => key.replace(PLURAL, "")));
const unused: string[] = [];
for (const base of [...bases].toSorted()) {
	// Error codes are checked against the backend by `src/locales/error-codes.test.ts`.
	if (base.startsWith("errors.")) continue;
	if (new RegExp(`${base.replace(ESCAPE, "\\$&")}(?![A-Za-z0-9_])`).test(corpus)) continue;
	if (backendCodes.has(base) || backendCodes.has(`messages.${base}`)) continue;
	if ([...prefixes].some((prefix) => base.startsWith(prefix))) continue;
	unused.push(base);
}

if (unused.length === 0) {
	process.stdout.write("No unused translation keys found.\n");
} else {
	process.stdout.write(`Keys with no literal use, backend code, or dynamic prefix (${unused.length}):\n`);
	for (const key of unused) process.stdout.write(`  ${key}\n`);
	process.stdout.write("Check each one by hand before removing it from en.ts and pl.ts.\n");
}
