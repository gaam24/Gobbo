import { useCallback, useEffect, useState } from "react";

import type { Status } from "@/bindings";
import { commands } from "@/bindings";
import { messageOf, unwrap } from "@/lib/ipc";
import { applyStatus } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";
import { useTauriListener } from "@/lib/use-tauri-listener";

/// How often the mod list checks whether the game runs.
const RUNNING_CHECK_MS = 2000;

/// The workspace keeps a status per game, not just for the one on screen. `merge` writes a fresh status to its own game
/// and returns the list to draw; a stale answer is dropped and a patch without a base is ignored. Pure and free of Tauri
/// calls so the "status is a read, sync_profiles is the change" rule can be tested on its own.
export function createStatusCache() {
	const byGame = new Map<string, Status>();
	return {
		get(gameId: string): Status | null {
			return byGame.get(gameId) ?? null;
		},
		merge(gameId: string, next: Status): Status | null {
			const current = byGame.get(gameId);

			if (current) {
				if (next.build < current.build) return null;
				const merged = applyStatus(current, next);
				byGame.set(gameId, merged);
				return merged;
			}

			if (next.patch) return null;
			byGame.set(gameId, next);
			return next;
		},
	};
}

/// The state of the active game: its status (the mod list, profiles, problems) and whether the game
/// runs right now. Switching the game seeds the previous state from the workspace's cache during render (the
/// React-recommended way) and refreshes it from the backend in an effect. `statusOf` and `onStatus` are optional so
/// callers without a cache keep the old reset-and-fetch behavior.
export function useGameState(
	id: string | null,
	pollRunning: boolean,
	onError: (message: string) => void,
	statusOf?: (gameId: string) => Status | null,
	onStatus?: (gameId: string, status: Status) => void,
) {
	const [status, setStatus] = useState<Status | null>(null);
	const [statusFailure, setStatusFailure] = useState<unknown>(null);
	const [retryCount, setRetryCount] = useState(0);
	const retry = useCallback(() => {
		setStatusFailure(null);
		setRetryCount((count) => count + 1);
	}, []);

	/// State belongs to one game, so a switch keeps the previous slot in the cache and seeds this one from it. The
	/// backend refresh below still runs, so a stale cache never becomes the source of truth.
	const [prevGameId, setPrevGameId] = useState(id);

	if (id !== prevGameId) {
		setPrevGameId(id);
		setStatus(id ? (statusOf?.(id) ?? null) : null);
		setStatusFailure(null);
	}

	/// The fetch does not go through the app's `attempt`, because that clears the message, and a game
	/// change after adding should keep its own message.
	useEffect(() => {
		if (!id) return;
		let active = true;
		void (async () => {
			try {
				const next = await unwrap(commands.syncProfiles(id));
				// The user may have switched games while the answer was in flight.
				// oxlint-disable-next-line typescript/no-unnecessary-condition -- the cleanup flips `active`, which flow analysis cannot see.
				if (active) {
					// A retry can cross a mutation; the newer list stays.
					setStatus((previous) => (previous && next.build < previous.build ? previous : next));
					// Keep the cache current while this game is shown, so a later switch starts from its newest list.
					onStatus?.(id, next);
					setStatusFailure(null);
				}
			} catch (error) {
				// oxlint-disable-next-line typescript/no-unnecessary-condition -- the cleanup flips `active`.
				if (active) {
					setStatusFailure(error);
					onError(messageOf(error));
				}
			}
		})();
		return () => {
			active = false;
		};
		// oxlint-disable-next-line react/exhaustive-effect-dependencies -- retryCount is the trigger this effect reads only through the dependency.
	}, [id, onError, onStatus, retryCount]);

	/// Whether the game runs, checked while the mod list is open and the window is visible. Gobbo and Steam both start
	/// it, and the process list shows it on Linux and on Windows. A failed check keeps the last answer.
	/// A game that ends sends its id. The poll stays as a fallback for a game started outside Gobbo or an event that
	/// was missed, but a game Gobbo started no longer waits up to two seconds to show as closed.
	const [runningTick, setRunningTick] = useState(0);
	// A browser preview has no events; the poll still answers.
	useTauriListener(async (scope) => {
		if (!id) return;
		const unlisten = await scope.listen<string>("game-exited", (event) => {
			if (event.payload === id) setRunningTick((tick) => tick + 1);
		});
		scope.add(unlisten);
	}, id);

	const running = useLoad(id && pollRunning ? id : null, () => unwrap(commands.gameRunning(id ?? "")), {
		pollMs: RUNNING_CHECK_MS,
		reloadKey: runningTick,
	});

	return {
		status,
		setStatus,
		gameRunning: running ?? false,
		statusError: statusFailure,
		retryStatus: retry,
	};
}
