import { useDownloadsContext } from "@/app/contexts/downloads-context";
import { useGames } from "@/app/contexts/games-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeList } from "@/app/contexts/notices-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { GameRail } from "@/components/game-rail";

/// The game rail with its data. Keeping the container here leaves `GameRail` a plain component that only draws what it
/// is given, which is what the render tests and future previews need.
export function GameRailHost() {
	const { managedGames, managed } = useGames();
	const { view, openGames, openAppSettings, openDownloads, openNotifications, openStorage } = useNavigation();
	const { notices } = useNoticeList();
	const { gameId, status, gameRunning, selectGame, openPathFor, copyPath, gameActions } = useWorkspace();
	const { summary } = useDownloadsContext();

	let activeState: "running" | "draft" | null = null;

	if (gameRunning) activeState = "running";
	else if ((status?.draft_changes ?? 0) > 0) activeState = "draft";

	const openRailView = (next: "games" | "app" | "downloads" | "notifications" | "storage") => {
		if (next === "games") openGames();
		else if (next === "app") openAppSettings();
		else if (next === "downloads") openDownloads();
		else if (next === "storage") openStorage();
		else openNotifications();
	};

	return (
		<GameRail
			games={managedGames}
			managed={managed ?? []}
			activeId={gameId}
			activeState={activeState}
			view={view}
			paths={
				status
					? {
							library: status.library_dir,
							archives: status.archives_dir,
							profile: status.profile_dir ?? null,
							overwrite: status.has_overwrite ? status.overwrite_dir : null,
						}
					: null
			}
			notices={notices}
			downloads={summary}
			onSelect={selectGame}
			onView={openRailView}
			onOpen={openPathFor}
			onCopy={copyPath}
			onRemove={gameActions.removeGame}
		/>
	);
}
