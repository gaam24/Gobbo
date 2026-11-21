import { Suspense } from "react";

import { useGameEditor } from "@/app/contexts/game-editor-context";
import { useGames } from "@/app/contexts/games-context";
import { useSettingsData } from "@/app/contexts/settings-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { GameEditor, GamesPanel } from "@/app/lazy-views";

/// The catalogue of supported games. The scan starts when the page mounts, exactly as before; the page itself only
/// forwards the data and the two actions that change the added list. The definition editor replaces the catalogue
/// while it is open, so both share the same screen and the same history entry.
export function GamesPage() {
	const {
		games,
		managed,
		scans,
		scanError,
		definitionProblems,
		catalogError,
		scannedAt,
		scanning,
		scanGames,
		reloadGames,
		checkGameFolder,
	} = useGames();
	const { gameActions, openPathFor, copyPath, selectGame } = useWorkspace();
	const { checkCatalogOnLaunch } = useSettingsData();
	const { target, closeGameEditor, openGameEditor } = useGameEditor();

	if (target) {
		return (
			<Suspense fallback={null}>
				<GameEditor key={target.id ?? "new"} id={target.id} onClose={closeGameEditor} />
			</Suspense>
		);
	}

	return (
		<Suspense fallback={null}>
			<GamesPanel
				games={games}
				managed={managed ?? []}
				scans={scans}
				scanError={scanError}
				definitionProblems={definitionProblems}
				catalogError={catalogError}
				checkCatalogOnLaunch={checkCatalogOnLaunch}
				onReload={() => reloadGames()}
				scannedAt={scannedAt}
				scanning={scanning}
				onScan={scanGames}
				onAdd={gameActions.addGame}
				onCheck={checkGameFolder}
				onSelect={selectGame}
				onRemove={gameActions.removeGame}
				onOpen={openPathFor}
				onCopy={copyPath}
				onCreate={() => openGameEditor()}
				onEdit={(id) => openGameEditor(id)}
			/>
		</Suspense>
	);
}
