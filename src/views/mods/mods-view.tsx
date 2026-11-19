import { Lock } from "lucide-react";
import { lazy, Suspense, useEffect, useMemo, useRef } from "react";
import { useSettingsData } from "@/app/contexts/settings-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { useOpenPalette } from "@/app/palette/use-open-palette";
import type { Status } from "@/bindings";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { useTranslation } from "@/i18n";
import { BisectBar } from "@/views/bisect-bar";
import { type GroupMenuActions, menuGroupFor } from "@/views/mod-table/mod-list-props";
import { CrashPrompt } from "@/views/mods/crash-prompt";
import { EmptyLibrary } from "@/views/mods/empty-library";
import { GameHeader } from "@/views/mods/game-header";
import { ImportVersionBanner } from "@/views/mods/import-version-banner";

import { ModsAlerts } from "@/views/mods/mods-alerts";
import { ModsListMenu } from "@/views/mods/mods-list-menu";
import { ModsSelectionBar } from "@/views/mods/mods-selection-bar";
import { CONSOLE_SHARE, ModsSplit } from "@/views/mods/mods-split";
import { ModsStatusBar } from "@/views/mods/mods-status-bar";
import { ModsToolbar } from "@/views/mods/mods-toolbar";
import { useHiddenMods } from "@/views/mods/use-hidden-mods";
import { useListKeys } from "@/views/mods/use-list-keys";
import { useListRows } from "@/views/mods/use-list-rows";
import { useModFilter } from "@/views/mods/use-mod-filter";
import { useModView } from "@/views/mods/use-mod-view";
import { useRowActions } from "@/views/mods/use-row-actions";

const GameConsole = lazy(async () => {
	const { GameConsole: component } = await import("@/views/console/game-console");
	return { default: component };
});

const ModList = lazy(async () => {
	const { ModList: component } = await import("@/views/mods/mod-list");
	return { default: component };
});

interface ModsViewProps {
	gameId: string;
	/// The status of the shown game; the page renders the loading and error states before it exists.
	status: Status;
}

/// The console's share of the main column, in percent. It lives outside the component, so it survives closing the console and
/// switching games during this session. It is not saved to disk.
let consoleShare = CONSOLE_SHARE;

/// The main column: header with the state, profile tabs, toolbar, the mod table, the console and the status bar.
export function ModsView({ gameId, status }: ModsViewProps) {
	const {
		gameName,
		selected: selection,
		setSelection: onSelect,
		updates,
		gameRunning,
		gameActions,
		modUpdates,
		pickArchives: onImport,
		launch: onLaunch,
		openSource: onOpenSource,
		openFolderPath: onOpenFolder,
		revealFolderPath: onRevealFile,
		pendingVersions,
		dismissPendingVersion,
	} = useWorkspace();
	const { readyProviders } = useSettingsData();
	const { consoleOpen } = useUiState();
	const { setConsoleOpen: onConsoleOpen, showDetails: onShowDetails, openDialog } = useUiActions();
	const {
		setModEnabled: onSetEnabled,
		moveMod: onMoveMod,
		selectVersion: onSelectVersion,
		reinstallMod: onReinstall,
		rescan: onRescan,
		undoList: onUndoList,
		redoList: onRedoList,
		bisectAnswer: onBisectAnswer,
		bisectStop: onBisectStop,
		ignoreVersion: onIgnoreVersion,
		unignoreVersion: onUnignoreVersion,
		updateMod: onUpdateMod,
	} = gameActions;
	const onCheckUpdate = modUpdates.checkModFromMenu;
	const onSortByRules = () => openDialog({ kind: "sort" });
	const onImportCommunityProfile = () => openDialog({ kind: "community-profile" });
	const onRemoveFromLibrary = (modId: string) => openDialog({ kind: "remove-mod", modId });
	const onDeleteVersion = (modId: string, folder: string) => openDialog({ kind: "remove-version", modId, folder });
	const onDeletePage = (pageId: string) => openDialog({ kind: "remove-page", pageId });
	const onArchiveReport = (folder: string, name: string) => openDialog({ kind: "archive-report", folder, name });
	const onOpenShortcuts = () => openDialog({ kind: "shortcuts" });
	const onRenameGroup = (id: string) => openDialog({ kind: "group-edit", id });
	const onUngroup = (id: string) => void gameActions.ungroupMods(id);
	const onSetTranslationWinner = (groupId: string) => openDialog({ kind: "translation-winners", groupId });
	const openPalette = useOpenPalette();
	const { t, i18n } = useTranslation();

	const searchRef = useRef<HTMLInputElement>(null);
	const tableRef = useRef<HTMLDivElement>(null);
	const [modView, setModView] = useModView();
	const { mods } = status;
	/// The folds and column layout belong to the active profile; finding it once keeps the hooks below from scanning
	/// the profile list again.
	const profileId = status.profiles.find((profile) => profile.active)?.id ?? "none";

	const { hidden, setHidden } = useHiddenMods({ gameId, profileId, mods, profiles: status.profiles });
	const { filter, setFilter, query, setQuery, tagFilter, setTagFilter, filtering, allTags, visible, counts, clearFilters } = useModFilter({
		mods,
		updates,
		hidden,
	});
	const { items, collapsed, groupSizes, rows, shown, toggleGroup, foldAll, foldedPages, togglePage } = useListRows({
		gameId,
		profileId,
		mods,
		separators: status.separators,
		groups: status.groups,
		filtering,
		visible,
		hidden,
	});

	const focused = selection[selection.length - 1] ?? null;
	const selectedSet = useMemo(() => new Set(selection), [selection]);

	/// One stable object for every memoized row and card. The refs carry the newest status and action wrappers, so a
	/// status answer never changes a row's props; the group is resolved when a menu actually opens.
	const latestMenu = useRef({ groups: status.groups, selection, openDialog, gameActions });
	useEffect(() => {
		latestMenu.current = { groups: status.groups, selection, openDialog, gameActions };
	}, [status.groups, selection, openDialog, gameActions]);
	const groupMenu = useMemo<GroupMenuActions>(
		() => ({
			groupForMenu: (modId) => menuGroupFor(modId, latestMenu.current.groups, latestMenu.current.selection),
			onEditGroup: (id, members) => latestMenu.current.openDialog({ kind: "group-edit", id, members }),
			onUngroupMods: (groupId) => void latestMenu.current.gameActions.ungroupMods(groupId),
			onSetModGroup: (modId, groupId) => void latestMenu.current.gameActions.setModGroup(modId, groupId),
			onSetTranslationWinner: (groupId) => latestMenu.current.openDialog({ kind: "translation-winners", groupId }),
			groupSelection: () => latestMenu.current.selection,
			onGroupSelected: (ids) => void latestMenu.current.gameActions.groupMods([...ids]),
		}),
		[],
	);

	useListKeys({
		searchRef,
		tableRef,
		filtering,
		focused,
		mods,
		selection,
		shown,
		grid: modView === "grid",
		history: status.history,
		onSetEnabled,
		onMoveMod,
		onSelect,
		onUndoList,
		onRedoList,
		onRescan,
	});

	const {
		toggle,
		move,
		reorder,
		renameSeparator,
		removeSeparator,
		groupEnabled,
		sortGroup,
		addSeparator,
		freshSeparator,
		addSeparatorAbove,
		addSeparatorAtEnd,
		deferAfterListMenu,
		takeAfterListMenu,
		renameDone,
		showDetailsOf,
		showConflictsOf,
		pick,
		sortByName,
	} = useRowActions({
		separators: status.separators,
		selection,
		focused,
		shown,
		onSetEnabled,
		onMoveMod,
		onSetSeparator: gameActions.setSeparator,
		onSortList: gameActions.sortList,
		onSelect,
		onShowDetails,
	});

	/// A mod picked from elsewhere (the command palette, the keyboard) scrolls into view.
	useEffect(() => {
		if (!focused) return;
		tableRef.current?.querySelector<HTMLElement>(`[data-mod="${CSS.escape(focused)}"]`)?.scrollIntoView({ block: "nearest" });
	}, [focused]);

	/// The selected mods, for the bar under the list. With nothing selected, the whole list is not walked.
	const picked = selection.length === 0 ? [] : mods.filter((mod) => selectedSet.has(mod.id));
	/// The ends of the full list for a move across a separator; a copy of every mod is not needed.
	let firstModId: string | null = null;
	let lastModId: string | null = null;
	for (const item of items) {
		if (item.kind !== "mod") continue;
		// oxlint-disable-next-line typescript/prefer-nullish-coalescing -- React Compiler cannot compile `??=`.
		if (firstModId === null) firstModId = item.id;
		lastModId = item.id;
	}

	return (
		<div className="@container flex min-h-0 min-w-0 flex-1 flex-col">
			<GameHeader gameId={gameId} status={status} />

			<ModsToolbar
				gameId={gameId}
				status={status}
				searchRef={searchRef}
				filter={filter}
				setFilter={setFilter}
				query={query}
				setQuery={setQuery}
				tagFilter={tagFilter}
				setTagFilter={setTagFilter}
				allTags={allTags}
				counts={counts}
				filtering={filtering}
				onSortByName={sortByName}
				onAddSeparator={addSeparator}
				onImportCommunityProfile={onImportCommunityProfile}
				modView={modView}
				onModView={setModView}
			/>

			{status.bisect && (
				<BisectBar
					bisect={status.bisect}
					gameRunning={gameRunning}
					onLaunch={() => onLaunch(null, true)}
					onAnswer={onBisectAnswer}
					onStop={onBisectStop}
				/>
			)}

			{gameRunning && (
				<div role="status" className="bg-muted text-muted-foreground flex shrink-0 items-center gap-2 border-b px-6 py-2.5 text-sm">
					<Lock className="size-4 shrink-0" aria-hidden />
					<span className="min-w-0">{t("mods.lock.running")}</span>
				</div>
			)}

			<ModsSplit
				consoleOpen={consoleOpen}
				defaultShare={consoleShare}
				onShare={(share) => {
					consoleShare = share;
				}}
				console={
					<Suspense fallback={null}>
						<GameConsole gameId={gameId} onClose={() => onConsoleOpen(false)} />
					</Suspense>
				}
			>
				<div className="flex h-full min-h-0 flex-1 flex-col">
					<ContextMenu>
						<ContextMenuTrigger asChild>
							<div ref={tableRef} className="flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden overflow-y-auto [overflow-anchor:none]">
								<CrashPrompt
									gameId={gameId}
									gameRunning={gameRunning}
									bisectRunning={status.bisect !== null}
									onFindMod={() => void gameActions.bisectStart()}
								/>
								<ModsAlerts problems={status.problems} warnings={status.warnings} />
								<ImportVersionBanner
									mods={pendingVersions}
									onUse={(item) => {
										dismissPendingVersion(item.folder);
										return onSelectVersion(item.family, item.folder);
									}}
									onDismiss={(item) => dismissPendingVersion(item.folder)}
								/>
								{mods.length === 0 ? (
									<EmptyLibrary gameName={gameName} libraryDir={status.library_dir} onImport={onImport} onOpenFolder={onOpenFolder} />
								) : (
									<Suspense fallback={null}>
										<ModList
											view={modView}
											gameId={gameId}
											groupMenu={groupMenu}
											scrollRef={tableRef}
											rows={rows}
											collapsed={collapsed}
											foldedPages={foldedPages}
											profileId={profileId}
											groupSizes={groupSizes}
											groups={status.groups}
											onRenameGroup={onRenameGroup}
											onUngroup={onUngroup}
											onSetTranslationWinner={onSetTranslationWinner}
											onTogglePage={togglePage}
											onDeletePage={onDeletePage}
											onSetEnabled={onSetEnabled}
											onRenameSeparator={renameSeparator}
											onRenameDone={renameDone}
											onRemoveSeparator={removeSeparator}
											onGroupEnabled={groupEnabled}
											onSortGroup={sortGroup}
											onToggleGroup={toggleGroup}
											selection={selection}
											filtering={filtering}
											updates={updates}
											libraryDir={status.library_dir}
											archivesDir={status.archives_dir}
											locale={i18n.language}
											onPick={pick}
											onToggle={toggle}
											onMove={move}
											onReorder={reorder}
											freshSeparator={freshSeparator}
											onClearFilters={clearFilters}
											onShowDetails={showDetailsOf}
											onOpenConflicts={showConflictsOf}
											onAddSeparatorAbove={addSeparatorAbove}
											onOpenFolder={onOpenFolder}
											onRevealFile={onRevealFile}
											onOpenSource={onOpenSource}
											onCheckUpdate={onCheckUpdate}
											onUpdateMod={onUpdateMod}
											onIgnoreVersion={onIgnoreVersion}
											onUnignoreVersion={onUnignoreVersion}
											onSetHidden={setHidden}
											hidden={hidden}
											readyProviders={readyProviders}
											onRemoveFromLibrary={onRemoveFromLibrary}
											onSelectVersion={onSelectVersion}
											onDeleteVersion={onDeleteVersion}
											onReinstall={onReinstall}
											onArchiveReport={onArchiveReport}
											onChangeInstaller={(modId, folder, name) => openDialog({ kind: "reinstall", modId, folder, name })}
											firstModId={firstModId}
											lastModId={lastModId}
										/>
									</Suspense>
								)}
							</div>
						</ContextMenuTrigger>
						<ModsListMenu
							shown={shown}
							mods={mods}
							filtering={filtering}
							hasSeparators={status.separators.length > 0}
							onSelect={onSelect}
							onAddSeparatorAtEnd={() => deferAfterListMenu(addSeparatorAtEnd)}
							onImport={onImport}
							onSortByName={sortByName}
							onSortByRules={onSortByRules}
							onFoldAll={foldAll}
							onCloseAutoFocus={(event) => {
								const action = takeAfterListMenu();
								if (!action) return;
								event.preventDefault();
								action();
							}}
						/>
					</ContextMenu>

					<ModsSelectionBar
						picked={picked}
						selection={selection}
						hidden={hidden}
						onSetHidden={setHidden}
						onSetEnabled={onSetEnabled}
						onSelect={onSelect}
					/>
				</div>
			</ModsSplit>

			<ModsStatusBar
				shown={filtering ? visible.length : mods.length - hidden.size}
				total={mods.length}
				consoleOpen={consoleOpen}
				onConsoleOpen={onConsoleOpen}
				onOpenPalette={openPalette}
				onOpenShortcuts={onOpenShortcuts}
			/>
		</div>
	);
}
