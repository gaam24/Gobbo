import {
	ArchiveRestore,
	ArrowDownAZ,
	Bell,
	Boxes,
	CheckCheck,
	CircleArrowUp,
	Copy,
	Download as DownloadIcon,
	FileStack,
	Files,
	FolderOpen,
	HardDrive,
	Keyboard,
	Layers,
	ListTree,
	Package,
	PanelRight,
	Play,
	Plus,
	Redo2,
	RefreshCw,
	SearchCheck,
	Settings2,
	SplitSquareVertical,
	Stethoscope,
	Terminal,
	Undo2,
	Upload,
	Wrench,
} from "lucide-react";
import { useCollections } from "@/app/contexts/collections-context";
import { useGames } from "@/app/contexts/games-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { gamePresentation } from "@/app/selectors/game-presentation";
import type { Status } from "@/bindings";
import { useTranslation } from "@/i18n";
import { authorOf, formatModList, hasUpdate } from "@/lib/mods";
import type { PaletteItem } from "@/lib/palette";
import { shortcutHint } from "@/lib/shortcuts";
import { useSourceName } from "@/sources/source-name";

const reasonWhen = (disabled: boolean, reason: string) => (disabled ? reason : undefined);

/// Builds the palette's items from the state at the moment it opens. Keeping it in a hook makes the command list part of
/// the feature layer instead of another 150 lines of JSX in the app shell.
export function usePaletteItems(): () => PaletteItem[] {
	const { t } = useTranslation();
	const { openLink: openCollection } = useCollections();
	const { managedGames } = useGames();
	const { inspectorOpen, consoleOpen } = useUiState();
	const { toggleInspector, toggleConsole, openDialog } = useUiActions();
	const {
		gameId: id,
		gameName,
		updatesReady,
		status,
		gameRunning,
		busy,
		setSelection,
		gameActions,
		modUpdates,
		...workspace
	} = useWorkspace();
	const { openGameSettings, openAppSettings, openDownloads, openNotifications, openStorage } = useNavigation();
	const sourceName = useSourceName();
	const { toMods, showGames, selectGame, openFolderPath, copyToClipboard } = workspace;
	const { launch, deploy } = workspace;

	const gameActionsFor = (gameStatus: Status): PaletteItem[] => {
		const presentation = gamePresentation(gameStatus, { gameRunning, busy });
		const launchLabel = presentation.canApply
			? t(presentation.primary.labelKey, presentation.primary.params)
			: t(presentation.launchLabelKey);
		const applyLabel = t(presentation.applyLabelKey);
		const labels = {
			enabled: t("mods.state.enabled"),
			disabled: t("mods.state.disabled"),
			requires: t("mods.col.requires"),
			or: t("mods.relations.requirementsOr"),
		};
		const updateCount = gameStatus.mods.filter((mod) => hasUpdate(modUpdates.updates, mod.id)).length;
		let launchReason: string | undefined;

		if (gameRunning) launchReason = t("palette.reason.running");
		else if (!presentation.canPlay) launchReason = t("palette.reason.cannotPlay");
		let applyReason: string | undefined;

		if (gameRunning) applyReason = t("palette.reason.running");
		else if (!presentation.canApply) applyReason = t("palette.reason.cannotApply");
		let updateAllReason: string | undefined;

		if (!updatesReady) updateAllReason = t("updates.needKey");
		else if (updateCount === 0) updateAllReason = t("palette.reason.noUpdates");
		const enabledCount = gameStatus.mods.filter((mod) => mod.enabled).length;
		let bisectReason: string | undefined;

		if (gameStatus.bisect !== null) bisectReason = t("palette.reason.bisectRunning");
		else if (enabledCount < 2) bisectReason = t("palette.reason.bisectNeedsTwo");
		const actions: Array<Omit<PaletteItem, "group">> = [
			{
				id: "launch",
				icon: Play,
				label: launchLabel,
				keywords: t("palette.keywords.launch"),
				disabled: gameRunning || !presentation.canPlay,
				reason: launchReason,
				run: () => launch(null, presentation.canApply),
			},
			{
				id: "deploy",
				icon: Layers,
				label: applyLabel,
				disabled: gameRunning || !presentation.canApply,
				reason: applyReason,
				run: deploy,
			},
			{
				id: "import",
				icon: Upload,
				label: t("mods.import"),
				keywords: t("palette.keywords.import"),
				run: workspace.pickArchives,
			},
			{
				id: "collection",
				icon: Boxes,
				label: t("collection.installAction"),
				keywords: t("palette.keywords.collection"),
				run: openCollection,
			},
			{
				id: "updates",
				icon: CircleArrowUp,
				label: t("updates.button"),
				disabled: !updatesReady,
				reason: reasonWhen(!updatesReady, t("updates.needKey")),
				run: modUpdates.checkUpdates,
			},
			{
				id: "update-all",
				icon: DownloadIcon,
				label: t("updates.all", { count: updateCount }),
				disabled: !updatesReady || updateCount === 0,
				reason: updateAllReason,
				run: gameActions.updateAll,
			},
			{
				id: "undo",
				icon: Undo2,
				label: t("shortcuts.items.undo"),
				hint: shortcutHint("undo"),
				disabled: !gameStatus.history.can_undo,
				reason: reasonWhen(!gameStatus.history.can_undo, t("palette.reason.nothingToUndo")),
				run: gameActions.undoList,
			},
			{
				id: "redo",
				icon: Redo2,
				label: t("palette.items.redo"),
				hint: shortcutHint("redo"),
				disabled: !gameStatus.history.can_redo,
				reason: reasonWhen(!gameStatus.history.can_redo, t("palette.reason.nothingToRedo")),
				run: gameActions.redoList,
			},
			{
				id: "select-all",
				icon: CheckCheck,
				label: t("palette.items.selectAll"),
				hint: shortcutHint("all"),
				run: () => {
					toMods();
					setSelection(gameStatus.mods.map((mod) => mod.id));
				},
			},
			{
				id: "separator",
				icon: SplitSquareVertical,
				label: t("separators.addAtEnd"),
				run: () => gameActions.setSeparator("", t("separators.newName")),
			},
			{
				id: "sort-name",
				icon: ArrowDownAZ,
				label: t("mods.sortAlpha"),
				run: gameActions.sortList,
			},
			{
				id: "sort-rules",
				icon: ListTree,
				label: t("mods.sortByRules"),
				run: () => openDialog({ kind: "sort" }),
			},
			{
				id: "details",
				icon: PanelRight,
				label: inspectorOpen ? t("header.hideInspector") : t("header.showInspector"),
				hint: shortcutHint("details"),
				run: () => {
					toMods();
					void toggleInspector();
				},
			},
			{
				id: "console",
				icon: Terminal,
				label: consoleOpen ? t("console.close") : t("console.open"),
				hint: shortcutHint("console"),
				run: () => {
					toMods();
					toggleConsole();
				},
			},
			{
				id: "rescan",
				icon: RefreshCw,
				label: t("mods.rescan"),
				hint: shortcutHint("rescan"),
				run: gameActions.rescan,
			},
			{
				id: "bisect",
				icon: SearchCheck,
				label: t("bisect.start"),
				disabled: gameStatus.bisect !== null || enabledCount < 2,
				reason: bisectReason,
				run: gameActions.bisectStart,
			},
			{
				id: "conflicts",
				icon: Files,
				label: t("conflicts.title"),
				keywords: t("conflicts.hint"),
				run: () => openDialog({ kind: "conflicts" }),
			},
			{
				id: "snapshots",
				icon: ArchiveRestore,
				label: t("snapshots.title"),
				keywords: t("snapshots.hint"),
				run: () => openDialog({ kind: "snapshots" }),
			},
			{
				id: "health-check",
				icon: Stethoscope,
				label: t("health.title"),
				keywords: t("health.hint"),
				run: () => openDialog({ kind: "health" }),
			},
			{
				id: "setup-check",
				icon: Wrench,
				label: t("setupCheck.title"),
				keywords: t("setupCheck.hint"),
				run: () => openDialog({ kind: "setup" }),
			},
			{
				id: "pack-export",
				icon: Package,
				label: t("pack.export"),
				run: () => openDialog({ kind: "pack-export" }),
			},
			{
				id: "pack-import",
				icon: Package,
				label: t("pack.import"),
				run: workspace.pickPack,
			},
			{
				id: "overwrite",
				icon: FileStack,
				label: t("overwrite.show"),
				disabled: !gameStatus.has_overwrite,
				reason: reasonWhen(!gameStatus.has_overwrite, t("palette.reason.noOverwrite")),
				run: () => openDialog({ kind: "overwrite" }),
			},
			{
				id: "game-folder",
				icon: FolderOpen,
				label: t("games.openFolder"),
				disabled: !gameStatus.settings.game_root,
				reason: reasonWhen(!gameStatus.settings.game_root, t("palette.reason.noGameFolder")),
				run: () => openFolderPath(gameStatus.settings.game_root),
			},
			{
				id: "library-folder",
				icon: FolderOpen,
				label: t("games.openLibrary"),
				run: () => openFolderPath(gameStatus.library_dir),
			},
			{
				id: "copy-list",
				icon: Copy,
				label: t("mods.copyList", { format: t("mods.copyFormat.markdown") }),
				disabled: gameStatus.mods.length === 0,
				reason: reasonWhen(gameStatus.mods.length === 0, t("palette.reason.emptyList")),
				run: () => copyToClipboard(formatModList(gameStatus.mods, "markdown", labels, sourceName)),
			},
		];
		return actions.map((item) => ({ ...item, group: "actions" }));
	};

	const gameEntries = (gameStatus: Status): PaletteItem[] => [
		{
			id: "go-mods",
			group: "go",
			icon: ListTree,
			label: t("palette.items.mods", { game: gameName }),
			run: toMods,
		},
		{
			id: "go-game-settings",
			group: "go",
			icon: Settings2,
			label: t("mods.gameSettings"),
			run: openGameSettings,
		},
		...gameStatus.mods.map(
			(mod): PaletteItem => ({
				id: `mod-${mod.id}`,
				group: "mods",
				label: mod.info.name,
				keywords: [authorOf(mod) ?? "", ...mod.tags, mod.folder].join(" "),
				hint: mod.enabled ? t("mods.state.enabled") : t("mods.state.disabled"),
				run: () => {
					toMods();
					setSelection([mod.id]);
				},
			}),
		),
		...gameStatus.profiles.map(
			(profile): PaletteItem => ({
				id: `profile-${profile.id}`,
				group: "profiles",
				label: t("palette.items.profile", { name: profile.name }),
				hint: profile.active ? t("palette.current") : undefined,
				disabled: profile.active,
				reason: reasonWhen(profile.active, t("palette.reason.activeProfile")),
				run: () => gameActions.switchProfile(profile.id),
			}),
		),
	];

	return () => [
		...(id && status ? [...gameActionsFor(status), ...gameEntries(status)] : []),
		{
			id: "go-downloads",
			group: "go",
			icon: DownloadIcon,
			label: t("downloads.title"),
			run: openDownloads,
		},
		{
			id: "go-notifications",
			group: "go",
			icon: Bell,
			label: t("notifications.title"),
			run: openNotifications,
		},
		{
			id: "go-storage",
			group: "go",
			icon: HardDrive,
			label: t("storage.title"),
			run: openStorage,
		},
		{
			id: "go-games",
			group: "go",
			icon: Plus,
			label: t("app.addGame"),
			run: showGames,
		},
		{
			id: "go-settings",
			group: "go",
			icon: Settings2,
			label: t("app.settings"),
			run: openAppSettings,
		},
		{
			id: "go-shortcuts",
			group: "go",
			icon: Keyboard,
			label: t("shortcuts.title"),
			hint: shortcutHint("help"),
			run: () => openDialog({ kind: "shortcuts" }),
		},
		...managedGames.map(
			(other): PaletteItem => ({
				id: `game-${other.id}`,
				group: "games",
				label: t("palette.items.game", { name: other.name }),
				hint: other.id === id ? t("palette.current") : undefined,
				disabled: other.id === id,
				reason: reasonWhen(other.id === id, t("palette.reason.currentGame")),
				run: () => selectGame(other.id),
			}),
		),
	];
}
