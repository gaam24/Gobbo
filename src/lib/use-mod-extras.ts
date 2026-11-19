import { useEffect, useRef, useState } from "react";

import { commands, type ModExtras } from "@/bindings";
import { unwrap } from "@/lib/ipc";

/// Keeps at most `limit` cached entries, dropping the ones outside the visible window first. Entries still in view are
/// kept even when there are more than the limit; the next idle moment can trim them.
export function trimCache<T>(cache: Map<string, T>, wanted: ReadonlySet<string>, limit = 256): void {
	for (const folder of cache.keys()) {
		if (cache.size <= limit) break;
		if (!wanted.has(folder)) cache.delete(folder);
	}
}

/// The folders whose extras are not cached yet, in the order the window asked for them.
export function missingFolders<T>(cache: ReadonlyMap<string, T>, folders: readonly string[]): string[] {
	return folders.filter((folder) => !cache.has(folder));
}

/// The page text and relation data of the rows in the virtual window. The list row keeps only what it draws, so this
/// asks for the rest only while a column that needs it is visible. `token` is the status list: a note, a tag or a
/// relation edit changes extras, and the cache is dropped when the list is rebuilt.
export function useModExtras(gameId: string, token: unknown, folders: readonly string[]): ReadonlyMap<string, ModExtras> {
	const [loaded, setLoaded] = useState<ReadonlyMap<string, ModExtras>>(() => new Map());
	const cache = useRef(new Map<string, ModExtras>());
	const tokenRef = useRef(token);

	useEffect(() => {
		if (tokenRef.current !== token) {
			tokenRef.current = token;
			cache.current = new Map();
			setLoaded(new Map());
		}
		// The extras are only needed around the viewport. Keep a few screens of history for a short scroll back, not
		// the whole list after someone scrolls through it once with a text column open.
		trimCache(cache.current, new Set(folders));
		const missing = missingFolders(cache.current, folders);
		if (missing.length === 0) return;
		let active = true;
		void (async () => {
			try {
				const extras = await unwrap(commands.modExtras(gameId, missing));
				// The game changed, or a newer list replaced this one while the answer was in flight.
				// oxlint-disable-next-line typescript/no-unnecessary-condition -- the cleanup flips `active`.
				if (!active) return;
				for (const extra of extras) cache.current.set(extra.folder, extra);
				setLoaded(new Map(cache.current));
			} catch {
				// The columns show their empty value; the next status answer asks again.
			}
		})();
		return () => {
			active = false;
		};
	}, [folders, gameId, token]);

	return loaded;
}
