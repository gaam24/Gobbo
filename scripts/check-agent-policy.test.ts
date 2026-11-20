import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { policyProblems } from "./check-agent-policy";

function scan(files: Record<string, string>): string[] {
	const root = mkdtempSync(join(tmpdir(), "gobbo-policy-"));

	try {
		for (const [name, text] of Object.entries(files)) {
			const target = join(root, name);
			mkdirSync(join(target, ".."), { recursive: true });
			writeFileSync(target, text);
		}

		return policyProblems(root);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
}

test("production code may not unwrap or panic", () => {
	expect(scan({ "a.rs": "fn f() { let value = g().unwrap(); }\n" })).toEqual(["a.rs: uses unwrap() outside a test"]);
	expect(scan({ "a.rs": 'fn f() { panic!("no"); }\n' })).toEqual(["a.rs: uses panic!() outside a test"]);
});

test("a test module is left alone", () => {
	expect(
		scan({
			"a.rs": "fn f() {}\n#[cfg(test)]\nmod tests { fn t() { g().unwrap(); } }\n",
		}),
	).toEqual([]);
});

test("an allow needs a reason", () => {
	expect(scan({ "a.rs": "#[allow(dead_code)]\nfn f() {}\n" })).toEqual(["a.rs: #[allow] has no reason ="]);
	expect(
		scan({
			"a.rs": '#[allow(dead_code, reason = "a test hook")]\nfn f() {}\n',
		}),
	).toEqual([]);
});
