import { createContext, type ReactNode, useCallback, useContext, useMemo, useRef, useState } from "react";
import { useGames } from "@/app/contexts/games-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { commands, type GameDataUsage } from "@/bindings";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { nameOfId } from "@/lib/records";

interface Storage {
	usage: GameDataUsage[] | null;
	/// A scan is running; the activity card shows it even after the Storage page is left.
	loading: boolean;
	failed: boolean;
	reload: () => Promise<boolean>;
	restore: (id: string) => Promise<boolean>;
	deleteData: (id: string) => Promise<boolean>;
}

/// A scan walks every game folder, so the page reuses an answer from the last half minute. Restoring or deleting a
/// folder asks for a fresh one.
const STORAGE_CACHE_MS = 30_000;

const StorageContext = createContext<Storage | null>(null);

export function useStorage(): Storage {
	const context = useContext(StorageContext);

	if (!context) {
		throw new Error("useStorage must be used inside <StorageProvider>.");
	}

	return context;
}

/// The game data folders and their sizes. Kept above the screens so a scan stays visible in the activity card after the
/// Storage page is left; the page starts the scan when it opens, because walking every game folder is not a start-up cost.
export function StorageProvider({ children }: { children: ReactNode }) {
	const { t } = useTranslation();
	const { games, setManaged } = useGames();
	const { notify, attempt } = useNoticeActions();
	const [usage, setUsage] = useState<GameDataUsage[] | null>(null);
	const [loading, setLoading] = useState(false);
	const [failed, setFailed] = useState(false);
	const lastScan = useRef(0);

	const nameOf = useCallback((id: string) => nameOfId(games, id), [games]);

	const read = useCallback(
		() =>
			attempt(async () => {
				setUsage(await unwrap(commands.storageUsage()));
				setFailed(false);
			}),
		[attempt],
	);

	const reload = useCallback(
		(force = false) => {
			if (!force && usage !== null && Date.now() - lastScan.current < STORAGE_CACHE_MS) return Promise.resolve(true);
			setLoading(true);
			return read().then((ok) => {
				if (ok) lastScan.current = Date.now();
				setFailed(!ok);
				setLoading(false);
				return ok;
			});
		},
		[read, usage],
	);

	const restore = useCallback(
		async (id: string) => {
			const name = nameOf(id);
			const ok = await attempt(async () => {
				setManaged(await unwrap(commands.restoreGame(id)));
				notify(t("notice.gameRestored", { name }));
			});

			if (ok) await reload(true);
			return ok;
		},
		[attempt, nameOf, notify, reload, setManaged, t],
	);

	const deleteData = useCallback(
		async (id: string) => {
			const name = nameOf(id);
			const ok = await attempt(async () => {
				await unwrap(commands.deleteRemovedGame(id));
				notify(t("notice.gameDataDeleted", { name }));
			});

			if (ok) await reload(true);
			return ok;
		},
		[attempt, nameOf, notify, reload, t],
	);

	const value = useMemo(
		(): Storage => ({ usage, loading, failed, reload, restore, deleteData }),
		[deleteData, failed, loading, reload, restore, usage],
	);

	return <StorageContext.Provider value={value}>{children}</StorageContext.Provider>;
}
