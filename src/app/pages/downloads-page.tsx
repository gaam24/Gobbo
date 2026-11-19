import { Suspense } from "react";
import { useDownloadsContext } from "@/app/contexts/downloads-context";
import { useGames } from "@/app/contexts/games-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useSettingsActions, useSettingsData } from "@/app/contexts/settings-context";
import { useWorkspaceActions } from "@/app/contexts/workspace-context";
import { DownloadsView } from "@/app/lazy-views";
import { useTranslation } from "@/i18n";

/// The Nexus download queue. The view is lazy: a session that never follows an `nxm://` link never parses it.
export function DownloadsPage() {
	const { i18n } = useTranslation();
	const { downloads, installDownload, removeDownload, retryDownload, clearDownloads } = useDownloadsContext();
	const { games, managedGames } = useGames();
	const { providers, sourceLinks, readyProviders } = useSettingsData();
	const { saveSourceLinks } = useSettingsActions();
	const { openAppSettings } = useNavigation();
	const { selectGame } = useWorkspaceActions();
	/// The providers that have download links; turning links on from the queue takes all of them.
	const linkProviders = providers.filter((provider) => provider.schemes.length > 0).map((provider) => provider.id);

	return (
		<Suspense fallback={null}>
			<DownloadsView
				downloads={downloads}
				games={games}
				locale={i18n.language}
				linksOn={sourceLinks.length > 0}
				hasNexusKey={readyProviders.has("nexus")}
				managedIds={managedGames.map((game) => game.id)}
				onInstall={installDownload}
				onRemove={removeDownload}
				onRetry={retryDownload}
				onClear={clearDownloads}
				onShowMods={selectGame}
				onTurnOnLinks={() => void saveSourceLinks(linkProviders)}
				onOpenSettings={openAppSettings}
			/>
		</Suspense>
	);
}
