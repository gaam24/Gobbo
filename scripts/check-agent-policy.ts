import { join } from "node:path";
import { productionSources } from "./rust-sources";

const ROOT = join(import.meta.dir, "..");
const APP = join(ROOT, "src-tauri", "src");

/// The Rust files that only serve tests or benchmarks. The walk and the production-text cut come from the shared
/// reader, so this guard sees the same files as the other architecture checks.
const TEST_ONLY = (name: string): boolean =>
	name === "tests.rs" || name === "test_support.rs" || name === "bench_api.rs" || name === "testing.rs";
const FORBIDDEN = [
	[/\.unwrap\(\)/, "unwrap()"],
	[/\.expect\(/, "expect()"],
	[/\bpanic!\(/, "panic!()"],
	[/\btodo!\(/, "todo!()"],
	[/\bunimplemented!\(/, "unimplemented!()"],
	[/\bdbg!\(/, "dbg!()"],
	[/\bprintln!\(/, "println!()"],
] as const;

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

	for (const { name, text } of productionSources(root, TEST_ONLY)) {
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
