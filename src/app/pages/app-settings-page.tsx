import { Suspense, useEffect } from "react";
import { useSettingsActions, useSettingsData } from "@/app/contexts/settings-context";
import { useWorkspace, useWorkspaceActions } from "@/app/contexts/workspace-context";
import { AppSettings } from "@/app/lazy-views";

/// The settings shared by every game. The page is lazy because most sessions never open it. `AppSettings` renders its own
/// page frame, which is the one scrolling element, so this component adds no wrapper of its own.
export function AppSettingsPage() {
	const {
		dataFolder,
		storage,
		providers,
		keysSaved,
		accounts,
		sourceLinks,
		linkStatus,
		language,
		rememberWindow,
		minimizeToTray,
		efficiencyMode,
		checkCollectionsOnLaunch,
		checkCatalogOnLaunch,
	} = useSettingsData();
	const {
		saveLanguage,
		saveSourceKey,
		openSourceKeyPage,
		saveSourceLinks,
		saveRememberWindow,
		saveMinimizeToTray,
		saveEfficiencyMode,
		saveCheckCollectionsOnLaunch,
		saveCheckCatalogOnLaunch,
		refreshSourceLinks,
		ensureSourceLinks,
	} = useSettingsActions();
	const { validateSourceKey, openFolderPath, copyPath } = useWorkspaceActions();
	const { gameId } = useWorkspace();

	/// The handler can change while the window is in the background, so the page reads the state the system has now.
	useEffect(() => {
		void refreshSourceLinks();
	}, [refreshSourceLinks]);

	return (
		<Suspense fallback={null}>
			<AppSettings
				language={language}
				onLanguage={saveLanguage}
				providers={{
					list: providers,
					keysSaved,
					accounts,
					links: sourceLinks,
					status: linkStatus,
				}}
				providerActions={{
					onSaveKey: saveSourceKey,
					onValidateKey: validateSourceKey,
					onOpenKeyPage: openSourceKeyPage,
					onLinks: saveSourceLinks,
					onRetakeLinks: ensureSourceLinks,
				}}
				rememberWindow={rememberWindow}
				onRememberWindow={saveRememberWindow}
				minimizeToTray={minimizeToTray}
				onMinimizeToTray={saveMinimizeToTray}
				efficiencyMode={efficiencyMode}
				onEfficiencyMode={saveEfficiencyMode}
				checkCollectionsOnLaunch={checkCollectionsOnLaunch}
				onCheckCollectionsOnLaunch={saveCheckCollectionsOnLaunch}
				checkCatalogOnLaunch={checkCatalogOnLaunch}
				onCheckCatalogOnLaunch={saveCheckCatalogOnLaunch}
				dataFolder={dataFolder}
				storage={storage}
				onOpenFolder={openFolderPath}
				onCopy={copyPath}
				gameId={gameId ?? null}
			/>
		</Suspense>
	);
}
