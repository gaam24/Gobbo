import { beforeEach, expect, test } from "bun:test";
import {
	createGameQueues,
	progressSnapshot,
	resetProgress,
	setActiveGame,
	setDeployProgress,
	setImportProgress,
	setUpdateAllCancel,
} from "@/lib/progress-store";
import { createStatusCache } from "@/lib/use-game-state";
import { createCheckRegistry } from "@/sources/use-updates";
import { statusFixture } from "@/test/fixtures/status";

const readyStage = { kind: "ready", enabled: 0, total: 0 } as const;

const GAME = "game-a";
const OTHER = "game-b";
const noop = () => undefined;

beforeEach(() => {
	resetProgress(GAME);
	resetProgress(OTHER);
	setActiveGame(GAME);
});

test("a repeated progress value is not stored again, so the activity card keeps the same snapshot", () => {
	const value = { stage: "writing", done: 1, total: 4, percent: 25 } as const;
	setDeployProgress(value, GAME);
	const afterFirst = progressSnapshot();
	setDeployProgress(value, GAME);
	setDeployProgress(value, GAME);
	expect(progressSnapshot()).toBe(afterFirst);
	expect(progressSnapshot().deploy).toBe(value);
});

test("a change of progress produces one new snapshot and setting it back clears it", () => {
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, GAME);
	const afterFirst = progressSnapshot();
	const next = { stage: "writing", done: 2, total: 2, percent: 100 } as const;
	setDeployProgress(next, GAME);
	expect(progressSnapshot()).not.toBe(afterFirst);
	expect(progressSnapshot().deploy).toBe(next);
	const afterSecond = progressSnapshot();
	setDeployProgress(null, GAME);
	expect(progressSnapshot()).not.toBe(afterSecond);
	expect(progressSnapshot().deploy).toBeNull();
});

test("reset without a game id is a no-op, so a game switch cannot wipe another game's work", () => {
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, GAME);
	const before = progressSnapshot();
	resetProgress();
	expect(progressSnapshot()).toBe(before);
	expect(progressSnapshot().deploy).not.toBeNull();
});

test("progress and the bulk-update cancel survive a switch back to the game that started them", () => {
	const cancel = noop;
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, GAME);
	setUpdateAllCancel(cancel, GAME);

	setActiveGame(OTHER);
	expect(progressSnapshot().deploy).toBeNull();
	expect(progressSnapshot().updateAllCancel).toBeNull();

	setActiveGame(GAME);
	expect(progressSnapshot().deploy?.percent).toBe(50);
	expect(progressSnapshot().updateAllCancel).toBe(cancel);
});

test("a background game's progress writes to its own slot without changing the visible snapshot", () => {
	setActiveGame(OTHER);
	const before = progressSnapshot();
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, GAME);
	expect(progressSnapshot()).toBe(before);
	expect(progressSnapshot().deploy).toBeNull();

	setActiveGame(GAME);
	expect(progressSnapshot().deploy?.percent).toBe(50);
});

test("an import entry is copied only when it changes", () => {
	const state = {
		stage: "hashing",
		percent: 10,
		name: "a.zip",
		index: 0,
		total: 1,
	} as const;
	setImportProgress("a.zip", state, GAME);
	const afterFirst = progressSnapshot();
	setImportProgress("a.zip", { ...state }, GAME);
	expect(progressSnapshot()).not.toBe(afterFirst);
	const afterSecond = progressSnapshot();
	setImportProgress("missing.zip", null, GAME);
	expect(progressSnapshot()).toBe(afterSecond);
});

test("the import map of one game survives another game's switch and is cleared only for its own game", () => {
	const state = { stage: "copying", percent: 90, name: "a.zip", index: 1, total: 1 } as const;
	setImportProgress("a.zip", state, GAME);
	setActiveGame(OTHER);
	expect(progressSnapshot().importing.size).toBe(0);

	setActiveGame(GAME);
	expect(progressSnapshot().importing.get("a.zip")?.name).toBe("a.zip");
	resetProgress(GAME);
	expect(progressSnapshot().importing.size).toBe(0);
});

test("a game's queue does not wait for another game's long action", async () => {
	const queues = createGameQueues();
	const events: string[] = [];
	let release: () => void = noop;
	const gate = new Promise<void>((resolve) => {
		release = resolve;
	});

	const long = queues.run(async () => {
		events.push("a:start");
		await gate;
		events.push("a:end");
	}, "game-a");
	const other = queues.run(async () => {
		events.push("b:done");
		await Promise.resolve();
	}, "game-b");

	// This settles while game A's action is still waiting: a slow edit of one game cannot block the other.
	await other;
	expect(events).toEqual(["a:start", "b:done"]);
	release();
	await long;
	expect(events).toEqual(["a:start", "b:done", "a:end"]);
});

test("two actions of the same game stay serialized and idle waits only that game", async () => {
	const queues = createGameQueues();
	const events: string[] = [];
	let release: () => void = noop;
	const gate = new Promise<void>((resolve) => {
		release = resolve;
	});

	const first = queues.run(async () => {
		events.push("first:start");
		await gate;
		events.push("first:end");
	}, "game-a");
	const second = queues.run(async () => {
		events.push("second");
		await Promise.resolve();
	}, "game-a");
	void queues.run(async () => {
		events.push("b:done");
		await Promise.resolve();
	}, "game-b");

	await queues.idle("game-b");
	expect(events).toEqual(["first:start", "b:done"]);
	release();
	await Promise.all([first, second, queues.idle("game-a")]);
	expect(events).toEqual(["first:start", "b:done", "first:end", "second"]);
});

test("a status written for a non-shown game is kept for that game alone", () => {
	const cache = createStatusCache();
	const a = statusFixture(readyStage, { build: 1, game_name: "A" });
	const b = statusFixture(readyStage, { build: 1, game_name: "B" });
	cache.merge("game-a", a);
	cache.merge("game-b", b);

	// A queued action of a background game still finds its base status while another game is shown.
	expect(cache.get("game-a")?.game_name).toBe("A");
	expect(cache.get("game-b")?.game_name).toBe("B");
	expect(cache.get("missing")).toBeNull();
});

test("a stale status answer and a patch without a base are dropped", () => {
	const cache = createStatusCache();
	const newer = statusFixture(readyStage, { build: 5 });
	const older = statusFixture(readyStage, { build: 4 });
	cache.merge("game-a", newer);

	expect(cache.merge("game-a", older)).toBeNull();
	expect(cache.get("game-a")?.build).toBe(5);
	expect(cache.merge("game-b", statusFixture(readyStage, { build: 1, patch: true }))).toBeNull();
	expect(cache.get("game-b")).toBeNull();
});

test("update checks of two games run side by side while a duplicate check of one is refused", () => {
	const checks = createCheckRegistry();
	expect(checks.start("game-a")).toBe(true);
	expect(checks.start("game-a")).toBe(false);
	expect(checks.start("game-b")).toBe(true);
	expect(checks.list()).toEqual(["game-b", "game-a"]);
	expect(checks.active()).toBe("game-b");

	checks.finish("game-b");
	expect(checks.active()).toBe("game-a");
	checks.finish("game-a");
	expect(checks.active()).toBeNull();
	// A failure clears its game's slot, so the check can be started again.
	expect(checks.start("game-a")).toBe(true);
});
