import type { ReactNode } from "react";
import { CollectionsProvider } from "@/app/contexts/collections-context";
import { DownloadsProvider } from "@/app/contexts/downloads-context";
import { GameEditorProvider } from "@/app/contexts/game-editor-context";
import { GamesProvider } from "@/app/contexts/games-context";
import { NavigationProvider } from "@/app/contexts/navigation-context";
import { NoticesProvider } from "@/app/contexts/notices-context";
import { SettingsProvider } from "@/app/contexts/settings-context";
import { StorageProvider } from "@/app/contexts/storage-context";
import { UiProvider } from "@/app/contexts/ui-context";
import { UnsavedProvider } from "@/app/contexts/unsaved-context";
import { useWorkspaceActions, WorkspaceProvider } from "@/app/contexts/workspace-context";
import { TrayBridge } from "@/app/hooks/use-tray";

/// The downloads depend on the import flow of the active workspace. Passing it as a prop keeps `DownloadsProvider` free
/// of the workspace context, so a hot update of the download queue cannot render it outside its provider.
function DownloadsBridge({ children }: { children: ReactNode }) {
	const { importPaths, acceptImported } = useWorkspaceActions();
	return (
		<DownloadsProvider importPaths={importPaths} acceptImported={acceptImported}>
			<CollectionsProvider>{children}</CollectionsProvider>
		</DownloadsProvider>
	);
}

/// The provider tree of the app. The order is the dependency order: notices are used by everyone, settings by the UI and
/// the workspace, and the workspace by downloads (a finished download imports itself into its game).
export function AppProviders({ children }: { children: ReactNode }) {
	return (
		<NoticesProvider>
			<SettingsProvider>
				<UnsavedProvider>
					<NavigationProvider>
						<TrayBridge>
							<UiProvider>
								<GameEditorProvider>
									<GamesProvider>
										<StorageProvider>
											<WorkspaceProvider>
												<DownloadsBridge>{children}</DownloadsBridge>
											</WorkspaceProvider>
										</StorageProvider>
									</GamesProvider>
								</GameEditorProvider>
							</UiProvider>
						</TrayBridge>
					</NavigationProvider>
				</UnsavedProvider>
			</SettingsProvider>
		</NoticesProvider>
	);
}
