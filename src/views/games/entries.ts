import type { GameScan, GameView, ManagedGame } from "@/bindings";

/// Where a supported game stands. `searching` lasts only while the libraries are being scanned; a failed scan leaves
/// the games `unsearched` instead of pretending the search is still running.
export type GameStatus = "added" | "found" | "missing" | "searching" | "unsearched";

/// The order of the groups in the list.
export const GAME_STATUSES: GameStatus[] = ["added", "found", "missing", "unsearched", "searching"];

/// The filter above the list. `all` shows every status; the others show one.
export type GameFilter = "all" | "added" | "found" | "missing";

export interface GameEntry {
	game: GameView;
	status: GameStatus;
	/// The game folder: the added one, or the one the scan found. `null` when there is none.
	root: string | null;
	/// Whether an added game still has its folder. Always `false` for games that are not added.
	ready: boolean;
}

/// Columns of a game row: icon, name, status, folder, action. The header row uses the same grid.
export const GAME_ROW_GRID = "grid grid-cols-[40px_minmax(0,1fr)_minmax(0,1.4fr)_120px] items-center gap-4 px-6";

/// Joins the supported games with what the app knows about them, so each game has exactly one status.
export function buildGameEntries(games: GameView[], managed: ManagedGame[], scans: GameScan[] | null, scanFailed = false): GameEntry[] {
	return games.map((game) => {
		const added = managed.find((item) => item.id === game.id);
		if (added) return { game, status: "added", root: added.game_root || null, ready: added.ready };
		if (scans === null) return { game, status: scanFailed ? "unsearched" : "searching", root: null, ready: false };
		const scan = scans.find((item) => item.id === game.id);
		if (scan?.found) return { game, status: "found", root: scan.found.game_root, ready: false };
		return { game, status: "missing", root: null, ready: false };
	});
}

/// The entries that pass the status filter and the search text. The text matches the name and the id, in lower case.
export function filterGameEntries(entries: GameEntry[], filter: GameFilter, query: string): GameEntry[] {
	const needle = query.trim().toLowerCase();
	return entries.filter(
		(entry) =>
			(filter === "all" || entry.status === filter) &&
			(needle === "" || entry.game.name.toLowerCase().includes(needle) || entry.game.id.includes(needle)),
	);
}
