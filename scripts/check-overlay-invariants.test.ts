import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { overlayProblems } from "./check-overlay-invariants";

function scan(hook: string, app: string): string[] {
	const root = mkdtempSync(join(tmpdir(), "gobbo-overlay-check-"));

	try {
		mkdirSync(join(root, "hook"), { recursive: true });
		mkdirSync(join(root, "app"), { recursive: true });
		writeFileSync(join(root, "hook", "lib.rs"), hook);
		writeFileSync(join(root, "app", "a.rs"), app);
		return overlayProblems(join(root, "hook"), join(root, "app"), root);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
}

test("DllMain may only store the module and disable thread calls", () => {
	expect(scan("fn DllMain() { remember(); DisableThreadLibraryCalls(); }\n", "")).toEqual([]);
	expect(scan('fn DllMain() { println!("no"); }\n', "")).toEqual([
		"overlay-hook/src/lib.rs: DllMain contains println!",
		"hook/lib.rs: debug print in the hook",
	]);
});

test("the app crate has no unsafe block", () => {
	expect(scan("fn DllMain() {}\n", "unsafe { }")).toEqual(["app/a.rs: unsafe in the app crate"]);
});
