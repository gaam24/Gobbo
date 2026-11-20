import type { DeployProgress, UpdateAllProgress, UpdateProgress } from "@/bindings";
import { createStore, useStore } from "@/lib/external-store";
import { createQueue } from "@/lib/serial-queue";

/// One independent serial queue per game. Actions of one game must never wait for a long action of another game, but
/// two edits of the same game still must not overwrite each other. `idle()` with no id waits every queue; with an id it
/// waits only that game's. Pure and free of Tauri calls so the selection rule can be tested on its own.
export function createGameQueues() {
	const queues = new Map<string, ReturnType<typeof createQueue>>();
	const forGame = (gameId: string) => {
		let queue = queues.get(gameId);

		if (!queue) {
			queue = createQueue();
			queues.set(gameId, queue);
		}

		return queue;
	};
	const run = (action: () => Promise<unknown>, gameId: string) => forGame(gameId).run(action);
	const idle = async (gameId?: string) => {
		if (gameId !== undefined) {
			await forGame(gameId).idle();
			return;
		}

		await Promise.all([...queues.values()].map((queue) => queue.idle()));
	};
	return { forGame, run, idle };
}

/// One archive of the import queue, as the activity card shows it.
export interface ImportState {
	stage: "downloading" | "hashing" | "extracting" | "copying";
	percent: number;
	name: string;
	index: number;
	total: number;
	/// Stops an import that supports it, such as a pack between batches.
	onCancel?: () => void;
}

export interface ActivityProgress {
	deploy: DeployProgress | null;
	update: UpdateProgress | null;
	updateAll: UpdateAllProgress | null;
	/// Stops the bulk update, when one is running.
	updateAllCancel: (() => void) | null;
	importing: ReadonlyMap<string, ImportState>;
}

const EMPTY: ActivityProgress = {
	deploy: null,
	update: null,
	updateAll: null,
	updateAllCancel: null,
	importing: new Map(),
};

/// Progress belongs to a game, not to the window: a long command captures its game id before the first await and
/// passes it to every setter, so switching games cannot write its progress into the game that happens to be shown.
/// The store below holds only the visible game's slot; the map keeps every other game's work alive.
const byGame = new Map<string, ActivityProgress>();
let activeGame: string | undefined;

const keyOf = (gameId?: string) => gameId ?? activeGame ?? "";
const slotOf = (key: string) => byGame.get(key) ?? EMPTY;

const store = createStore<ActivityProgress>(EMPTY);

function write(key: string, next: ActivityProgress) {
	byGame.set(key, next);
	// Only the visible game's slot drives the activity card. A background game changing progress does not re-render it.
	if (key === keyOf()) store.set(next);
}

/// Chooses the game whose progress `useProgress` and `progressSnapshot` read. The workspace calls it when the shown
/// game changes. Setters still take an explicit game id for work that outlives a switch; this only selects the view.
export function setActiveGame(gameId: string | undefined) {
	if (activeGame === gameId) return;
	activeGame = gameId;
	store.set(slotOf(keyOf()));
}

/// The optional `gameId` is the last parameter of every setter so existing callers keep working. Omitting it writes to
/// the game selected by `setActiveGame`; a long command must pass its captured id instead.
export function setDeployProgress(value: DeployProgress | null, gameId?: string) {
	const key = keyOf(gameId);
	const current = slotOf(key);

	if (current.deploy === value) return;
	write(key, { ...current, deploy: value });
}

export function setUpdateProgress(value: UpdateProgress | null, gameId?: string) {
	const key = keyOf(gameId);
	const current = slotOf(key);

	if (current.update === value) return;
	write(key, { ...current, update: value });
}

export function setUpdateAllProgress(value: UpdateAllProgress | null, gameId?: string) {
	const key = keyOf(gameId);
	const current = slotOf(key);

	if (current.updateAll === value) return;
	write(key, { ...current, updateAll: value });
}

/// Sets or clears the action that stops the bulk update. It is separate from the progress, so the first progress
/// event does not need to carry it and the last one does not remove it before the run returns.
export function setUpdateAllCancel(value: (() => void) | null, gameId?: string) {
	const key = keyOf(gameId);
	const current = slotOf(key);

	if (current.updateAllCancel === value) return;
	write(key, { ...current, updateAllCancel: value });
}

/// Adds, updates or removes one archive of the import queue. The map is copied only when it changes.
export function setImportProgress(key: string, state: ImportState | null, gameId?: string) {
	const slotKey = keyOf(gameId);
	const current = slotOf(slotKey);
	const next = new Map(current.importing);

	if (state) next.set(key, state);
	else if (!next.delete(key)) return;
	write(slotKey, { ...current, importing: next });
}

/// Clears one game's progress. With no id it is deliberately a no-op: dropping the previous game's rows on a switch is
/// what made long-running work disappear. An action clears its own slot when it starts and when it ends.
export function resetProgress(gameId?: string) {
	if (gameId === undefined) return;

	if (!byGame.delete(gameId)) return;

	if (gameId === keyOf()) store.set(slotOf(gameId));
}

/// The single subscription point for progress. Screens do not read it, so a percent event re-renders the activity card
/// and nothing else. It shows the visible game's slot; a background game's progress is kept in `byGame` until selected.
export function useProgress(): ActivityProgress {
	return useStore(store);
}

/// The current snapshot of the visible game, for a development or test reader that has no component to render. Pass a
/// game id to the setters (or call `setActiveGame`) before reading it.
export function progressSnapshot(): ActivityProgress {
	return store.get();
}
