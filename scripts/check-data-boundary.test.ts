import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { boundaryProblems } from "./check-data-boundary";

function scan(files: Record<string, string>): string[] {
	const root = mkdtempSync(join(tmpdir(), "gobbo-boundary-"));
	try {
		for (const [name, text] of Object.entries(files)) {
			const target = join(root, name);
			mkdirSync(join(target, ".."), { recursive: true });
			writeFileSync(target, text);
		}
		return boundaryProblems(root);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
}

test("a lower layer cannot name a provider", () => {
	expect(scan({ "domain/a.rs": 'const P: &str = "nexus";\n' })).toEqual(["domain/a.rs: names a provider in the domain layer"]);
	expect(scan({ "context/a.rs": 'const P: &str = "nexus";\n' })).toEqual(["context/a.rs: names a provider in the context layer"]);
	expect(scan({ "providers/a.rs": 'const P: &str = "nexus";\n' })).toEqual([]);
});

test("a game or loader id is not a branch outside the data folders", () => {
	expect(scan({ "mods/a.rs": 'if game_id == "slot" {}\n' })).toEqual(["mods/a.rs: branches on a game or loader id"]);
	expect(scan({ "games/a.rs": 'if game_id == "slot" {}\n' })).toEqual([]);
});
