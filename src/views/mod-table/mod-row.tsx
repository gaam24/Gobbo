import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { cn } from "cn";
import { GripVertical } from "lucide-react";
import { Fragment, memo, type MouseEvent as ReactMouseEvent, type ReactNode } from "react";
import type { ModExtras, UpdateReport } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import type { ColumnId, ColumnSpec } from "@/lib/mod-columns";
import { alternativesText, authorOf, choicesText, formatSize, type Mod } from "@/lib/mods";
import { PageStatusCell } from "@/sources/status";
import { ConflictCell, CountCell, DateCell, TextCell, VersionCell } from "@/views/mod-table/cells";
import { EMPTY, ROW } from "@/views/mod-table/layout";
import type { GroupMenuActions } from "@/views/mod-table/mod-list-props";
import { itemClicks, ModMenu } from "@/views/mod-table/mod-menu";

/// Actions every row needs. The view passes stable callbacks, so a memoized row skips re-rendering
/// while its own mod, selection membership, and update report stay the same.
export interface ModRowActions {
	/// One stable lookup for the group a row's menu describes, plus its group operations.
	groupMenu: GroupMenuActions;
	onPick: (modId: string, event: ReactMouseEvent) => void;
	onToggle: (modId: string, enabled: boolean) => void | Promise<void>;
	onMove: (modId: string, delta: number) => void | Promise<void>;
	onOpenFolder: (path: string) => void | Promise<void>;
	onRevealFile: (path: string) => void | Promise<void>;
	/// Opens a mod page on its provider, on the files section when `tab` asks for it.
	onOpenSource: (provider: string, modId: string, tab?: "files") => void;
	onCheckUpdate: (modId: string) => void | Promise<void>;
	/// Opens the inspector on the conflicts tab of this mod.
	onOpenConflicts: (modId: string) => void;
	/// Downloads the newest provider file, installs it and selects it in the active profile.
	onUpdateMod: (modId: string, name: string, update: UpdateReport) => void | Promise<void>;
	/// Hides the provider's current version until it lists another one, or shows it again.
	onIgnoreVersion: (modId: string, provider: string) => void | Promise<void>;
	onUnignoreVersion: (modId: string, provider: string) => void | Promise<void>;
	/// Hides disabled mods from this profile's list, or shows them again.
	onSetHidden: (modIds: string[], hide: boolean) => void;
	/// The providers Gobbo can ask now; a mod can be checked when one of its sources is among them.
	readyProviders: ReadonlySet<string>;
	onRemoveFromLibrary: (modId: string) => void;
	onSelectVersion: (modId: string, folder: string) => void | Promise<void>;
	/// Opens the confirmation for deleting one installed version.
	onDeleteVersion: (modId: string, folder: string) => void;
	/// Unpacks the mod's folder again from its archive copy.
	onReinstall: (folder: string) => void | Promise<void>;
	/// Compares the installed folder with its archive copy and shows what was left out.
	onArchiveReport: (folder: string, name: string) => void;
	/// Opens the FOMOD wizard to reinstall with different installer options.
	onChangeInstaller: (modId: string, folder: string, name: string) => void;
	/// Puts a new separator above the mod and starts renaming it.
	onAddSeparatorAbove: (modId: string) => void;
	/// Picks the mod and brings its details into view.
	onShowDetails: (modId: string) => void;
}

/// One mod row with its context menu. Memoized: while typing in the search field only the rows whose
/// data changed re-render, not the whole table. The grip on the left is the only drag handle.
export const ModRow = memo(function ModRow({
	gameId,
	mod,
	columns,
	picked,
	focused,
	tabbable,
	filtering,
	isFirst,
	isLast,
	position,
	setSize,
	nested = false,
	hidden,
	update,
	extras,
	libraryDir,
	archivesDir,
	locale,
	...actions
}: ModRowActions & {
	gameId: string;
	mod: Mod;
	/// The visible columns, in order. The table keeps the same array while the layout does not change, so memoized rows hold.
	columns: ColumnSpec[];
	picked: boolean;
	focused: boolean;
	/// The one row the Tab key reaches. The selected row carries it; without a selection the first mounted row does.
	tabbable: boolean;
	filtering: boolean;
	isFirst: boolean;
	isLast: boolean;
	/// The row's position in the whole list and the list size, for a screen reader while rows are virtualized.
	position?: number;
	setSize?: number;
	/// The row is a part under a page header; it is indented under that header.
	nested?: boolean;
	/// The user hid the mod from this list; its menu offers to show it again.
	hidden: boolean;
	update: UpdateReport | undefined;
	/// The page text and relation data, loaded for the rows in the viewport when a column needs it.
	extras: ModExtras | undefined;
	libraryDir: string;
	archivesDir: string;
	locale: string;
}) {
	const { t } = useTranslation();
	const { onPick, onToggle, onSelectVersion, onDeleteVersion, onUpdateMod, onOpenConflicts } = actions;
	const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
		id: mod.id,
		disabled: filtering,
	});
	const style = { transform: CSS.Transform.toString(transform), transition };
	const author = authorOf(mod);
	/// The author sits under the name only while it has no column of its own.
	const authorHasColumn = columns.some((column) => column.id === "author");
	/// The version line downloads the update when the provider can serve it; the mod menu keeps the page as a fallback.
	const updateNow =
		update?.has_update && update.latest && !update.latest_installed ? () => onUpdateMod(mod.id, mod.info.name, update) : undefined;

	const cell = (id: ColumnId): ReactNode => {
		switch (id) {
			case "order":
				return <span className="text-muted-foreground text-right font-mono text-sm tabular-nums">{mod.priority}</span>;
			case "enabled":
				return (
					<Switch
						checked={mod.enabled}
						aria-label={t("mods.toggle", { name: mod.info.name })}
						onCheckedChange={(enabled) => void onToggle(mod.id, enabled)}
					/>
				);
			case "mod":
				return (
					<button
						type="button"
						data-mod={mod.id}
						aria-current={focused ? "true" : undefined}
						aria-pressed={picked}
						className={cn(
							"focus-visible:ring-ring grid min-w-0 gap-0.5 rounded-sm py-1 text-left outline-none focus-visible:ring-2",
							nested && "pl-8",
						)}
						onClick={(event) => onPick(mod.id, event)}
					>
						<span className={cn("truncate text-sm font-medium", !mod.enabled && "text-muted-foreground")} title={mod.info.name}>
							{mod.info.name}
						</span>
						{author && !authorHasColumn && <span className="text-subtle-foreground truncate text-xs">{author}</span>}
					</button>
				);
			case "version":
				return (
					<VersionCell
						// A new version count remounts the picker, so a cached list never hides an imported version.
						key={`${mod.folder}:${mod.version_count}`}
						gameId={gameId}
						mod={mod}
						update={update}
						onSelectVersion={onSelectVersion}
						onDeleteVersion={onDeleteVersion}
						onUpdate={updateNow}
					/>
				);
			case "conflicts":
				return <ConflictCell mod={mod} onOpen={() => onOpenConflicts(mod.id)} />;
			case "profiles":
				return <TextCell value={mod.profiles.join(", ")} />;
			case "collections":
				return <TextCell value={mod.collections.join(", ")} />;
			case "load":
				return <span className="text-right font-mono text-xs tabular-nums">{mod.load_priority ?? EMPTY}</span>;
			case "slot":
				return <TextCell value={mod.slot} />;
			case "missing":
				return (
					<span
						className={cn(
							"text-right font-mono text-xs font-medium tabular-nums",
							mod.unmet_requirements > 0 ? "text-destructive" : "text-subtle-foreground",
						)}
						title={t("mods.col.missingHint")}
					>
						{mod.unmet_requirements > 0 ? mod.unmet_requirements : EMPTY}
					</span>
				);
			case "requires":
				return <TextCell value={mod.requires_names.join(", ")} />;
			case "sourceRequirements":
				return <TextCell value={alternativesText(mod.requirement_groups, t("mods.relations.requirementsOr"))} />;
			case "incompatible":
				return <TextCell value={extras?.incompatible_names.join(", ")} />;
			case "overrides":
				return <TextCell value={extras?.overrides_names.join(", ")} />;
			case "stepsBefore":
				return <TextCell value={extras?.steps_before.join(", ")} />;
			case "stepsAfter":
				return <TextCell value={extras?.steps_after.join(", ")} />;
			case "requirementsRead":
				return <DateCell seconds={extras?.source_sync[0]?.checked_at} locale={locale} />;
			case "dlc":
				return (
					<TextCell
						value={alternativesText(extras?.source_dlcs.flatMap((entry) => entry.dlcs) ?? [], t("mods.relations.requirementsOr"))}
					/>
				);
			case "files":
				return <span className="text-right font-mono text-xs tabular-nums">{mod.files}</span>;
			case "versions":
				return <span className="text-right font-mono text-xs tabular-nums">{mod.version_count}</span>;
			case "installed":
				return <DateCell seconds={mod.installed_at} locale={locale} />;
			case "folder":
				return <TextCell value={mod.folder} />;
			case "size":
				return <span className="text-right font-mono text-xs tabular-nums">{formatSize(mod.size_kb, locale)}</span>;
			case "author":
				return <TextCell value={author} />;
			case "uploader":
				return <TextCell value={mod.source?.uploader} />;
			case "category":
				return <TextCell value={mod.source?.category} />;
			case "tags":
				return <TextCell value={mod.tags.join(", ")} />;
			case "sourceName":
				return <TextCell value={mod.source?.name} />;
			case "sourceFile":
				return (
					<span className="text-right font-mono text-xs tabular-nums">{mod.sources.find((entry) => entry.file_id)?.file_id ?? EMPTY}</span>
				);
			case "endorsements":
				return <CountCell value={extras?.source?.endorsements} locale={locale} />;
			case "downloads":
				return <CountCell value={extras?.source?.downloads} locale={locale} />;
			case "uniqueDownloads":
				return <CountCell value={extras?.source?.unique_downloads} locale={locale} />;
			case "adult":
				return <TextCell value={mod.source?.adult ? t("mods.col.adultYes") : null} />;
			case "sourceStatus":
				return <PageStatusCell status={extras?.source?.status} />;
			case "sourceCreated":
				return <DateCell seconds={extras?.source?.created_at} locale={locale} />;
			case "sourceUpdated":
				return <DateCell seconds={extras?.source?.updated_at} locale={locale} />;
			case "sourceId":
				return <span className="text-right font-mono text-xs tabular-nums">{mod.sources[0]?.mod_id ?? EMPTY}</span>;
			case "archiveFile":
				return <TextCell value={mod.archive_file} />;
			case "choices":
				return <TextCell value={choicesText(extras?.fomod ?? [])} />;
			case "archive":
				return <TextCell value={mod.archive} />;
			case "checked":
				return <DateCell seconds={mod.source?.checked_at} locale={locale} />;
			case "sourceTags":
				return <TextCell value={extras?.source?.tags.join(", ")} />;
			case "notes":
				return <TextCell value={extras?.notes} />;
			case "summary":
				return <TextCell value={extras?.source?.summary} />;
			case "about":
				return <TextCell value={extras?.source?.description} />;
			case "instructions":
				return <TextCell value={mod.instructions} />;
			case "description":
				return <TextCell value={extras?.info_description} />;
			default:
				return null;
		}
	};

	return (
		<ModMenu
			mod={mod}
			update={update}
			filtering={filtering}
			hidden={hidden}
			isFirst={isFirst}
			isLast={isLast}
			libraryDir={libraryDir}
			archivesDir={archivesDir}
			{...actions}
		>
			{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
			<div
				ref={setNodeRef}
				style={style}
				data-grid-row
				role="row"
				tabIndex={tabbable ? 0 : -1}
				aria-rowindex={position === undefined ? undefined : position + 1}
				aria-posinset={position}
				aria-setsize={setSize}
				{...itemClicks(mod.id, actions)}
				className={cn(
					ROW,
					// The virtual list reserves this exact height for the rows it does not mount.
					"h-14 cursor-default border-b py-1",
					nested && "bg-muted/10",
					picked ? "bg-muted" : "hover:bg-muted/50",
					isDragging && "bg-muted relative z-10 opacity-90 shadow-md",
				)}
			>
				<Button
					ref={setActivatorNodeRef}
					variant="ghost"
					size="icon-sm"
					aria-label={t("mods.dragHandle", { name: mod.info.name })}
					title={filtering ? t("mods.dragDisabled") : t("mods.dragHandle", { name: mod.info.name })}
					disabled={filtering}
					className="text-subtle-foreground cursor-grab touch-none active:cursor-grabbing disabled:cursor-default disabled:opacity-40"
					{...attributes}
					{...listeners}
					tabIndex={-1}
				>
					<GripVertical className="size-4" aria-hidden />
				</Button>
				{columns.map((column) => (
					<Fragment key={column.id}>{cell(column.id)}</Fragment>
				))}
			</div>
		</ModMenu>
	);
});
