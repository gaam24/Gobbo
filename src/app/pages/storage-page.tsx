import { Suspense, useEffect } from "react";
import { useGames } from "@/app/contexts/games-context";
import { useStorage } from "@/app/contexts/storage-context";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { StorageView } from "@/app/lazy-views";
import { useTranslation } from "@/i18n";

/// The storage page: every game data folder with its size, including the games removed from the list.
export function StoragePage() {
	const { i18n } = useTranslation();
	const { games } = useGames();
	const { openPathFor } = useWorkspaceActions();
	const { usage, loading, failed, reload, restore, deleteData } = useStorage();

	// Walking every game folder is only worth it when the page is open; the provider keeps the scan visible afterwards.
	useEffect(() => {
		void reload();
	}, [reload]);

	return (
		<Suspense fallback={null}>
			<StorageView
				games={games}
				usage={usage}
				loading={loading}
				failed={failed}
				locale={i18n.language}
				onRefresh={reload}
				onRestore={restore}
				onDelete={deleteData}
				onOpen={openPathFor}
			/>
		</Suspense>
	);
}
