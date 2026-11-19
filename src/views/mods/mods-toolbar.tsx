import {
	ArrowDownAZ,
	Boxes,
	CircleArrowUp,
	Copy,
	Download,
	EllipsisVertical,
	FileInput,
	FileUp,
	Languages,
	LayoutGrid,
	List,
	ListTree,
	RefreshCw,
	SearchCheck,
	Trash2,
	Upload,
} from "lucide-react";
import { useState } from "react";
import { useCollections } from "@/app/contexts/collections-context";
import { useSettingsActions } from "@/app/contexts/settings-context";
import { useUiActions } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import type { Status } from "@/bindings";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";
import { formatModList, type ListFormat } from "@/lib/mods";
import { useSourceName } from "@/sources/source-name";
import { updateTargets } from "@/sources/update-targets";
import { HistoryMenu } from "@/views/history/history-menu";
import { FilterBar, type FilterBarProps } from "@/views/mods/filter-bar";
import { SeparatorButton } from "@/views/mods/separator-button";
import { UpdateAllDialog } from "@/views/mods/update-all-dialog";
import type { ModView } from "@/views/mods/use-mod-view";

const LIST_FORMATS = ["text", "markdown", "bbcode"] as const satisfies readonly ListFormat[];

/// The toolbar under the game header: filters on the left, history and list-level actions on the right. The dropdown
/// keeps the less frequent actions (sorting, MO2 import, bisect, unused versions, copying) out of the main row.
export function ModsToolbar({
	gameId,
	status,
	onSortByName,
	onAddSeparator,
	onImportCommunityProfile,
	modView,
	onModView,
	...filters
}: FilterBarProps & {
	gameId: string;
	status: Status;
	onSortByName: () => void | Promise<void>;
	onAddSeparator: (name: string) => void | Promise<void>;
	onImportCommunityProfile: () => void;
	modView: ModView;
	onModView: (view: ModView) => void;
}) {
	const { t, i18n } = useTranslation();
	const { openDialog } = useUiActions();
	const { openLink: openCollection } = useCollections();
	const language = i18n.language;
	const { updatesReady, gameActions, modUpdates, pickArchives: onImport, copyToClipboard: onCopyText } = useWorkspace();
	const { currentBrowserProviders } = useSettingsActions();
	const sourceName = useSourceName();
	const onCheckUpdates = modUpdates.checkUpdates;
	const onUpdateAll = gameActions.updateAll;
	const {
		rescan: onRescan,
		sortByRules: onSortByRules,
		importMo2List: onImportMo2List,
		bisectStart: onBisectStart,
		deleteUnusedVersions: onDeleteUnusedVersions,
		undoList: onUndoList,
		redoList: onRedoList,
		restoreList: onRestoreList,
	} = gameActions;
	const { filtering } = filters;
	const [confirmUnused, setConfirmUnused] = useState(false);
	const [previewUpdates, setPreviewUpdates] = useState(false);
	const mods = status.mods;
	const targets = updateTargets(mods);
	const updateCount = targets.length;
	// The report says which updates exist; the backend's route says which of them need the files page. The synchronous
	// accessor keeps the count in step with a key saved just before this screen rendered.
	const browserOnly = currentBrowserProviders();
	const manualCount = targets.filter((target) => browserOnly.has(target.provider)).length;
	const activeProfile = status.profiles.find((profile) => profile.active);
	const enabledCount = mods.reduce((count, mod) => count + (mod.enabled ? 1 : 0), 0);
	const labels = {
		enabled: t("mods.state.enabled"),
		disabled: t("mods.state.disabled"),
		requires: t("mods.col.requires"),
		or: t("mods.relations.requirementsOr"),
	};
	const copyList = (format: ListFormat) => void onCopyText(formatModList(mods, format, labels, sourceName));

	return (
		<div className="flex shrink-0 flex-wrap items-center gap-2 border-b px-6 py-3">
			<FilterBar {...filters} />
			<div className="ml-auto flex items-center gap-1.5">
				<ToggleGroup
					type="single"
					variant="outline"
					size="sm"
					value={modView}
					aria-label={t("mods.view.label")}
					onValueChange={(value) => {
						// Radix sends an empty value when the pressed item is pressed again; the choice then stays.
						if (value === "list" || value === "grid") onModView(value);
					}}
				>
					<ToggleGroupItem value="list" aria-label={t("mods.view.list")} title={t("mods.view.list")}>
						<List />
					</ToggleGroupItem>
					<ToggleGroupItem value="grid" aria-label={t("mods.view.grid")} title={t("mods.view.grid")}>
						<LayoutGrid />
					</ToggleGroupItem>
				</ToggleGroup>
				<HistoryMenu
					gameId={gameId}
					profileName={activeProfile?.name ?? ""}
					total={mods.length}
					enabled={enabledCount}
					history={status.history}
					locale={language}
					onUndo={onUndoList}
					onRedo={onRedoList}
					onRestore={onRestoreList}
				/>
				<SeparatorButton disabled={filtering} onAdd={onAddSeparator} />
				{!updatesReady && (
					<span id="updates-need-key" className="sr-only">
						{t("updates.needKey")}
					</span>
				)}
				<Tooltip>
					<TooltipTrigger asChild>
						<span className="inline-flex">
							<Button
								variant="outline"
								aria-label={t("updates.button")}
								aria-describedby={!updatesReady ? "updates-need-key" : undefined}
								title={t("updates.button")}
								className="@max-5xl:px-2.5"
								onClick={() => void onCheckUpdates()}
								disabled={!updatesReady}
							>
								<CircleArrowUp /> <span className="hidden @5xl:inline">{t("updates.button")}</span>
							</Button>
						</span>
					</TooltipTrigger>
					{!updatesReady && <TooltipContent className="max-w-64">{t("updates.needKey")}</TooltipContent>}
				</Tooltip>
				{!updatesReady || updateCount === 0 ? (
					<span id="update-all-reason" className="sr-only">
						{!updatesReady ? t("updates.needKey") : t("palette.reason.noUpdates")}
					</span>
				) : null}
				<Tooltip>
					<TooltipTrigger asChild>
						<span className="inline-flex">
							<Button
								variant="outline"
								aria-label={t("updates.all", { count: updateCount })}
								aria-describedby={!updatesReady || updateCount === 0 ? "update-all-reason" : undefined}
								title={updatesReady ? t("updates.all", { count: updateCount }) : t("updates.needKey")}
								className="@max-5xl:px-2.5"
								onClick={() => setPreviewUpdates(true)}
								disabled={!updatesReady || updateCount === 0}
							>
								<Download /> <span className="hidden @5xl:inline">{t("updates.all", { count: updateCount })}</span>
							</Button>
						</span>
					</TooltipTrigger>
					{!updatesReady && <TooltipContent className="max-w-64">{t("updates.needKey")}</TooltipContent>}
					{updatesReady && updateCount === 0 && <TooltipContent className="max-w-64">{t("palette.reason.noUpdates")}</TooltipContent>}
				</Tooltip>
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button variant="ghost" size="icon" aria-label={t("mods.more")} title={t("mods.more")}>
							<EllipsisVertical />
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent className="w-72">
						<DropdownMenuLabel>{t("mods.menu.mods")}</DropdownMenuLabel>
						<DropdownMenuItem onSelect={() => void onImport()}>
							<Upload /> {t("mods.import")}
						</DropdownMenuItem>
						<DropdownMenuItem onSelect={openCollection}>
							<Boxes /> {t("collection.installAction")}
						</DropdownMenuItem>
						<DropdownMenuItem disabled={filtering} title={t("mods.sortByName")} onSelect={() => void onSortByName()}>
							<ArrowDownAZ /> {t("mods.sortAlpha")}
						</DropdownMenuItem>
						<DropdownMenuItem disabled={filtering} title={t("mods.sortByRulesHint")} onSelect={() => void onSortByRules()}>
							<ListTree /> {t("mods.sortByRules")}
						</DropdownMenuItem>
						<DropdownMenuItem onSelect={() => openDialog({ kind: "translation-hints" })}>
							<Languages /> {t("mods.hints.action")}
						</DropdownMenuItem>
						<DropdownMenuItem onSelect={() => void onRescan()}>
							<RefreshCw /> {t("mods.rescan")}
						</DropdownMenuItem>
						<DropdownMenuSeparator />
						<DropdownMenuLabel>{t("mods.menu.maintenance")}</DropdownMenuLabel>
						<DropdownMenuItem
							disabled={status.bisect !== null || enabledCount < 2}
							title={t("bisect.hint")}
							onSelect={() => void onBisectStart()}
						>
							<SearchCheck /> {t("bisect.start")}
						</DropdownMenuItem>
						<DropdownMenuItem onSelect={() => setConfirmUnused(true)}>
							<Trash2 /> {t("mods.deleteUnusedVersions")}
						</DropdownMenuItem>
						<DropdownMenuSeparator />
						<DropdownMenuLabel>{t("mods.menu.copy")}</DropdownMenuLabel>
						{LIST_FORMATS.map((format) => (
							<DropdownMenuItem key={format} disabled={mods.length === 0} title={t("mods.copyListHint")} onSelect={() => copyList(format)}>
								<Copy /> {t("mods.copyList", { format: t(`mods.copyFormat.${format}`) })}
							</DropdownMenuItem>
						))}
						<DropdownMenuSeparator />
						<DropdownMenuLabel>{t("mods.menu.import")}</DropdownMenuLabel>
						<DropdownMenuItem title={t("mods.importMo2Hint")} onSelect={() => void onImportMo2List()}>
							<FileInput /> {t("mods.importFrom", { name: "Mod Organizer 2" })}
						</DropdownMenuItem>
						<DropdownMenuItem title={t("community.hint")} onSelect={onImportCommunityProfile}>
							<FileUp /> {t("mods.importFrom", { name: "r2modman/Gale" })}
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
			<ConfirmDialog
				open={confirmUnused}
				onOpenChange={setConfirmUnused}
				title={t("mods.deleteUnusedConfirm.title")}
				description={t("mods.deleteUnusedConfirm.body")}
				confirmLabel={t("mods.deleteUnusedConfirm.confirm")}
				onConfirm={() => void onDeleteUnusedVersions()}
			/>
			<UpdateAllDialog
				open={previewUpdates}
				onOpenChange={setPreviewUpdates}
				targets={targets}
				manualCount={manualCount}
				onConfirm={onUpdateAll}
			/>
		</div>
	);
}
