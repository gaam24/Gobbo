import { expect, test } from "bun:test";

import { localFailedTasks, registryDedupeKey, updateTasks } from "@/app/hooks/use-activity";
import type { TaskDedupe } from "@/bindings";
import { dedupeKey } from "@/lib/tasks";
import type { RegisteredTask } from "@/lib/tasks-store";

// A deterministic translator: the translated title is asserted by key, not by a locale-dependent string.
const t = (key: string): string => key;

const GAME = "game-a";

/// A registry row with the typed identity a task builder puts on a backend row. The frontend registry does not own the
/// field yet, so the fixture adds it to the shared shape instead of relying on an encoded id.
type TypedTask = RegisteredTask & { dedupe?: TaskDedupe };

function local(id: string, dedupe?: TaskDedupe): TypedTask {
	return {
		id,
		game: GAME,
		kind: "update",
		title: "Local",
		state: "running",
		source: "local",
		dedupe,
	};
}

function backend(id: string, kind: string, dedupe: TaskDedupe): TypedTask {
	return {
		id,
		game: GAME,
		kind: "update",
		title: "Backend",
		state: "running",
		source: "backend",
		sourceKind: kind,
		dedupe,
	};
}

test("a backend lifecycle row and the local row of one operation share a dedupe group", () => {
	const cases: Array<[TaskDedupe, string, string]> = [
		[{ Apply: { game: GAME } }, "apply:game-a", "apply"],
		[{ Import: { game: GAME, path: "/mods/a.zip" } }, "import:game-a\n/mods/a.zip", "import"],
		[{ Update: { game: GAME } }, "update-mod:game-a\nCET", "update"],
		[{ UpdateAll: { game: GAME } }, "update:game-a", "update-all"],
		[{ Requirement: { game: GAME } }, "requirement:game-a", "requirement"],
		[{ Collection: { game: GAME, profile: "main", collection: "x" } }, "collection:game-a\nmain\nx", "collection"],
		[{ Pack: { game: GAME, path: "recipe.zip" } }, "pack:game-a\nrecipe.zip", "pack"],
		[{ PackLink: { game: GAME, link: "nxm://link" } }, "pack-link:game-a\nnxm://link", "pack-link"],
	];

	for (const [dedupe, id, kind] of cases) {
		const fromBackend = registryDedupeKey(backend(id, kind, dedupe));
		// The typed identity decides the group, not the id the row happens to carry.
		const fromLocal = registryDedupeKey(local("a-different-id", dedupe));
		expect(fromBackend).toBe(fromLocal);
		expect(fromBackend).not.toBe("");
	}

	const apply = registryDedupeKey(local("any", { Apply: { game: GAME } }));
	const update = registryDedupeKey(local("any", { Update: { game: GAME } }));
	expect(apply).not.toBe(update);
});

test("a failed import row shares its group with the backend row of the same import", () => {
	const path = "/mods/a.zip";
	const importIdentity: TaskDedupe = { Import: { game: GAME, path } };
	const [row] = localFailedTasks(
		[
			{
				id: `import:${path}`,
				kind: "import",
				game: GAME,
				title: "Could not import a.zip",
				dedupeKey: dedupeKey(importIdentity),
			},
		],
		[{ id: GAME, name: "Game A" }],
	);

	// The rich local failure wins over the generic backend row, so Working shows one "Needs attention" row.
	expect(row?.dedupeKey).toBe(registryDedupeKey(backend(`import:${GAME}\n${path}`, "import", importIdentity)));
	expect(row?.state).toBe("failed");
	expect(row?.gameName).toBe("Game A");
});

test("a failure without an identity keeps its id as its group", () => {
	const [row] = localFailedTasks(
		[
			{
				id: "import:/mods/a.zip",
				kind: "import",
				title: "Could not import a.zip",
			},
		],
		[],
	);
	expect(row?.dedupeKey).toBeUndefined();
});

test("a pack import and its download share one group although their ids differ", () => {
	const dedupe: TaskDedupe = { Pack: { game: GAME, path: "recipe.zip" } };
	const fromImport = registryDedupeKey(local("pack-import:game-a|recipe.zip", dedupe));
	const fromDownload = registryDedupeKey(local("pack-download:game-a|recipe.zip", dedupe));

	expect(fromImport).toBe(fromDownload);
});

test("rows without a typed identity keep the id rule so older journals still merge", () => {
	expect(registryDedupeKey(local("manual-update:game-a"))).toBe("manual-update:game-a");
	expect(registryDedupeKey(local("import-wizard:game-a"))).toBe("import-wizard:game-a");
	expect(registryDedupeKey(backend("apply:game-a", "apply", "Other"))).toBe(registryDedupeKey(local("apply:game-a")));
	expect(registryDedupeKey(backend("collection:game-a\nmain\nx", "collection", "Other"))).toBe(
		registryDedupeKey(local("collection:game-a|main|x")),
	);
});

test("each game being checked produces its own update row", () => {
	// Generic games keep the rule universal: two concurrent checks must not collapse into one row.
	const games = [
		{ id: "game-a", name: "Game A" },
		{ id: "game-b", name: "Game B" },
	];
	const rows = updateTasks(["game-a", "game-b"], games, t);

	expect(rows).toHaveLength(2);
	expect(rows.map((row) => row.dedupeKey)).toEqual(["checking:game-a", "checking:game-b"]);
	expect(rows.map((row) => row.game)).toEqual(["game-a", "game-b"]);
	expect(rows.map((row) => row.gameName)).toEqual(["Game A", "Game B"]);
	expect(rows.map((row) => row.detail)).toEqual(["Game A", "Game B"]);
	expect(rows.every((row) => row.title === "tasks.checkingUpdates" && row.state === "running")).toBe(true);
});

test("an unlisted checking game falls back to its id, and no checks produce no rows", () => {
	const rows = updateTasks(["game-missing"], [], t);

	expect(rows).toHaveLength(1);
	expect(rows[0]?.dedupeKey).toBe("checking:game-missing");
	expect(rows[0]?.gameName).toBe("game-missing");
	expect(updateTasks([], [], t)).toEqual([]);
});
