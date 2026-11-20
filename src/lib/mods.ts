import type {
	Choice,
	FileState,
	KnownRows,
	ModDetails,
	ModExtras,
	ModGroup,
	ProfileView,
	Row,
	SourceSummary,
	Status,
	UpdateReport,
} from "@/bindings";

export type Mod = Status["mods"][number];

/// List filters. `missing` shows the mods whose requirements the profile does not meet, and `updates` the mods whose
/// selected version differs from the one their provider lists, from the last check. `hidden` shows the mods the list hides.
export type Filter = "all" | "enabled" | "disabled" | "missing" | "conflict" | "updates" | "hidden";

export type Separator = Status["separators"][number];

/// One row of the list as the user sees it: a mod, or a separator that heads the mods below it.
export type ListItem = { kind: "mod"; id: string; mod: Mod } | { kind: "separator"; id: string; name: string };

/// One page group: the parts of one source page that sit next to each other in the list.
export interface PageGroup {
	/// The row id, prefixed so it cannot collide with a mod family id.
	id: string;
	pageId: string;
	name: string;
	author: string | null;
	category: string | null;
	mods: Mod[];
}

/// A header and the mods below it. Page groups are derived from adjacent parts of one source page; user groups are
/// stored in the status. Both render as one header with its parts, so the list views can treat them the same way.
export interface Group {
	source: "page" | "user";
	/// The header row id, prefixed so it cannot collide with a mod family id.
	id: string;
	name: string;
	/// The visible mods under the header, in list order.
	parts: Mod[];
	/// The stored group, set when `source` is `"user"`.
	group?: ModGroup;
	/// The page details, set when `source` is `"page"`.
	page?: PageGroup;
}

/// A group that comes from the stored group list, not from a page run.
export type UserGroup = Group & { group: ModGroup };

/// A row of the table: a mod, a separator, a page header with its parts below it, or a user-group header with its parts
/// below it. `groupKey` ties a nested mod to the header it belongs to, so the grid keeps a group's cards together.
export type TableItem =
	| { kind: "mod"; id: string; mod: Mod; nested?: boolean; groupKey?: string }
	| { kind: "separator"; id: string; name: string }
	| { kind: "page"; id: string; page: PageGroup }
	| { kind: "group"; id: string; group: Group };

/// A row of the grid: a separator, a page or user-group header, or a row of cards. The separator and the headers take
/// the full width; the cards of a group follow its header.
export type GridItem =
	| { kind: "separator"; id: string; name: string }
	| { kind: "page"; id: string; page: PageGroup }
	| { kind: "group"; id: string; group: Group }
	| { kind: "cards"; mods: Mod[] };

/// The page key of a mod, when its provider page is known.
export function pageKeyOf(mod: Mod): string | null {
	return mod.page_id ?? null;
}

/// The prefix of a page header's row id. It keeps the id apart from a mod family id.
const PAGE_ROW_PREFIX = "page:";

/// The row id of a page group.
export function pageRowId(pageId: string): string {
	return `${PAGE_ROW_PREFIX}${pageId}`;
}

/// The prefix of a user-group header's row id. It keeps the id apart from a page row id and a mod family id.
const GROUP_ROW_PREFIX = "group:";

/// The row id of a user group header.
export function groupRowId(groupId: string): string {
	return `${GROUP_ROW_PREFIX}${groupId}`;
}

/// The installed parts of one source page, in list order. A page-wide command names the page by its first part, because
/// the backend works on families.
export function pagePartsOf(mods: readonly Mod[], pageId: string): Mod[] {
	return mods.filter((mod) => mod.page_id === pageId);
}

/// The index just after the run of entries from `start` whose key is `page`. The first entry with another key ends it.
function runEnd<T>(entries: readonly T[], start: number, page: string, keyOf: (entry: T) => string | null): number {
	let end = start;
	while (end < entries.length) {
		const entry = entries[end];
		if (entry === undefined || keyOf(entry) !== page) break;
		end += 1;
	}
	return end;
}

/// A page group from its first part. The name and the author come from the page when a page check read it.
function pageGroupOf(pageId: string, mods: Mod[]): PageGroup {
	const first = mods[0];
	const source = first?.source;
	return {
		id: pageRowId(pageId),
		pageId,
		name: first?.page_name ?? source?.name ?? pageId,
		author: first ? authorOf(first) : null,
		category: source?.category ?? null,
		mods,
	};
}

/// The stored groups that still have at least two visible members. Parts follow the order of `mods`, not the order in
/// which the group lists them, and a removed member is simply absent. A mod is in at most one group.
export function userGroups(mods: readonly Mod[], groups: readonly ModGroup[]): UserGroup[] {
	const result: UserGroup[] = [];
	for (const group of groups) {
		const members = new Set(group.members);
		const parts = mods.filter((mod) => members.has(mod.id));
		if (parts.length < 2) continue;
		result.push({ source: "user", id: groupRowId(group.id), name: group.name, parts, group });
	}
	return result;
}

/// The visible groups by part id. User groups are taken first; their parts are left out of the page runs so a page never
/// emits a header over rows that already have a user-group header.
function groupsOf(items: readonly ListItem[], stored: readonly ModGroup[]): Map<string, Group> {
	const byId = new Map<string, Group>();
	const visible = items.flatMap((item) => (item.kind === "mod" ? [item.mod] : []));
	const covered = new Set<string>();
	for (const group of userGroups(visible, stored)) {
		for (const part of group.parts) {
			byId.set(part.id, group);
			covered.add(part.id);
		}
	}
	let at = 0;
	while (at < items.length) {
		const row = items[at];
		if (row?.kind !== "mod" || covered.has(row.id)) {
			at += 1;
			continue;
		}
		const pageId = pageKeyOf(row.mod);
		if (pageId === null) {
			at += 1;
			continue;
		}
		// A covered mod or a separator ends the run, so page groups only form over rows still free of a user group.
		const keyOf = (item: ListItem): string | null => (item.kind === "mod" && !covered.has(item.id) ? pageKeyOf(item.mod) : null);
		const next = runEnd(items, at, pageId, keyOf);
		if (next - at >= 2) {
			const mods = items.slice(at, next).flatMap((entry) => (entry.kind === "mod" ? [entry.mod] : []));
			const page = pageGroupOf(pageId, mods);
			const group: Group = { source: "page", id: page.id, name: page.name, parts: mods, page };
			for (const mod of mods) byId.set(mod.id, group);
			at = next;
		} else {
			at += 1;
		}
	}
	return byId;
}

/// The group named by a header row id, page or user, or `undefined` when that header is gone.
function groupByRow(byId: ReadonlyMap<string, Group>, rowId: string): Group | undefined {
	for (const group of byId.values()) if (group.id === rowId) return group;
	return undefined;
}

/// The family ids under a group header, for a group-wide enable toggle. Works for both page and user group row ids.
export function groupMemberIds(items: ListItem[], rowId: string, groups: readonly ModGroup[] = []): string[] {
	const group = groupByRow(groupsOf(items, groups), rowId);
	return group ? group.parts.map((part) => part.id) : [];
}

/// The table rows. Adjacent mods of one source page or a stored user group become one header and its parts; a group with
/// one visible part stays a plain mod row. A folded header hides its parts, but keeps the header. `groups` are the stored
/// user groups; without them the page rule is the only one, as before.
export function tableItems(rows: ListItem[], foldedPages: ReadonlySet<string> = new Set(), groups: readonly ModGroup[] = []): TableItem[] {
	const byId = groupsOf(rows, groups);
	const items: TableItem[] = [];
	const emitted = new Set<string>();
	let at = 0;
	while (at < rows.length) {
		const row = rows[at];
		if (!row) break;
		if (row.kind === "separator") {
			items.push(row);
			at += 1;
			continue;
		}
		const group = byId.get(row.id);
		if (!group) {
			items.push({ kind: "mod", id: row.id, mod: row.mod });
			at += 1;
			continue;
		}
		if (emitted.has(group.id)) {
			at += 1;
			continue;
		}
		emitted.add(group.id);
		const key = group.page?.pageId ?? group.id;
		items.push(group.page ? { kind: "page", id: group.id, page: group.page } : { kind: "group", id: group.id, group });
		if (!foldedPages.has(group.id)) {
			for (const mod of group.parts) items.push({ kind: "mod", id: mod.id, mod, nested: true, groupKey: key });
		}
		at += 1;
		while (at < rows.length) {
			const part = rows[at];
			if (part && byId.get(part.id)?.id === group.id) at += 1;
			else break;
		}
	}
	return items;
}

/// The grid rows. The grouping is the `tableItems` rule, so a group with one visible part stays a plain card, a folded
/// group keeps only its header, and a filter expands a group as it does in the table. Cards are packed into rows of
/// `columns`; the cards of one group stay in rows after its header and never share a row with the mods above or below it.
export function gridItems(table: readonly TableItem[], columns: number): GridItem[] {
	const width = Math.max(1, columns);
	const items: GridItem[] = [];
	let cards: Mod[] = [];
	let cardKey: string | null = null;
	const flush = () => {
		if (cards.length === 0) return;
		items.push({ kind: "cards", mods: cards });
		cards = [];
	};
	for (const item of table) {
		if (item.kind !== "mod") {
			flush();
			items.push(item);
			continue;
		}
		// A part belongs to the header above it, keyed by `groupKey` (a page id or a user group row id); a mod outside a
		// header is only a card. A new group starts a new card row.
		const key = item.nested === true ? (item.groupKey ?? pageKeyOf(item.mod)) : null;
		if (cards.length > 0 && key !== cardKey) flush();
		if (cards.length === 0) cardKey = key;
		cards.push(item.mod);
		if (cards.length === width) flush();
	}
	flush();
	return items;
}

/// The mod ids hidden by folded headers, page or user. Only groups with at least two visible parts get a header, so only
/// those can be folded.
export function foldedPageParts(mods: readonly Mod[], foldedPages: ReadonlySet<string>, groups: readonly ModGroup[] = []): Set<string> {
	if (foldedPages.size === 0) return new Set();
	const items: ListItem[] = mods.map((mod) => ({ kind: "mod", id: mod.id, mod }));
	const byId = groupsOf(items, groups);
	const hidden = new Set<string>();
	const done = new Set<string>();
	for (const mod of mods) {
		if (done.has(mod.id)) continue;
		const group = byId.get(mod.id);
		if (!group) continue;
		for (const part of group.parts) {
			done.add(part.id);
			if (foldedPages.has(group.id)) hidden.add(part.id);
		}
	}
	return hidden;
}

/// Lists of alternatives as one line: the alternatives of a list are joined by `or`, and the lists by commas.
export function alternativesText(lists: string[][], or: string): string {
	return lists.map((list) => list.join(` ${or} `)).join(", ");
}

/// The installer choices of a mod, as one line: each option with its group.
export function choicesText(choices: Choice[]): string {
	return choices.map((choice) => `${choice.group}: ${choice.option}`).join(", ");
}

/// The list as it is shown: the mods in priority order, each separator right above the mod it names, and the
/// separators without a mod at the end.
export function listItems(mods: Mod[], separators: Separator[]): ListItem[] {
	const ids = new Set(mods.map((mod) => mod.id));
	const above = new Map<string, Separator[]>();
	const atEnd: Separator[] = [];
	for (const separator of separators) {
		const before = separator.before ?? null;
		if (before !== null && ids.has(before)) {
			above.set(before, [...(above.get(before) ?? []), separator]);
		} else {
			atEnd.push(separator);
		}
	}
	const header = (separator: Separator): ListItem => ({ kind: "separator", id: separator.id, name: separator.name });
	const items: ListItem[] = [];
	for (const mod of mods) {
		items.push(...(above.get(mod.id) ?? []).map((separator) => header(separator)), { kind: "mod", id: mod.id, mod });
	}
	items.push(...atEnd.map((separator) => header(separator)));
	return items;
}

/// A change to the list, worked out from the list as it is when the change runs: edits wait for each other, so a
/// quick second click starts from the answer to the first. `null` changes nothing.
export type ListEdit = (items: ListItem[]) => Row[] | null;

/// The list of a status, with its separators.
export function statusItems(status: Pick<Status, "mods" | "separators">): ListItem[] {
	return listItems(status.mods, status.separators);
}

/// The row revisions the window holds. A list edit sends them, so the answer carries only the rows that changed.
export function knownRows(status: Status): KnownRows {
	return { ids: status.mods.map((mod) => mod.id), revisions: status.revisions };
}

/// The status after a patch. Unchanged rows keep their old object, so memoized rows and cards skip the render; changed
/// rows replace theirs, new rows take the order the backend sent, and removed ids leave. A full status replaces all.
export function applyStatus(previous: Status | null, next: Status): Status {
	if (!next.patch) return next;
	if (!previous) return next;
	const rows = new Map<string, { mod: Mod; revision: string }>();
	previous.mods.forEach((mod, index) => {
		rows.set(mod.id, { mod, revision: previous.revisions[index] ?? "" });
	});
	next.mods.forEach((mod, index) => {
		rows.set(mod.id, { mod, revision: next.revisions[index] ?? "" });
	});
	for (const id of next.removed) rows.delete(id);
	const order = next.order ?? previous.mods.map((mod) => mod.id);
	const mods = order.flatMap((id) => {
		const row = rows.get(id);
		return row ? [row.mod] : [];
	});
	return {
		...next,
		mods,
		revisions: mods.map((mod) => rows.get(mod.id)?.revision ?? ""),
		removed: [],
		order: null,
		patch: false,
	};
}

/// Profile rows: the order is the priority, `enabled` says whether a mod goes into the game, and a separator row
/// carries its name (and its id in `folder`).
export function toRows(items: ListItem[]): Row[] {
	return items.map((item) =>
		item.kind === "mod"
			? { folder: item.mod.folder, enabled: item.mod.enabled, separator: null }
			: { folder: item.id, enabled: false, separator: item.name },
	);
}

/// The mods a list import could not find, as one line: the name, and the version when there is one.
export function missingList(missing: ReadonlyArray<{ name: string; version?: string | null }>): string {
	return missing.map((entry) => (entry.version ? `${entry.name} ${entry.version}` : entry.name)).join(", ");
}

/// How the mod list is copied: plain text, Markdown, or BBCode (the markup of Nexus Mods descriptions and forums).
export type ListFormat = "text" | "markdown" | "bbcode";

/// The fields one line of a copied list needs.
export type ListedMod = Pick<Mod, "priority" | "enabled" | "sources" | "info" | "instructions" | "requires_names" | "requirement_groups">;

/// A list row with the extras the inspector loaded. The page fields are the full summary; before the details arrive the
/// same components get the light preview with empty `tags`, so a row never blanks while it loads.
export type FullMod = Omit<Mod, "source"> &
	Pick<
		ModExtras,
		| "notes"
		| "requirements"
		| "source_dlcs"
		| "source_sync"
		| "fomod"
		| "fomod_installer"
		| "requires"
		| "incompatible"
		| "incompatible_names"
		| "overrides"
		| "overrides_names"
		| "steps_before"
		| "steps_after"
	> & {
		source: SourceSummary | null;
		/// The full description from the mod's page. The row carries only a short preview; the inspector loads the rest.
		page_description: string | null;
	};

/// The page description shown in the inspector. A source page may carry a full text while another only has a short
/// preview, so take the first full one and fall back to the preview until the details arrive.
export function pageDescriptionOf(
	sources: ReadonlyArray<{ meta?: { description?: string | null } | null }>,
	preview?: string | null,
): string | null {
	return sources.find((source) => source.meta?.description)?.meta?.description ?? preview ?? null;
}

/// The words of a copied list in the interface language: the two states, the label of what a mod needs, and `or`.
export interface ListLabels {
	enabled: string;
	disabled: string;
	requires: string;
	or: string;
}

/// Markdown reads these characters as markup, so a mod name keeps them as plain text.
function escapeMarkdown(text: string): string {
	return text.replace(/[\\`*_[\]]/g, "\\$&");
}

/// The players' instructions under a list line, each line indented so it belongs to that item. Empty when there are none.
function underItem(instructions: string, indent: string): string {
	if (instructions === "") return "";
	return `\n${instructions
		.split("\n")
		.map((line) => `${indent}${line}`)
		.join("\n")}`;
}

/// What a mod needs, on one line: its mods, then its provider requirements. Empty when it needs nothing.
function requiresLine(mod: ListedMod, labels: ListLabels): string {
	const needs = alternativesText([...mod.requires_names.map((name) => [name]), ...mod.requirement_groups], labels.or);
	return needs === "" ? "" : `${labels.requires}: ${needs}`;
}

/// The mod list as text, one mod per line in priority order. `labels` are the words in the interface language, and
/// `sourceName` names the provider of a link. A mod without a saved page URL gets no link.
export function formatModList(mods: ListedMod[], format: ListFormat, labels: ListLabels, sourceName: (provider: string) => string): string {
	const lines = mods.map((mod) => {
		const state = mod.enabled ? labels.enabled : labels.disabled;
		const version = mod.info.version ? ` ${mod.info.version}` : "";
		const primary = mod.sources.find((source) => source.page_url);
		const page = primary?.page_url;
		const label = primary ? sourceName(primary.provider) : "";
		const details = [requiresLine(mod, labels), mod.instructions.trim()].filter((line) => line !== "").join("\n");
		if (format === "markdown") {
			return `${mod.priority}. **${escapeMarkdown(mod.info.name)}**${version}${page ? ` ([${label}](${page}))` : ""} — ${state}${underItem(details, "   ")}`;
		}
		if (format === "bbcode") {
			// Brackets would open a BBCode tag, so they are left out of the name.
			return `[*][b]${mod.info.name.replace(/[[\]]/g, "")}[/b]${version}${page ? ` [url=${page}]${label}[/url]` : ""} - ${state}${underItem(details, "")}`;
		}
		return `${mod.priority}. ${mod.info.name}${version} [${state}]${underItem(details, "   ")}`;
	});
	return format === "bbcode" ? `[list=1]\n${lines.join("\n")}\n[/list]` : lines.join("\n");
}

/// `Intl` formatters are expensive to create, and the mod table formats a value per row per render,
/// so each shape is cached per locale for the whole time the app runs.
const plainNumbers = new Map<string, Intl.NumberFormat>();
const megabytes = new Map<string, Intl.NumberFormat>();
const clockTimes = new Map<string, Intl.DateTimeFormat>();
const mediumDates = new Map<string, Intl.DateTimeFormat>();
const mediumDateTimes = new Map<string, Intl.DateTimeFormat>();

function cached<T>(cache: Map<string, T>, locale: string, make: () => T): T {
	let formatter = cache.get(locale);
	if (!formatter) {
		formatter = make();
		cache.set(locale, formatter);
	}
	return formatter;
}

/// A file size: whole kilobytes below a megabyte, one decimal place above, and gigabytes from a gigabyte up.
export function formatSize(kb: number, locale: string): string {
	if (kb < 1024) return `${cached(plainNumbers, locale, () => new Intl.NumberFormat(locale)).format(kb)} KB`;
	const decimal = cached(megabytes, locale, () => new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }));
	if (kb < 1024 * 1024) return `${decimal.format(kb / 1024)} MB`;
	return `${decimal.format(kb / (1024 * 1024))} GB`;
}

const durations = new Map<string, Record<"second" | "minute" | "hour", Intl.NumberFormat>>();

function durationUnit(locale: string, unit: string): Intl.NumberFormat {
	return new Intl.NumberFormat(locale, { style: "unit", unit, unitDisplay: "short", maximumFractionDigits: 0 });
}

/// A rough time left, in the largest unit that fits: "45 s", "3 min", "2 h".
export function formatDuration(seconds: number, locale: string): string {
	const units = cached(durations, locale, () => ({
		second: durationUnit(locale, "second"),
		minute: durationUnit(locale, "minute"),
		hour: durationUnit(locale, "hour"),
	}));
	if (seconds < 60) return units.second.format(Math.max(1, Math.ceil(seconds)));
	if (seconds < 3600) return units.minute.format(Math.ceil(seconds / 60));
	return units.hour.format(Math.round(seconds / 3600));
}

/// Whether a mod has an update to take. The backend decides it (`UpdateReport::has_update`): a confirmed newer file and an
/// unordered difference both count; an ignored or current version does not.
export function hasUpdate(updates: Record<string, UpdateReport>, modId: string): boolean {
	return updates[modId]?.has_update === true;
}

/// The update state of each mod, by id: the failure of this session when its last check failed, else the saved check the
/// library sent with the status. The table, the filters and the inspector all read this one map.
export function updatesOf(mods: readonly Mod[], failures: Record<string, UpdateReport>): Record<string, UpdateReport> {
	const saved: Record<string, UpdateReport> = {};
	for (const mod of mods) {
		if (mod.update) saved[mod.id] = mod.update;
	}
	return { ...saved, ...failures };
}

/// The author shown for a mod: the one from its provider page when a check brought it, else the one from the archive.
/// The display name of a mod id; the id itself when the mod is not in the list.
export function modName(mods: readonly Mod[] | undefined, id: string): string {
	return mods?.find((mod) => mod.id === id)?.info.name ?? id;
}

export function authorOf(mod: Mod): string | null {
	return mod.source?.author ?? mod.info.author ?? null;
}

/// When the mod was last checked, in milliseconds: the stamp the library keeps from its last check, so it survives a
/// restart and a game switch.
export function checkedAtOf(mod: Mod): number | undefined {
	return mod.source?.checked_at != null ? mod.source.checked_at * 1000 : undefined;
}

/// The tags of the mods, each once, sorted in the interface language.
export function tagsOf(mods: readonly Mod[], locale: string): string[] {
	return [...new Set(mods.flatMap((mod) => mod.tags))].toSorted((a, b) => a.localeCompare(b, locale));
}

/// A mod matches when it passes the filter, the search text, and the tag filter.
export function matches(mod: Mod, filter: Filter, query: string, tagFilter: string | null, updates: Record<string, UpdateReport>): boolean {
	return matchesNeedle(mod, filter, query.trim().toLowerCase(), tagFilter, updates);
}

/// `matches` with the search text already trimmed and lowercased, so a list of thousands does not build the same
/// needle for every mod.
export function matchesNeedle(
	mod: Mod,
	filter: Filter,
	needle: string,
	tagFilter: string | null,
	updates: Record<string, UpdateReport>,
): boolean {
	const byState =
		filter === "all" ||
		filter === "hidden" ||
		(filter === "enabled" && mod.enabled) ||
		(filter === "disabled" && !mod.enabled) ||
		(filter === "missing" && mod.unmet_requirements > 0) ||
		(filter === "conflict" && mod.enabled && (mod.wins > 0 || mod.loses > 0)) ||
		(filter === "updates" && hasUpdate(updates, mod.id));
	const byText =
		needle === "" || `${mod.info.name} ${mod.folder} ${mod.page_name ?? ""} ${mod.source?.name ?? ""}`.toLowerCase().includes(needle);
	const byTag = tagFilter === null || mod.tags.includes(tagFilter);
	return byState && byText && byTag;
}

/// The mods the list hides: a mod that is off here and that the user hid, or that only another profile's collection uses.
/// `choices` holds what the user chose by hand: `true` hides a mod, `false` keeps it shown against the collection rule.
/// An enabled mod is never hidden, so nothing that reaches the game disappears from the list.
export function hiddenModIds(mods: Mod[], profiles: ProfileView[], choices: ReadonlyMap<string, boolean>): Set<string> {
	const claimed = new Set(
		profiles.filter((profile) => !profile.active).flatMap((profile) => profile.collections.flatMap((collection) => collection.families)),
	);
	const hidden = new Set<string>();
	for (const mod of mods) {
		if (mod.enabled) continue;
		const choice = choices.get(mod.id) ?? (claimed.has(mod.id) && mod.collections.length === 0);
		if (choice) hidden.add(mod.id);
	}
	return hidden;
}

/// Conflicts count only while both mods are enabled. A disabled mod wins and loses nothing.
export function activeConflicts(mod: Mod): { wins: number; loses: number } {
	return mod.enabled ? { wins: mod.wins, loses: mod.loses } : { wins: 0, loses: 0 };
}

/// Counterparts of a mod's conflicts: for each other mod, how many files this mod wins and how many it loses.
export function conflictsOf(conflicts: ModDetails["conflicts"], modId: string): Map<string, { win: number; lose: number }> {
	const map = new Map<string, { win: number; lose: number }>();
	for (const conflict of conflicts) {
		if (conflict.winner === modId) {
			for (const loser of conflict.losers) {
				const entry = map.get(loser) ?? { win: 0, lose: 0 };
				entry.win += 1;
				map.set(loser, entry);
			}
		} else if (conflict.losers.includes(modId)) {
			const entry = map.get(conflict.winner) ?? { win: 0, lose: 0 };
			entry.lose += 1;
			map.set(conflict.winner, entry);
		}
	}
	return map;
}

export const FILE_STATE = {
	win: { text: "text-win", dot: "bg-win" },
	lose: { text: "text-lose", dot: "bg-lose" },
	none: { text: "text-subtle-foreground", dot: "bg-idle" },
	disabled: { text: "text-subtle-foreground", dot: "bg-idle" },
} as const satisfies Record<FileState, { text: string; dot: string }>;

/// Local clock time, formatted in the interface language.
export function formatTime(ms: number, locale: string): string {
	return cached(clockTimes, locale, () => new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" })).format(ms);
}

const counts = new Map<string, Intl.NumberFormat>();

/// A big count, short: "12K", "4.1M" (in the interface language).
export function formatCount(value: number, locale: string): string {
	return cached(counts, locale, () => new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 })).format(value);
}

/// Calendar date, formatted in the interface language.
export function formatDate(ms: number, locale: string): string {
	return cached(mediumDates, locale, () => new Intl.DateTimeFormat(locale, { dateStyle: "medium" })).format(ms);
}

/// Calendar date and time of a notice, formatted in the interface language.
export function formatDateTime(ms: number, locale: string): string {
	return cached(mediumDateTimes, locale, () => new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" })).format(ms);
}

/// The date of a check: "today" when it happened on the current day, otherwise the date.
export function isToday(ms: number, now: number = Date.now()): boolean {
	const then = new Date(ms);
	const today = new Date(now);
	return then.getFullYear() === today.getFullYear() && then.getMonth() === today.getMonth() && then.getDate() === today.getDate();
}
