import { useCallback, useEffect, useRef } from "react";
import { commands, type Status } from "@/bindings";
import { unwrap } from "@/lib/ipc";

/// The provider page of each mod that never had it (name, author, category, description, picture), read in the
/// background: after every import, and once per game in a run when its list opens. One request per mod, once. Reads of
/// one game wait for each other, so a mod is never asked for twice.
export function useSourceFill({
	enabled,
	activeGameId,
	setStatusFor,
	queueProfile,
}: {
	/// Without the provider key there is no page data to read.
	enabled: boolean;
	activeGameId: string | null;
	setStatusFor: (gameId: string, status: Status) => boolean;
	/// The fresh status joins the profile edit queue of the game it belongs to, so it never lands over a newer edit
	/// and a fill of one game does not wait for another game's queue. The id is optional for existing callers.
	queueProfile: (action: () => Promise<unknown>, gameId?: string) => Promise<void>;
}): (gameId: string) => Promise<boolean> {
	const fills = useRef(new Map<string, Promise<unknown>>());
	const filledGames = useRef(new Set<string>());

	const fillSourceInfo = useCallback(
		(gameId: string) => {
			if (!enabled) return Promise.resolve(false);
			const run = async () => {
				try {
					const filled = await unwrap(commands.fillSourceInfo(gameId));
					if (filled === 0) return true;
					await queueProfile(async () => {
						const fresh = await unwrap(commands.getStatus(gameId));
						setStatusFor(gameId, fresh);
					}, gameId);
					return true;
				} catch {
					// The page data only adds to what the archive says; without it the mod still works. It is retried on the
					// next import or the next time this game is opened.
					return false;
				}
			};
			const previous = fills.current.get(gameId);
			const next = (async () => {
				await previous;
				return run();
			})();
			fills.current.set(gameId, next);
			return next;
		},
		[enabled, queueProfile, setStatusFor],
	);

	/// One fill per game in a run, as soon as its list can be shown with a key. A failure removes the mark, so opening
	/// the game again or importing another archive retries it.
	useEffect(() => {
		if (!(activeGameId && enabled) || filledGames.current.has(activeGameId)) return;
		filledGames.current.add(activeGameId);
		void (async () => {
			const filled = await fillSourceInfo(activeGameId);
			if (!filled) filledGames.current.delete(activeGameId);
		})();
	}, [activeGameId, enabled, fillSourceInfo]);

	return fillSourceInfo;
}
