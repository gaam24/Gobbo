import { beforeEach, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import {
	type ActivityProgress,
	createGameQueues,
	setActiveGame,
	setDeployProgress,
	setImportProgress,
	setUpdateAllCancel,
	useProgress,
} from "@/lib/progress-store";
import { createStatusCache } from "@/lib/use-game-state";
import { createCheckRegistry } from "@/sources/use-updates";
import { statusFixture } from "@/test/fixtures/status";

const readyStage = { kind: "ready", enabled: 0, total: 0 } as const;

const noop = () => undefined;

/// The store has no reset export; `setActiveGame(undefined)` is its public way to show no game, which restores the empty
/// slot. Each test still writes under its own game id, so a previous test's progress cannot leak into the next one.
beforeEach(() => setActiveGame(undefined));

function ProgressProbe({ observe }: { observe: (value: ActivityProgress) => void }) {
	observe(useProgress());
	return null;
}

/// Reads the visible game's slot through the production subscription, the same one the activity card uses. A server
/// render runs the hook once against the current snapshot, so the probe sees the exact object a component would.
function currentProgress(): ActivityProgress {
	const seen: ActivityProgress[] = [];
	renderToString(createElement(ProgressProbe, { observe: (value) => seen.push(value) }));
	const snapshot = seen[0];
	if (snapshot === undefined) throw new Error("the progress hook did not render");
	return snapshot;
}

test("a repeated progress value leaves the visible progress unchanged", () => {
	const game = "t1-game";
	setActiveGame(game);
	const value = { stage: "writing", done: 1, total: 4, percent: 25 } as const;
	setDeployProgress(value, game);
	const afterFirst = currentProgress();
	setDeployProgress(value, game);
	setDeployProgress(value, game);
	expect(currentProgress().deploy?.stage).toBe("writing");
	expect(currentProgress().deploy?.done).toBe(1);
	expect(currentProgress().deploy?.total).toBe(4);
	expect(currentProgress().deploy?.percent).toBe(25);
	// No re-render contract: identical content keeps the snapshot identity the activity card subscribed to.
	expect(currentProgress()).toBe(afterFirst);
});

test("a change of progress updates the visible value and clearing it empties it", () => {
	const game = "t2-game";
	setActiveGame(game);
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, game);
	expect(currentProgress().deploy?.percent).toBe(50);
	const next = { stage: "writing", done: 2, total: 2, percent: 100 } as const;
	setDeployProgress(next, game);
	expect(currentProgress().deploy?.done).toBe(2);
	expect(currentProgress().deploy?.percent).toBe(100);
	setDeployProgress(null, game);
	expect(currentProgress().deploy).toBeNull();
});

test("progress and the bulk-update cancel survive a switch back to the game that started them", () => {
	const game = "t4-game";
	const other = "t4-other";
	setActiveGame(game);
	const cancel = noop;
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, game);
	setUpdateAllCancel(cancel, game);

	setActiveGame(other);
	expect(currentProgress().deploy).toBeNull();
	expect(currentProgress().updateAllCancel).toBeNull();

	setActiveGame(game);
	expect(currentProgress().deploy?.percent).toBe(50);
	expect(currentProgress().updateAllCancel).toBe(cancel);
});

test("a background game's progress writes to its own slot without changing the visible snapshot", () => {
	const game = "t5-game";
	const other = "t5-other";
	setActiveGame(other);
	setDeployProgress({ stage: "writing", done: 1, total: 2, percent: 50 }, game);
	expect(currentProgress().deploy).toBeNull();

	setActiveGame(game);
	expect(currentProgress().deploy?.percent).toBe(50);
});

test("an import entry is copied only when it changes", () => {
	const game = "t6-game";
	setActiveGame(game);
	const state = {
		stage: "hashing",
		percent: 10,
		name: "a.zip",
		index: 0,
		total: 1,
	} as const;
	setImportProgress("a.zip", state, game);
	setImportProgress("a.zip", { ...state }, game);
	expect(currentProgress().importing.get("a.zip")?.name).toBe("a.zip");
	expect(currentProgress().importing.get("a.zip")?.stage).toBe("hashing");
	expect(currentProgress().importing.get("a.zip")?.percent).toBe(10);
	setImportProgress("missing.zip", null, game);
	expect(currentProgress().importing.has("missing.zip")).toBe(false);
	expect(currentProgress().importing.size).toBe(1);
});

test("the import map of one game survives another game's switch", () => {
	const game = "t7-game";
	const other = "t7-other";
	setActiveGame(game);
	const state = {
		stage: "copying",
		percent: 90,
		name: "a.zip",
		index: 1,
		total: 1,
	} as const;
	setImportProgress("a.zip", state, game);
	setActiveGame(other);
	expect(currentProgress().importing.size).toBe(0);

	setActiveGame(game);
	expect(currentProgress().importing.get("a.zip")?.name).toBe("a.zip");
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
