import { Suspense } from "react";
import { useGameEditor } from "@/app/contexts/game-editor-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { GameSettings } from "@/app/lazy-views";
import { commands } from "@/bindings";
import { StatusError } from "@/components/app/status-error";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { pickSaveFile } from "@/lib/tauri";

/// The settings of the active game. Like the mod list, it is a container around a presentational screen.
export function GameSettingsPage() {
	const { openGames } = useNavigation();
	const { gameId, game, status, statusError, retryStatus, gameActions, gameRunning, openFolderPath, copyPath } = useWorkspace();
	const { t } = useTranslation();
	const { notify, attempt } = useNoticeActions();
	const { openGameEditor } = useGameEditor();

	const exportDefinition = async () => {
		if (!game) return;
		const path = await pickSaveFile(`${game.id}.zip`, t("settings.game.exportFilter"), "zip");
		if (!path) return;
		await attempt(async () => {
			await unwrap(commands.exportDefinition(game.id, path));
			notify(t("notice.definitionExported", { path }));
		});
	};

	if (!gameId) return null;
	if (!status) {
		return statusError ? <StatusError error={statusError} onRetry={retryStatus} /> : null;
	}

	return (
		<Suspense fallback={null}>
			<GameSettings
				key={gameId}
				status={status}
				game={game}
				onSave={gameActions.saveSettings}
				lists={{
					setProtonOption: gameActions.setProtonOption,
					addProfileData: gameActions.addProfileData,
					removeProfileData: gameActions.removeProfileData,
				}}
				userData={{
					onSetUserData: gameActions.setProfileUserData,
					onUserDataSize: gameActions.userDataSize,
					onMoveUserData: gameActions.moveProfileUserData,
					gameRunning,
				}}
				onOpen={openFolderPath}
				onCopy={copyPath}
				onInstallPrerequisites={gameActions.installToolPrerequisites}
				onRemove={gameActions.removeGame}
				onExportDefinition={exportDefinition}
				onEditDefinition={
					game?.origin === "user"
						? () => {
								openGameEditor(game.id);
								openGames();
							}
						: undefined
				}
			/>
		</Suspense>
	);
}
