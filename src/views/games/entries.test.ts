import { expect, test } from "bun:test";

import type { GameScan, GameView, ManagedGame } from "@/bindings";
import { buildGameEntries, filterGameEntries } from "@/views/games/entries";

function game(id: string, name: string): GameView {
	return {
		id,
		name,
		origin: "bundled",
		verified: null,
		steam_app_ids: [],
		places: [],
		loader: null,
		needs_settings_dir: false,
		needs_mods_root: false,
		providers: [],
	};
}

function scan(id: string, root: string | null): GameScan {
	return {
		id,
		found: root === null ? null : { game_root: root, library: "/library/steam", store: "steam" },
	};
}

const managed: ManagedGame[] = [{ id: "added", game_root: "/games/added", ready: true }];

test("an added game wins over a scan result and keeps its folder", () => {
	const added = game("added", "Added");
	const entries = buildGameEntries([added], managed, [scan("added", "/elsewhere")]);
	expect(entries[0]).toEqual({
		game: added,
		status: "added",
		root: "/games/added",
		ready: true,
	});
});

test("a game without an answer yet is searching, then found or missing", () => {
	const found = game("found", "Found");
	const missing = game("missing", "Missing");
	expect(buildGameEntries([found, missing], [], null).map((entry) => entry.status)).toEqual(["searching", "searching"]);

	const entries = buildGameEntries([found, missing], [], [scan("found", "/games/found"), scan("missing", null)]);
	expect(entries).toEqual([
		{ game: found, status: "found", root: "/games/found", ready: false },
		{ game: missing, status: "missing", root: null, ready: false },
	]);
});

test("a failed scan leaves games unsearched, not searching forever", () => {
	const entries = buildGameEntries([game("found", "Found"), game("missing", "Missing")], [], null, true);
	expect(entries.map((entry) => entry.status)).toEqual(["unsearched", "unsearched"]);
});

test("the filter matches the status, the name and the id without case", () => {
	const entries = buildGameEntries([game("alpha", "Alpha Quest"), game("beta", "Beta Tale 2077")], [], null);
	expect(filterGameEntries(entries, "all", "ALPHA").map((entry) => entry.game.id)).toEqual(["alpha"]);
	expect(filterGameEntries(entries, "all", "2077").map((entry) => entry.game.id)).toEqual(["beta"]);
	expect(filterGameEntries(entries, "all", "").every((entry) => entry.status === "searching")).toBe(true);
	expect(filterGameEntries(entries, "added", "")).toHaveLength(0);
});
