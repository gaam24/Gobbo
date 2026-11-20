import { Suspense } from "react";

import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { ModInspector } from "@/app/lazy-views";

/// The details column only exists on the mod list. The outer component watches the route; the inner one subscribes to
/// the game status, so a status answer on another screen does not re-render an inspector that is not there.
export function ModsInspectorHost() {
	const { view } = useNavigation();
	return view === "mods" ? <ModsInspectorView /> : null;
}

function ModsInspectorView() {
	const { openDownloads } = useNavigation();
	const { notifyError } = useNoticeActions();
	const { wide, inspectorCollapsed, inspectorWidth, drawerOpen } = useUiState();
	const { setDrawerOpen, resizeInspector, openDialog } = useUiActions();
	const {
		gameId,
		gameName,
		gameRunning,
		status,
		selected,
		setSelection,
		updates,
		gameActions,
		modUpdates,
		pickArchives,
		deploy,
		openFolderPath,
		revealFolderPath,
		showGames,
	} = useWorkspace();

	if (!(gameId && status) || (wide && inspectorCollapsed)) return null;

	return (
		<Suspense fallback={null}>
			<ModInspector
				gameId={gameId}
				gameName={gameName}
				gameRunning={gameRunning}
				status={status}
				selection={selected}
				updates={updates}
				busy={gameActions.busy}
				onSelect={setSelection}
				onSetEnabled={(ids, enabled) => {
					for (const modId of ids) void gameActions.setModEnabled(modId, enabled);
				}}
				onSetTags={gameActions.setModTags}
				onSetNote={gameActions.setModNote}
				onSetPriority={gameActions.setModPriority}
				onSetInstructions={gameActions.setModInstructions}
				onSetRelations={gameActions.setModRelations}
				onSetFileWinner={gameActions.setFileWinner}
				onMoveBeside={gameActions.moveMod}
				onSetSource={gameActions.setModSource}
				onIgnoreVersion={gameActions.ignoreVersion}
				onUnignoreVersion={gameActions.unignoreVersion}
				onSetInfo={gameActions.setModInfo}
				onSetSteps={gameActions.setModSteps}
				onCheck={modUpdates.checkMod}
				onSelectVersion={gameActions.selectVersion}
				onDeleteVersion={(modId, folder) => openDialog({ kind: "remove-version", modId, folder })}
				onDeleteUnusedVersions={gameActions.deleteUnusedVersions}
				onOpenFolder={openFolderPath}
				onRevealFile={revealFolderPath}
				onArchiveReport={(folder, name) => openDialog({ kind: "archive-report", folder, name })}
				onOpenGames={showGames}
				onError={notifyError}
				onRemoveFromLibrary={(modId) => openDialog({ kind: "remove-mod", modId })}
				onOpenOverwrite={() => openDialog({ kind: "overwrite" })}
				onRunTool={gameActions.runTool}
				onImport={pickArchives}
				onDeploy={deploy}
				onOpenDownloads={openDownloads}
				mode={wide ? "side" : "sheet"}
				open={drawerOpen}
				onOpenChange={setDrawerOpen}
				width={inspectorWidth}
				onResize={resizeInspector}
				onResetWidth={() => resizeInspector(null)}
			/>
		</Suspense>
	);
}
