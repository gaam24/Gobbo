import { readFileSync } from "node:fs";
import { join } from "node:path";
import { portableName, rustFiles } from "./rust-sources";

/// The hook's hard invariants that a static read can prove: no heavy work in DllMain, no debug print in the shipped
/// hook, and no unsafe in the app crate. The dynamic invariants (the game folder is never written, last error is kept)
/// are covered by the hook tests and the probe.
const root = join(import.meta.dir, "..");
const hook = join(root, "src-tauri", "crates", "overlay-hook", "src");
const app = join(root, "src-tauri", "src");
const DEBUG_PRINT = /\b(println!|dbg!)\s*\(/;
const UNSAFE_BLOCK = /\bunsafe\s*\{/;

function dllMainBody(text: string): string | null {
	const start = text.indexOf("fn DllMain");
	if (start === -1) return null;
	const open = text.indexOf("{", start);
	if (open === -1) return null;
	let depth = 0;
	for (let at = open; at < text.length; at += 1) {
		if (text[at] === "{") depth += 1;
		if (text[at] === "}") {
			depth -= 1;
			if (depth === 0) return text.slice(open, at + 1);
		}
	}
	return null;
}

export function overlayProblems(hookRoot: string, appRoot: string, displayRoot = root): string[] {
	const problems: string[] = [];
	const lib = join(hookRoot, "lib.rs");
	if (readFileSync(lib, "utf8").includes("fn DllMain")) {
		const body = dllMainBody(readFileSync(lib, "utf8")) ?? "";
		for (const forbidden of ["fs::", "File::", "format!", "Mutex", "RwLock", "println!", "dbg!", "LoadLibrary", "detour"]) {
			if (body.includes(forbidden)) problems.push(`overlay-hook/src/lib.rs: DllMain contains ${forbidden}`);
		}
	} else {
		problems.push("overlay-hook/src/lib.rs has no DllMain");
	}
	for (const path of rustFiles(hookRoot)) {
		const text = readFileSync(path, "utf8");
		if (DEBUG_PRINT.test(text)) problems.push(`${portableName(displayRoot, path)}: debug print in the hook`);
	}
	for (const path of rustFiles(appRoot)) {
		if (UNSAFE_BLOCK.test(readFileSync(path, "utf8"))) {
			problems.push(`${portableName(displayRoot, path)}: unsafe in the app crate`);
		}
	}
	return problems;
}

if (import.meta.main) {
	const problems = overlayProblems(hook, app);
	if (problems.length > 0) {
		process.stderr.write(`${problems.join("\n")}\n`);
		process.exit(1);
	}
	process.stdout.write("Overlay invariants passed.\n");
}
