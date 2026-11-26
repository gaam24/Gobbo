import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { shapeProblems } from "./check-module-shape";

function scan(files: Record<string, string>): string[] {
	const root = mkdtempSync(join(tmpdir(), "gobbo-shape-"));

	try {
		for (const [name, text] of Object.entries(files)) {
			const target = join(root, name);
			mkdirSync(join(target, ".."), { recursive: true });
			writeFileSync(target, text);
		}

		return shapeProblems(root);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
}

const lines = (count: number): string => {
	const body = Array.from({ length: count }, (_, index) => `line ${index + 1}`).join("\n");
	return `${body}\n`;
};

test("a file over 400 production lines fails", () => {
	expect(scan({ "games/a.rs": lines(401) })).toEqual(["games/a.rs: 401 production lines (limit 400)"]);
	expect(scan({ "games/a.rs": lines(400) })).toEqual([]);
});

test("a module with an inline test module counts only its production part", () => {
	const text = `${lines(299)}#[cfg(test)]\nmod tests {\n${lines(99)}}\n`;
	expect(scan({ "games/a.rs": text })).toEqual([]);
});

test("a test-only re-export does not cut production lines", () => {
	const text = `${lines(400)}#[cfg(test)]\npub mod testing;\n${lines(1)}`;
	expect(scan({ "games/a.rs": text })).toEqual(["games/a.rs: 403 production lines (limit 400)"]);
});

test("test-only files and folders are ignored", () => {
	expect(
		scan({
			"games/tests.rs": lines(500),
			"games/test_support.rs": lines(500),
			"games/bench_api.rs": lines(500),
			"games/tests/fixture.rs": lines(500),
		}),
	).toEqual([]);
});

test("a mod.rs facade shares the 400-line limit", () => {
	expect(scan({ "games/mod.rs": lines(401) })).toEqual(["games/mod.rs: 401 production lines (limit 400)"]);
	expect(scan({ "games/mod.rs": lines(400) })).toEqual([]);
});
