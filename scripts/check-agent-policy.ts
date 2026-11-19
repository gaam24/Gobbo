import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..");
const APP = join(ROOT, "src-tauri", "src");
const TEST_MODULE = /#\[cfg\(test\)\]/;
const FORBIDDEN = [
	[/\.unwrap\(\)/, "unwrap()"],
	[/\.expect\(/, "expect()"],
	[/\bpanic!\(/, "panic!()"],
	[/\btodo!\(/, "todo!()"],
	[/\bunimplemented!\(/, "unimplemented!()"],
	[/\bdbg!\(/, "dbg!()"],
	[/\bprintln!\(/, "println!()"],
] as const;

/// Production code only: a test may unwrap and build any fixture it needs.
function production(text: string): string {
	const at = text.search(TEST_MODULE);
	return at === -1 ? text : text.slice(0, at);
}

function files(dir: string): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		const path = join(dir, entry.name);
		if (entry.isDirectory()) return entry.name === "tests" ? [] : files(path);
		if (!entry.name.endsWith(".rs")) return [];
		if (entry.name === "tests.rs" || entry.name === "test_support.rs" || entry.name === "bench_api.rs" || entry.name === "testing.rs")
			return [];
		return [path];
	});
}

/// A file may not use an `#[allow]` without saying why. The check looks at the whole attribute, including a multi-line
/// one, because the reason is usually on its own line.
function unexplainedAllow(text: string): boolean {
	for (const match of text.matchAll(/#\[allow\(/g)) {
		const start = match.index;
		const end = text.indexOf(")]", start);
		if (end === -1) continue;
		if (!text.slice(start, end).includes("reason =")) return true;
	}
	return false;
}

export function policyProblems(root: string): string[] {
	const problems: string[] = [];
	for (const path of files(root)) {
		const text = production(readFileSync(path, "utf8"));
		const name = relative(root, path);
		for (const [pattern, label] of FORBIDDEN) {
			if (pattern.test(text)) problems.push(`${name}: uses ${label} outside a test`);
		}
		if (unexplainedAllow(text)) problems.push(`${name}: #[allow] has no reason =`);
	}
	return problems;
}

if (import.meta.main) {
	const problems = policyProblems(APP);
	if (problems.length > 0) {
		process.stderr.write(`${problems.join("\n")}\n`);
		process.exit(1);
	}
	process.stdout.write("Agent policy passed.\n");
}
