import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { cn } from "cn";
import { Fragment, type ReactNode } from "react";
import type { ModExtras, UpdateReport } from "@/bindings";
import { useTranslation } from "@/i18n";
import type { ColumnId, ColumnSpec } from "@/lib/mod-columns";
import { activeConflicts, formatSize, type Group, hasUpdate, type Mod } from "@/lib/mods";
import { ConflictText, CountCell, DateCell } from "@/views/mod-table/cells";
import { ROW } from "@/views/mod-table/layout";
import { GroupMenu, PageDragHandle, PageFoldButton, PageMenu, PageToggle } from "@/views/mod-table/page-controls";

interface GroupRowProps {
	/// A source page or a stored user group. Both render one header over their parts; the cell logic is shared.
	group: Group;
	columns: ColumnSpec[];
	folded: boolean;
	filtering: boolean;
	updates: Record<string, UpdateReport>;
	/// The page text and relation data of the parts, loaded for the rows in the viewport when a column needs it.
	extras: ReadonlyMap<string, ModExtras>;
	locale: string;
	/// The row's position in the whole list and the list size, for a screen reader while rows are virtualized.
	position?: number;
	setSize?: number;
	onToggle: (enabled: boolean) => void | Promise<void>;
	onFold: () => void;
	onOpenSource: () => void;
	onDeletePage: () => void;
	onRenameGroup: () => void;
	onUngroup: () => void;
	onSetTranslationWinner: () => void;
}

/// A text the parts of a page share or add up to. A cell with nothing to show stays blank, so a header does not carry a
/// dash in every column.
function GroupText({ value }: { value: string | null | undefined }) {
	const text = value ?? null;
	return (
		<span className="text-muted-foreground block truncate text-xs" title={text ?? undefined}>
			{text}
		</span>
	);
}

/// The header of a source page or a user group. The parts below it are the real profile entries; this row only groups
/// them. It shows what the parts have in common or add up to, and nothing else: the parts keep their own versions and
/// states. A page keeps its name, author, category and menu; a user group uses its stored name and language.
export function PageRow({
	group,
	columns,
	folded,
	filtering,
	updates,
	extras,
	locale,
	position,
	setSize,
	onToggle,
	onFold,
	onOpenSource,
	onDeletePage,
	onRenameGroup,
	onUngroup,
	onSetTranslationWinner,
}: GroupRowProps) {
	const { t } = useTranslation();
	const mods = group.parts;
	const name = group.name;
	const author = group.page?.author ?? null;
	const category = group.page?.category ?? group.group?.language ?? null;
	const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
		id: group.id,
		disabled: filtering,
	});
	const style = { transform: CSS.Transform.toString(transform), transition };
	const all = mods.every((mod) => mod.enabled);
	const none = mods.every((mod) => !mod.enabled);
	const updateCount = mods.filter((mod) => hasUpdate(updates, mod.id)).length;
	const checkedAt = mods.reduce<number | null>((latest, mod) => {
		const at = mod.source?.checked_at;
		return at != null && (latest === null || at > latest) ? at : latest;
	}, null);
	// The same words as a part's version line, so a header and its parts read the same way.
	let status = t("mods.version.upToDate");

	if (updateCount > 0) status = t("mods.page.updates", { count: updateCount });
	else if (mods.some((mod) => updates[mod.id]?.error)) status = t("mods.version.checkFailed");
	else if (checkedAt == null) status = t("mods.version.notChecked");
	const conflicts = mods.reduce(
		(total, mod) => {
			const active = activeConflicts(mod);
			return { wins: total.wins + active.wins, loses: total.loses + active.loses };
		},
		{ wins: 0, loses: 0 },
	);
	// Size, files and versions add up over every part, as each part shows its own value whatever its switch says.
	const sizeKb = mods.reduce((total, mod) => total + mod.size_kb, 0);
	const files = mods.reduce((total, mod) => total + mod.files, 0);
	const versions = mods.reduce((total, mod) => total + mod.version_count, 0);
	const profiles = [...new Set(mods.flatMap((mod) => mod.profiles))];
	const source = mods.find((mod) => mod.sources.length > 0)?.sources[0];
	const versionsSeen = new Set(mods.map((mod) => mod.info.version ?? ""));
	const oneVersion = versionsSeen.size === 1 ? mods[0]?.info.version : null;

	const dateOrBlank = (seconds: number | null | undefined): ReactNode =>
		seconds == null ? <span /> : <DateCell seconds={seconds} locale={locale} />;
	const countOrBlank = (value: number | null | undefined): ReactNode =>
		value == null ? <span /> : <CountCell value={value} locale={locale} />;
	/// The distinct strings the parts of a page carry, for a grouped text cell.
	const shared = (values: (mod: Mod) => string[]): string => [...new Set(mods.flatMap((mod) => values(mod)))].join(", ");
	/// The first part that names a value. Numbers keep zero; text skips an empty string.
	const firstDefined = <T,>(pick: (mod: Mod) => T | null | undefined): T | undefined =>
		mods.map((mod) => pick(mod)).find((value) => value != null);
	const firstText = (pick: (mod: Mod) => string | null | undefined): string | undefined =>
		mods.map((mod) => pick(mod)).find((value) => value) ?? undefined;

	// The columns are one flat set shared with the header; a switch keeps every case visible next to the column it maps.
	const cell = (id: ColumnId): ReactNode => {
		switch (id) {
			case "order": {
				// The parts sit next to each other, so their priorities are one range.
				const first = Math.min(...mods.map((mod) => mod.priority));
				const last = Math.max(...mods.map((mod) => mod.priority));
				return (
					<span className="text-muted-foreground text-right font-mono text-sm tabular-nums">
						{first === last ? first : `${first}–${last}`}
					</span>
				);
			}
			case "load":
			case "sourceFile":
				return <span />;
			case "enabled":
				return (
					<PageToggle all={all} none={none} onToggle={() => void onToggle(!all)} label={group.page ? undefined : t("mods.group.toggle")} />
				);
			case "mod":
				return (
					<div className="flex min-w-0 items-center gap-1">
						<PageFoldButton name={name} folded={folded} onFold={onFold} />
						<button
							type="button"
							className="focus-visible:ring-ring grid min-w-0 flex-1 gap-0.5 rounded-sm py-1 text-left outline-none focus-visible:ring-2"
							onClick={onFold}
						>
							<span className="truncate text-sm font-semibold" title={name}>
								{name}
							</span>
							<span className="text-subtle-foreground flex min-w-0 items-center gap-1.5 text-xs">
								{author && <span className="truncate">{author}</span>}
								{author && <span className="shrink-0">{"·"}</span>}
								<span className="shrink-0">
									{group.page ? t("mods.page.parts", { count: mods.length }) : t("mods.group.mods", { count: mods.length })}
								</span>
							</span>
						</button>
						{group.page ? (
							<PageMenu
								name={name}
								folded={folded}
								all={all}
								hasSource={source !== undefined}
								onFold={onFold}
								onToggleAll={() => void onToggle(!all)}
								onOpenSource={onOpenSource}
								onDeletePage={onDeletePage}
							/>
						) : (
							<GroupMenu
								name={name}
								folded={folded}
								all={all}
								kind={group.group?.kind ?? "bundle"}
								onFold={onFold}
								onToggleAll={() => void onToggle(!all)}
								onRename={onRenameGroup}
								onSetTranslationWinner={onSetTranslationWinner}
								onUngroup={onUngroup}
							/>
						)}
					</div>
				);
			case "version":
				return (
					<div className="grid min-w-0 gap-0.5 text-xs">
						{oneVersion && <span className="truncate font-mono text-sm tabular-nums">{oneVersion}</span>}
						<span className={updateCount > 0 ? "text-update font-medium" : "text-muted-foreground"}>{status}</span>
					</div>
				);
			case "conflicts":
				if (conflicts.wins === 0 && conflicts.loses === 0) return <span />;
				return <ConflictText wins={conflicts.wins} loses={conflicts.loses} />;
			case "profiles":
				return <GroupText value={profiles.join(", ")} />;
			case "collections":
				return <GroupText value={shared((mod) => mod.collections)} />;
			case "slot": {
				const slots = new Set(mods.map((mod) => mod.slot));
				return <GroupText value={slots.size === 1 ? mods[0]?.slot : null} />;
			}
			case "missing": {
				const missing = mods.reduce((total, mod) => total + mod.unmet_requirements, 0);

				if (missing === 0) return <span />;
				return <span className="text-destructive text-right font-mono text-xs font-medium tabular-nums">{missing}</span>;
			}
			case "requires":
				return <GroupText value={shared((mod) => mod.requires_names)} />;
			case "sourceRequirements":
				return <GroupText value={shared((mod) => mod.requirement_groups.flat())} />;
			case "incompatible":
				return <GroupText value={shared((mod) => extras.get(mod.folder)?.incompatible_names ?? [])} />;
			case "overrides":
				return <GroupText value={shared((mod) => extras.get(mod.folder)?.overrides_names ?? [])} />;
			case "stepsBefore":
				return <GroupText value={shared((mod) => extras.get(mod.folder)?.steps_before ?? [])} />;
			case "stepsAfter":
				return <GroupText value={shared((mod) => extras.get(mod.folder)?.steps_after ?? [])} />;
			case "requirementsRead":
				return dateOrBlank(checkedAt);
			case "dlc":
				return <GroupText value={shared((mod) => extras.get(mod.folder)?.source_dlcs.flatMap((entry) => entry.dlcs.flat()) ?? [])} />;
			case "category":
				return <GroupText value={category} />;
			case "size":
				return <span className="text-right font-mono text-xs tabular-nums">{formatSize(sizeKb, locale)}</span>;
			case "files":
				return <span className="text-right font-mono text-xs tabular-nums">{files}</span>;
			case "versions":
				return <span className="text-right font-mono text-xs tabular-nums">{versions}</span>;
			case "installed":
				return dateOrBlank(Math.max(...mods.map((mod) => mod.installed_at)));
			case "folder":
				return <GroupText value={mods.map((mod) => mod.folder).join(", ")} />;
			case "archive":
				return <GroupText value={shared((mod) => (mod.archive ? [mod.archive] : []))} />;
			case "archiveFile":
				return <GroupText value={shared((mod) => (mod.archive_file ? [mod.archive_file] : []))} />;
			case "choices":
				return <GroupText value={shared((mod) => ((extras.get(mod.folder)?.fomod.length ?? 0) > 0 ? [mod.info.name] : []))} />;
			case "author":
				return <GroupText value={author} />;
			case "uploader":
				return <GroupText value={mods.find((mod) => mod.source?.uploader)?.source?.uploader} />;
			case "sourceName":
				return <GroupText value={name} />;
			case "sourceId":
				return <span className="text-right font-mono text-xs tabular-nums">{source?.mod_id}</span>;
			case "endorsements":
				return countOrBlank(firstDefined((mod) => extras.get(mod.folder)?.source?.endorsements));
			case "downloads":
				return countOrBlank(firstDefined((mod) => extras.get(mod.folder)?.source?.downloads));
			case "uniqueDownloads":
				return countOrBlank(firstDefined((mod) => extras.get(mod.folder)?.source?.unique_downloads));
			case "adult":
				return <GroupText value={mods.some((mod) => mod.source?.adult) ? t("mods.col.adultYes") : null} />;
			case "sourceStatus":
				return <GroupText value={firstText((mod) => extras.get(mod.folder)?.source?.status)} />;
			case "sourceCreated":
				return dateOrBlank(firstDefined((mod) => extras.get(mod.folder)?.source?.created_at));
			case "sourceUpdated":
				return dateOrBlank(firstDefined((mod) => extras.get(mod.folder)?.source?.updated_at));
			case "checked":
				return dateOrBlank(checkedAt);
			case "sourceTags":
				return <GroupText value={shared((mod) => extras.get(mod.folder)?.source?.tags ?? [])} />;
			case "tags":
				return <GroupText value={shared((mod) => mod.tags)} />;
			case "notes":
				return <GroupText value={firstText((mod) => extras.get(mod.folder)?.notes)} />;
			case "description":
				return <GroupText value={firstText((mod) => extras.get(mod.folder)?.info_description)} />;
			case "summary":
				return <GroupText value={firstText((mod) => extras.get(mod.folder)?.source?.summary)} />;
			case "about":
				return <GroupText value={firstText((mod) => extras.get(mod.folder)?.source?.description)} />;
			case "instructions":
				return <GroupText value={mods.find((mod) => mod.instructions)?.instructions} />;
			default:
				return <span />;
		}
	};

	return (
		<>
			{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
			<div
				ref={setNodeRef}
				style={style}
				data-grid-row
				role="row"
				tabIndex={-1}
				aria-rowindex={position === undefined ? undefined : position + 1}
				aria-posinset={position}
				aria-setsize={setSize}
				className={cn(ROW, "bg-muted/40 h-14 border-b py-1", isDragging && "bg-muted relative z-10 opacity-90 shadow-md")}
			>
				<PageDragHandle
					name={name}
					filtering={filtering}
					setActivatorNodeRef={setActivatorNodeRef}
					attributes={attributes}
					listeners={listeners}
				/>
				{columns.map((column) => (
					<Fragment key={column.id}>{cell(column.id)}</Fragment>
				))}
			</div>
		</>
	);
}
