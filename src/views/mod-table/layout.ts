import {
	Activity,
	AlignLeft,
	Archive,
	Ban,
	Bookmark,
	Boxes,
	CalendarCheck,
	CalendarClock,
	CalendarPlus,
	ClipboardList,
	Download,
	EyeOff,
	FileArchive,
	FileDown,
	Files,
	FileText,
	Flag,
	Folder,
	FolderTree,
	Gauge,
	GitCompareArrows,
	Globe,
	HardDrive,
	Hash,
	History,
	Layers,
	LibraryBig,
	Link,
	ListChecks,
	ListOrdered,
	type LucideIcon,
	NotebookPen,
	Package,
	PackagePlus,
	PackageX,
	Play,
	Power,
	Puzzle,
	Replace,
	SlidersHorizontal,
	Tag,
	Tags,
	TextQuote,
	ThumbsUp,
	Type,
	Upload,
	UserRound,
	Users,
} from "lucide-react";
import type { CSSProperties } from "react";
import {
	type ColumnGroup,
	type ColumnId,
	type ColumnLayout,
	type ColumnSpec,
	DEFAULT_LAYOUT,
	isSavedLayout,
	restoreLayout,
	type SavedLayout,
} from "@/lib/mod-columns";
import { readPref, writePref } from "@/lib/ui-prefs";

const GRIP_WIDTH = 28;

export const EMPTY = "—";

/// Shared by the header and the rows: the track sizes come from a CSS variable, so a drag only changes one style. On a
/// wide window the extra space becomes gutter between the columns instead of one oversized cell.
export const ROW = "grid justify-between items-center gap-4 px-6 [grid-template-columns:var(--mod-columns)]";

/// The widest a column gets from fitting it to its content; a longer text is cut, as before.
export const FIT_MAX = 640;

/// The most rows a column fit measures. An explicit fit on a 10k list must not mount and lay out every row; an even
/// sample keeps a long name near either end in the measurement.
export const FIT_SAMPLE = 500;

/// At most `limit` rows, spread evenly and keeping the first. A short list comes back whole.
export function sampleRows<T>(rows: T[], limit = FIT_SAMPLE): T[] {
	if (rows.length <= limit) return rows;
	const picked: T[] = [];
	const step = rows.length / limit;

	for (let index = 0; index < limit; index += 1) {
		const row = rows[Math.floor(index * step)];

		if (row !== undefined) picked.push(row);
	}

	return picked;
}

const LAYOUT_PREF = "columns";

/// A layout as the webview's storage gives it back: a saved one, or `null` when the layout was reset.
const isStoredLayout = (value: unknown): value is SavedLayout | null => value === null || isSavedLayout(value);

/// The layout of the table. It lives outside the component, so closing the game or switching games keeps it, and the
/// webview's storage keeps it for the next start.
let layoutForSession = restoreLayout(readPref<SavedLayout | null>(LAYOUT_PREF, null, isStoredLayout));

/// The layout chosen in this session, or the saved one when the session has not chosen yet.
export function sessionLayout(): ColumnLayout {
	return layoutForSession;
}

/// Keeps a layout. It is called from event handlers only, never while rendering.
export function rememberLayout(layout: ColumnLayout): ColumnLayout {
	layoutForSession = layout;
	writePref(LAYOUT_PREF, layout === DEFAULT_LAYOUT ? null : layout);
	return layout;
}

/// The parts of the Columns menu, in the order they are listed.
export const COLUMN_GROUPS: ColumnGroup[] = ["list", "rules", "files", "source", "text"];

/// The icon each column has in the Columns menu.
export const COLUMN_ICONS: Record<ColumnId, LucideIcon> = {
	order: ListOrdered,
	enabled: Power,
	mod: Package,
	version: Tag,
	conflicts: GitCompareArrows,
	load: Gauge,
	slot: FolderTree,
	profiles: LibraryBig,
	collections: Boxes,
	missing: PackageX,
	requires: Link,
	sourceRequirements: Globe,
	incompatible: Ban,
	overrides: Replace,
	stepsBefore: Play,
	stepsAfter: Flag,
	requirementsRead: ListChecks,
	dlc: Puzzle,
	category: Layers,
	size: HardDrive,
	files: Files,
	versions: History,
	installed: PackagePlus,
	folder: Folder,
	archive: Archive,
	archiveFile: FileArchive,
	choices: SlidersHorizontal,
	author: UserRound,
	uploader: Upload,
	sourceName: Type,
	sourceId: Hash,
	sourceFile: FileDown,
	endorsements: ThumbsUp,
	downloads: Download,
	uniqueDownloads: Users,
	adult: EyeOff,
	sourceStatus: Activity,
	sourceCreated: CalendarPlus,
	sourceUpdated: CalendarClock,
	checked: CalendarCheck,
	sourceTags: Bookmark,
	tags: Tags,
	notes: NotebookPen,
	description: TextQuote,
	summary: AlignLeft,
	about: FileText,
	instructions: ClipboardList,
};

/// The grid tracks of the grip and the visible columns. A column with a set width has that width. Without one, Mod fills
/// the space that is left up to `FIT_MAX` and the others have their starting width. `sizing` replaces the track of some
/// columns, to measure their content.
export function trackList(columns: ColumnSpec[], widths: ColumnLayout["widths"], sizing: Partial<Record<ColumnId, string>> = {}): string {
	const tracks = columns.map((column) => {
		const replaced = sizing[column.id];

		if (replaced !== undefined) return replaced;
		const width = widths[column.id];

		if (width !== undefined) return `${width}px`;
		return column.id === "mod" ? `minmax(${column.min}px,${FIT_MAX}px)` : `${column.width}px`;
	});
	return [`${GRIP_WIDTH}px`, ...tracks].join(" ");
}

/// Columns that give way, first to last, when the list has no room for all of them. The switch, Mod and Version stay.
const GIVE_WAY: ColumnId[] = [
	"about",
	"instructions",
	"summary",
	"description",
	"notes",
	"sourceTags",
	"tags",
	"choices",
	"dlc",
	"sourceRequirements",
	"requires",
	"incompatible",
	"overrides",
	"stepsAfter",
	"stepsBefore",
	"requirementsRead",
	"archiveFile",
	"archive",
	"folder",
	"sourceName",
	"sourceStatus",
	"sourceCreated",
	"sourceUpdated",
	"adult",
	"uniqueDownloads",
	"downloads",
	"endorsements",
	"sourceFile",
	"sourceId",
	"checked",
	"uploader",
	"author",
	"category",
	"installed",
	"versions",
	"slot",
	"collections",
	"profiles",
	"load",
	"files",
	"size",
	"conflicts",
	"missing",
	"order",
];

/// The row's padding and the gap before each column, as in `ROW`.
const ROW_PADDING = 48;

const COLUMN_GAP = 16;

/// The width a row needs for these columns: set widths, starting widths, and Mod at its smallest.
export function neededWidth(columns: ColumnSpec[], widths: ColumnLayout["widths"]): number {
	return columns.reduce(
		(sum, column) => sum + COLUMN_GAP + (widths[column.id] ?? (column.id === "mod" ? column.min : column.width)),
		GRIP_WIDTH + ROW_PADDING,
	);
}

/// The columns that fit in `room` pixels: the ones that give way go first. Without a measure, all of them.
export function fitting(columns: ColumnSpec[], widths: ColumnLayout["widths"], room: number | null): ColumnSpec[] {
	if (room === null) return columns;
	let shown = columns;

	for (const id of GIVE_WAY) {
		if (neededWidth(shown, widths) <= room) break;
		shown = shown.filter((column) => column.id !== id);
	}

	return shown;
}

/// The track sizes as a CSS variable. React's style type has no custom properties, so the variable is added with `Object.assign`.
export function trackVariable(template: string): CSSProperties {
	const style: CSSProperties = {};
	return Object.assign(style, { "--mod-columns": template });
}
