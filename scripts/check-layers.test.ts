import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { allowed, layerProblems } from "./check-layers";

test("a lower layer cannot use a higher layer or a feature module", () => {
	expect(allowed("domain", "context")).toBe(false);
	expect(allowed("domain", "infra")).toBe(false);
	expect(allowed("infra", "domain")).toBe(true);
	expect(allowed("platform", "providers")).toBe(false);
	expect(allowed("context", "games")).toBe(true);
	expect(allowed("context", "migrations")).toBe(true);
	expect(allowed("context", "launch")).toBe(false);
});

test("the scanner names the file and the forbidden module", () => {
	const root = mkdtempSync(join(tmpdir(), "gobbo-layer-"));
	try {
		mkdirSync(join(root, "domain"));
		writeFileSync(join(root, "domain", "bad.rs"), "use crate::context::Data;\n");
		mkdirSync(join(root, "infra"));
		writeFileSync(join(root, "infra", "good.rs"), "use crate::domain::pathkey;\n");
		expect(layerProblems(root)).toEqual(["domain/bad.rs: domain must not use crate::context"]);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});
