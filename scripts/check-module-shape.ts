import { readFileSync } from "node:fs";
import { join } from "node:path";
import { portableName, rustFiles } from "./rust-sources";

const ROOT = join(import.meta.dir, "../src-tauri/src");
const MAX_MOD_LINES = 400;

/// An inline test module: `#[cfg(test)]`, optional attributes such as `#[allow(...)]`, then `mod tests`. A test-only
/// re-export such as `#[cfg(test)] pub mod testing;` is not a test module and does not end production code.
const INLINE_TESTS = /#\[cfg\(test\)\](?:\s*#\[[^\]]*\])*\s*mod tests\b/;

/// Test-only files carry fixtures and helpers rather than production modules.
function isTestOnly(name: string): boolean {
	return name === "tests.rs" || name === "test_support.rs" || name === "bench_api.rs";
}

/// The number of lines in `text`, without counting a trailing newline as an extra empty line.
function lineCount(text: string): number {
	if (text.length === 0) return 0;
	const lines = text.split("\n");
	return lines[lines.length - 1] === "" ? lines.length - 1 : lines.length;
}

/// Production files over the line limit, including `mod.rs`, with the limit in each message.
export function shapeProblems(root: string): string[] {
	const problems: string[] = [];

	for (const path of rustFiles(root, isTestOnly)) {
		const name = portableName(root, path);
		const source = readFileSync(path, "utf8");
		const inlineTests = INLINE_TESTS.exec(source);
		const production = inlineTests === null ? source : source.slice(0, inlineTests.index);
		const lines = lineCount(production);

		if (lines > MAX_MOD_LINES) {
			problems.push(`${name}: ${lines} production lines (limit ${MAX_MOD_LINES})`);
		}
	}

	return problems;
}

if (import.meta.main) {
	const problems = shapeProblems(ROOT);

	if (problems.length > 0) {
		process.stderr.write(`${problems.join("\n")}\n`);
		process.exit(1);
	}

	process.stdout.write("Module shape passed.\n");
}
