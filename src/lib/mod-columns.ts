/// The columns of the mod list: what each one is, which ones always stay, and how a layout the user chose is kept. The view
/// draws them; this file has no React, so the rules can be tested.

export type ColumnId =
	| "order"
	| "enabled"
	| "mod"
	| "version"
	| "conflicts"
	| "load"
	| "slot"
	| "profiles"
	| "collections"
	| "missing"
	| "requires"
	| "sourceRequirements"
	| "incompatible"
	| "overrides"
	| "stepsBefore"
	| "stepsAfter"
	| "requirementsRead"
	| "dlc"
	| "category"
	| "size"
	| "files"
	| "versions"
	| "installed"
	| "folder"
	| "archive"
	| "archiveFile"
	| "choices"
	| "author"
	| "uploader"
	| "sourceName"
	| "sourceId"
	| "sourceFile"
	| "endorsements"
	| "downloads"
	| "uniqueDownloads"
	| "adult"
	| "sourceStatus"
	| "sourceCreated"
	| "sourceUpdated"
	| "checked"
	| "sourceTags"
	| "tags"
	| "notes"
	| "description"
	| "summary"
	| "about"
	| "instructions";

/// The part of the Columns menu a column belongs to.
export type ColumnGroup = "list" | "rules" | "files" | "source" | "text";

export interface ColumnSpec {
	id: ColumnId;
	/// The starting width in pixels. The Mod column has none until the user sets one: it takes the space that is left.
	width: number;
	min: number;
	/// Numbers and IDs sit at the right edge of their column.
	end?: boolean;
	/// Always shown, so the menu cannot hide it: the switch and the name of the mod.
	pinned?: boolean;
	/// Off until the user turns it on from the Columns menu.
	defaultHidden?: boolean;
	group: ColumnGroup;
}

/// Every column after the grip, in the order a new list starts with. The grip is always there, because it is the mouse
/// handle for reordering. The defaults show what a player reads first; the rest is one click away.
export const COLUMNS: ColumnSpec[] = [
	{ id: "order", width: 64, min: 48, end: true, group: "list" },
	{ id: "enabled", width: 64, min: 56, pinned: true, group: "list" },
	{ id: "mod", width: 0, min: 200, pinned: true, group: "list" },
	{ id: "version", width: 168, min: 120, group: "list" },
	{ id: "conflicts", width: 150, min: 96, group: "list" },
	{
		id: "load",
		width: 96,
		min: 72,
		end: true,
		defaultHidden: true,
		group: "list",
	},
	{ id: "slot", width: 120, min: 80, defaultHidden: true, group: "list" },
	{ id: "profiles", width: 200, min: 120, defaultHidden: true, group: "list" },
	{
		id: "collections",
		width: 200,
		min: 120,
		defaultHidden: true,
		group: "list",
	},
	{ id: "missing", width: 110, min: 84, end: true, group: "rules" },
	{ id: "requires", width: 200, min: 120, defaultHidden: true, group: "rules" },
	{
		id: "sourceRequirements",
		width: 200,
		min: 120,
		defaultHidden: true,
		group: "rules",
	},
	{
		id: "incompatible",
		width: 200,
		min: 120,
		defaultHidden: true,
		group: "rules",
	},
	{
		id: "overrides",
		width: 200,
		min: 120,
		defaultHidden: true,
		group: "rules",
	},
	{
		id: "stepsBefore",
		width: 140,
		min: 96,
		defaultHidden: true,
		group: "rules",
	},
	{
		id: "stepsAfter",
		width: 140,
		min: 96,
		defaultHidden: true,
		group: "rules",
	},
	{
		id: "requirementsRead",
		width: 130,
		min: 96,
		defaultHidden: true,
		group: "rules",
	},
	{ id: "dlc", width: 200, min: 120, defaultHidden: true, group: "rules" },
	{ id: "category", width: 150, min: 96, group: "source" },
	{ id: "size", width: 92, min: 72, end: true, group: "files" },
	{
		id: "files",
		width: 72,
		min: 56,
		end: true,
		defaultHidden: true,
		group: "files",
	},
	{
		id: "versions",
		width: 80,
		min: 64,
		end: true,
		defaultHidden: true,
		group: "files",
	},
	{ id: "installed", width: 130, min: 96, defaultHidden: true, group: "files" },
	{ id: "folder", width: 200, min: 120, defaultHidden: true, group: "files" },
	{ id: "archive", width: 240, min: 120, defaultHidden: true, group: "files" },
	{
		id: "archiveFile",
		width: 240,
		min: 120,
		defaultHidden: true,
		group: "files",
	},
	{ id: "choices", width: 200, min: 120, defaultHidden: true, group: "files" },
	{ id: "author", width: 160, min: 96, defaultHidden: true, group: "source" },
	{ id: "uploader", width: 160, min: 96, defaultHidden: true, group: "source" },
	{
		id: "sourceName",
		width: 200,
		min: 120,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "sourceId",
		width: 110,
		min: 72,
		end: true,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "sourceFile",
		width: 96,
		min: 72,
		end: true,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "endorsements",
		width: 110,
		min: 80,
		end: true,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "downloads",
		width: 110,
		min: 80,
		end: true,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "uniqueDownloads",
		width: 120,
		min: 88,
		end: true,
		defaultHidden: true,
		group: "source",
	},
	{ id: "adult", width: 96, min: 72, defaultHidden: true, group: "source" },
	{
		id: "sourceStatus",
		width: 140,
		min: 96,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "sourceCreated",
		width: 130,
		min: 96,
		defaultHidden: true,
		group: "source",
	},
	{
		id: "sourceUpdated",
		width: 130,
		min: 96,
		defaultHidden: true,
		group: "source",
	},
	{ id: "checked", width: 130, min: 96, defaultHidden: true, group: "source" },
	{
		id: "sourceTags",
		width: 200,
		min: 96,
		defaultHidden: true,
		group: "source",
	},
	{ id: "tags", width: 200, min: 96, defaultHidden: true, group: "text" },
	{ id: "notes", width: 240, min: 120, defaultHidden: true, group: "text" },
	{
		id: "description",
		width: 300,
		min: 120,
		defaultHidden: true,
		group: "text",
	},
	{ id: "summary", width: 260, min: 120, defaultHidden: true, group: "text" },
	{ id: "about", width: 300, min: 120, defaultHidden: true, group: "text" },
	{
		id: "instructions",
		width: 240,
		min: 120,
		defaultHidden: true,
		group: "text",
	},
];

export interface ColumnLayout {
	/// Widths the user set, in pixels, by column. A column without one has its starting width.
	widths: Partial<Record<ColumnId, number>>;
	/// Columns the user turned off. Pinned columns are never here.
	hidden: ColumnId[];
	/// Every column, in the order the user wants them. Hidden columns keep their place here too.
	order: ColumnId[];
}

/// Columns whose value is not in the light list row. The table asks for extras only while one of them is visible.
export const EXTRA_COLUMNS: ReadonlySet<ColumnId> = new Set<ColumnId>([
	"incompatible",
	"overrides",
	"stepsBefore",
	"stepsAfter",
	"requirementsRead",
	"dlc",
	"choices",
	"endorsements",
	"downloads",
	"uniqueDownloads",
	"sourceStatus",
	"sourceCreated",
	"sourceUpdated",
	"sourceTags",
	"notes",
	"description",
	"summary",
	"about",
]);

export const DEFAULT_LAYOUT: ColumnLayout = {
	widths: {},
	hidden: COLUMNS.filter((column) => column.defaultHidden).map((column) => column.id),
	order: COLUMNS.map((column) => column.id),
};

const COLUMN_IDS = new Set<string>(COLUMNS.map((column) => column.id));
export const isColumnId = (value: unknown): value is ColumnId => typeof value === "string" && COLUMN_IDS.has(value);

const isPinned = (id: ColumnId): boolean => COLUMNS.some((column) => column.id === id && column.pinned === true);

/// The column specs in the order of `order`.
export function orderedColumns(order: ColumnId[]): ColumnSpec[] {
	return order.flatMap((id) => COLUMNS.filter((column) => column.id === id));
}

/// A layout as it was saved: an earlier version saved none with an order, and may have saved pinned columns as hidden.
export interface SavedLayout {
	widths: ColumnLayout["widths"];
	hidden: ColumnId[];
	order?: ColumnId[];
}

/// A list in which every entry is a column, and none is named twice.
const namesColumnsOnce = (list: unknown): boolean =>
	Array.isArray(list) && list.every((id) => isColumnId(id)) && new Set(list).size === list.length;

/// A saved layout whose shape is right: widths are numbers, and every column named in it exists, once.
export function isSavedLayout(value: unknown): value is SavedLayout {
	if (typeof value !== "object" || value === null || !("widths" in value) || !("hidden" in value)) return false;
	const { widths, hidden } = value;
	const order = "order" in value ? value.order : undefined;
	return (
		typeof widths === "object" &&
		widths !== null &&
		Object.entries(widths).every(([id, width]) => isColumnId(id) && typeof width === "number" && width > 0) &&
		namesColumnsOnce(hidden) &&
		(order === undefined || namesColumnsOnce(order))
	);
}

/// The layout the view uses for a saved one, or the default when nothing was saved. Columns the save does not name come
/// last in their default order, and a pinned column is never hidden.
export function restoreLayout(saved: SavedLayout | null): ColumnLayout {
	if (!saved) return DEFAULT_LAYOUT;
	const order = [...new Set([...(saved.order ?? []), ...DEFAULT_LAYOUT.order])];
	const widths: ColumnLayout["widths"] = {};

	for (const [id, width] of Object.entries(saved.widths)) {
		if (!isColumnId(id) || typeof width !== "number") continue;
		const min = COLUMNS.find((column) => column.id === id)?.min ?? 1;
		widths[id] = Math.max(min, width);
	}

	return {
		widths,
		hidden: saved.hidden.filter((id) => !isPinned(id)),
		order,
	};
}

export function moveColumnTo(order: ColumnId[], id: ColumnId, target: ColumnId): ColumnId[] {
	const from = order.indexOf(id);
	const to = order.indexOf(target);

	if (from < 0 || to < 0 || from === to) return order;
	const next = [...order];
	next.splice(from, 1);
	next.splice(to, 0, id);
	return next;
}

/// The order with `id` swapped with its neighbour among the visible columns, one step left (-1) or right (1). Hidden
/// columns keep their places; at the end of the list nothing moves.
export function stepColumn(order: ColumnId[], visible: ColumnId[], id: ColumnId, step: -1 | 1): ColumnId[] {
	const at = visible.indexOf(id);
	const neighbour = at < 0 ? undefined : visible[at + step];

	if (neighbour === undefined) return order;
	const from = order.indexOf(id);
	const to = order.indexOf(neighbour);

	if (from < 0 || to < 0) return order;
	const next = [...order];
	next[from] = neighbour;
	next[to] = id;
	return next;
}
