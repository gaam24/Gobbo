import { expect, test } from "bun:test";

import { needsGameSwitch } from "@/lib/task-open";

test("opening a task of the active game shows its dialog directly", () => {
	expect(needsGameSwitch("game-one", "game-one")).toBe(false);
});

test("opening a task of another game switches to that game first", () => {
	expect(needsGameSwitch("game-two", "game-one")).toBe(true);
});

test("opening a task without an active game switches first", () => {
	expect(needsGameSwitch("game-one", undefined)).toBe(true);
});
