import { expect, test } from "bun:test";

import { dedupeKey, groupedTasks, type Task, type TaskGroupLabelKey, taskAnnouncement, taskCounts } from "@/lib/tasks";

function task(id: string, state: Task["state"]): Task {
	return { id, kind: "import", title: id, state };
}

test("a typed identity gives every workflow the key its local and backend rows share", () => {
	const game = "game-a";
	expect(dedupeKey({ Apply: { game } }, "apply:game-a")).toBe("apply:game-a");
	expect(dedupeKey({ Import: { game, path: "/mods/a.zip" } }, "import:game-a\n/mods/a.zip")).toBe("import:game-a:/mods/a.zip");
	expect(dedupeKey({ Update: { game } }, "update-mod:game-a\nCET")).toBe("update:game-a");
	expect(dedupeKey({ UpdateAll: { game } }, "update:game-a")).toBe("update-all:game-a");
	expect(dedupeKey({ Requirement: { game } }, "requirement:game-a")).toBe("requirement:game-a");
	expect(dedupeKey({ Collection: { game, profile: "main", collection: "x" } }, "collection:game-a\nmain\nx")).toBe(
		"collection:game-a:main:x",
	);
	expect(dedupeKey({ Pack: { game, path: "recipe.zip" } }, "pack:game-a\nrecipe.zip")).toBe("pack:game-a:recipe.zip");
	// A download and an import of the same file share a group although their ids differ.
	expect(dedupeKey({ Pack: { game, path: "recipe.zip" } }, "pack-download:game-a\nrecipe.zip")).toBe("pack:game-a:recipe.zip");
	expect(dedupeKey({ PackLink: { game, link: "nxm://link" } }, "pack-link:game-a\nnxm://link")).toBe("pack-link:game-a:nxm://link");
});

test("a typed identity round-trips with the local and backend ids of the same operation", () => {
	const game = "game-a";
	const pairs = [
		["apply:game-a", { Apply: { game } }, "apply:game-a"],
		["collection:game-a|main|x", { Collection: { game, profile: "main", collection: "x" } }, "collection:game-a\nmain\nx"],
		["pack-import:game-a|recipe.zip", { Pack: { game, path: "recipe.zip" } }, "pack:game-a\nrecipe.zip"],
		["pack-download:game-a|recipe.zip", { Pack: { game, path: "recipe.zip" } }, "pack-download:game-a\nrecipe.zip"],
		["pack-link:game-a|nxm://link", { PackLink: { game, link: "nxm://link" } }, "pack-link:game-a\nnxm://link"],
	] as const;

	for (const [localId, dedupe, backendId] of pairs) {
		expect(dedupeKey("Other", localId)).toBe(dedupeKey(dedupe, backendId));
		expect(dedupeKey("Other", backendId, true, "backend")).toBe(dedupeKey(dedupe, backendId));
	}
});

const label = (key: TaskGroupLabelKey): string => key;

function grouped(): Task[] {
	return [
		{ id: "b", kind: "import", title: "Beta", state: "running", game: "game-b", gameName: "Game B" },
		{ id: "a", kind: "import", title: "Alpha", state: "failed", game: "game-a", gameName: "Game A" },
		{ id: "c", kind: "import", title: "Alpha", state: "running", game: "game-a", gameName: "Game A" },
		{ id: "d", kind: "import", title: "Zeta", state: "waiting", game: null },
		{ id: "e", kind: "import", title: "Gamma", state: "failed", game: "game-a", gameName: "Game A" },
		{ id: "f", kind: "import", title: "Delta", state: "running", game: null },
	];
}

test("the card groups by game and orders each game's rows by state, then title", () => {
	const games = groupedTasks(grouped());

	// Global work comes before named games, then games by name.
	expect(games.map((group) => group.game)).toEqual([null, "game-a", "game-b"]);
	expect(games.map((group) => group.tasks.map((row) => row.id))).toEqual([["f", "d"], ["c", "a", "e"], ["b"]]);
	expect(games.map((group) => group.count)).toEqual([2, 3, 1]);
});

test("the grouping does not depend on registration order", () => {
	const tasks = grouped();
	expect(groupedTasks(tasks.toReversed())).toEqual(groupedTasks(tasks));
});

test("equal titles fall back to the id so two rows never swap", () => {
	const tasks: Task[] = [
		{ ...task("b", "running"), title: "Same" },
		{ ...task("a", "running"), title: "Same" },
	];
	expect(groupedTasks(tasks)[0]?.tasks.map((row) => row.id)).toEqual(["a", "b"]);
});

test("an empty list has no groups and a game without a catalogue entry keeps its id", () => {
	expect(groupedTasks([])).toEqual([]);
	const groups = groupedTasks([{ ...task("a", "running"), game: "game-missing" }]);
	expect(groups[0]).toMatchObject({ game: "game-missing", gameName: undefined, count: 1 });
});

test("the counts stay in step with the groups", () => {
	const tasks = grouped();
	expect(taskCounts(tasks)).toEqual({ running: 3, waiting: 1, failed: 2 });
});

test("the live region names only the state that changed, never a progress tick", () => {
	const before = { running: 2, waiting: 0, failed: 1 };

	expect(taskAnnouncement(before, before, label)).toBe("");
	expect(taskAnnouncement(before, { ...before }, label)).toBe("");
	expect(taskAnnouncement(before, { ...before, waiting: 1 }, label)).toBe("tasks.group.waiting: 1");
	expect(taskAnnouncement(before, { ...before, failed: 2 }, label)).toBe("tasks.group.failed: 2");
	expect(taskAnnouncement(before, { ...before, running: 3 }, label)).toBe("tasks.group.running: 3");
	expect(taskAnnouncement(before, { running: 1, waiting: 1, failed: 2 }, label)).toBe("tasks.group.waiting: 1");
});
