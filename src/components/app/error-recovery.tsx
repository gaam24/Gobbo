import { useNavigation } from "@/app/contexts/navigation-context";
import { useSettingsData } from "@/app/contexts/settings-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { Recovery } from "@/bindings";
import { Button } from "@/components/ui/button";
import { type TranslationKey, useTranslation } from "@/i18n";
import { recoveryOf } from "@/lib/ipc";
import { openPath } from "@/lib/open";

const LABEL: Record<Recovery["action"], TranslationKey> = {
	set_game_folder: "errors.recovery.setGameFolder",
	set_settings_folder: "errors.recovery.setSettingsFolder",
	set_mods_folder: "errors.recovery.setModsFolder",
	save_provider_key: "errors.recovery.saveProviderKey",
	close_game: "errors.recovery.closeGame",
	open_log: "errors.recovery.openLog",
};

/// A button for the step the backend offers next to an error. It is one component, so every screen names the same
/// recovery action the same way. A failure without a recovery keeps its message and its Retry only.
export function ErrorRecovery({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
	const recovery = recoveryOf(error);
	if (!recovery) return null;
	return <RecoveryButton recovery={recovery} onRetry={onRetry} />;
}

function RecoveryButton({ recovery, onRetry }: { recovery: Recovery; onRetry?: () => void }) {
	const { t } = useTranslation();
	const { openGameSettings, openAppSettings } = useNavigation();
	const { dataFolder } = useSettingsData();
	const { gameActions } = useWorkspace();

	const action = () => {
		switch (recovery.action) {
			case "set_game_folder":
			case "set_settings_folder":
			case "set_mods_folder":
				openGameSettings();
				return;
			case "save_provider_key":
				openAppSettings();
				return;
			case "close_game":
				void gameActions.stopGame();
				return;
			case "open_log":
				void openPath(dataFolder, null, false, () => {
					// Opening the log folder is best-effort; no follow-up is needed when it succeeds.
				});
				return;
			default:
				onRetry?.();
		}
	};

	return (
		<Button variant="outline" size="sm" onClick={action}>
			{t(LABEL[recovery.action])}
		</Button>
	);
}
