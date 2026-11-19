import { expect, test } from "bun:test";

import type { ModGroup, ProfileView, Status, UpdateReport } from "@/bindings";
import {
	activeConflicts,
	applyStatus,
	foldedPageParts,
	formatDuration,
	formatModList,
	formatSize,
	gridItems,
	groupMemberIds,
	groupRowId,
	hasUpdate,
	hiddenModIds,
	knownRows,
	type ListedMod,
	listItems,
	type Mod,
	matches,
	missingList,
	pageDescriptionOf,
	pagePartsOf,
	pageRowId,
	tableItems,
	toRows,
	updatesOf,
	userGroups,
} from "@/lib/mods";
import { sourceLists } from "@/sources/requirements";

const labels = { enabled: "enabled", disabled: "disabled", requires: "Requires", or: "or" };
const sourceName = (provider: string) => (provider === "nexus" ? "Nexus" : provider);

const mods: ListedMod[] = [
	{
		priority: 1,
		enabled: true,
		sources: [{ provider: "nexus", mod_id: "4311", page_url: "https://www.nexusmods.com/examplegame/mods/4311", file_id: null }],
		info: { name: "Cool Mod", version: "1.2", author: null, description: null },
		instructions: "",
		requires_names: [],
		requirement_groups: [],
	},
	{
		priority: 2,
		enabled: false,
		sources: [],
		info: { name: "Plain_Mod [v2]", version: null, author: null, description: null },
		instructions: "",
		requires_names: [],
		requirement_groups: [],
	},
];

const sourceRequirement = (mod_id: number, group: number | null) => ({
	source: { provider: "nexus", mod_id: String(mod_id), file_id: null },
	name: `Mod ${mod_id}`,
	from_source: true,
	group,
	ignored: false,
	state: "missing" as const,
});

const part = (id: string, partId: string, name: string): Mod =>
	listed({
		id,
		folder: name,
		page_id: "nexus:225",
		page_name: "AudioXL",
		part_id: partId,
		part_name: name,
		info: { name, version: "1.0", author: "DigitalVixen", description: null },
	});

/// A plain mod for the user-group tests, with a stable id and a readable folder and name.
const member = (id: string): Mod => listed({ id, folder: id, info: { name: id, version: null, author: null, description: null } });

/// A stored user group with generic ids and the bundle kind.
const userGroup = (id: string, members: string[]): ModGroup => ({ id, name: `Group ${id}`, members, kind: "bundle" });

test("adjacent parts of one page become one page header with the parts under it", () => {
	const items = listItems(
		[
			part("nexus:225", "100", "Base"),
			part("nexus:225#200", "200", "Addon"),
			listed({ id: "local:other", folder: "Other", page_id: null }),
		],
		[],
	);
	const table = tableItems(items);
	expect(table.map((row) => row.kind)).toEqual(["page", "mod", "mod", "mod"]);
	expect(table[0]).toMatchObject({ kind: "page", id: pageRowId("nexus:225"), page: { name: "AudioXL", author: "DigitalVixen" } });
	if (table[0]?.kind === "page") expect(table[0].page.mods.map((mod) => mod.info.name)).toEqual(["Base", "Addon"]);
	expect(foldedPageParts([part("a", "100", "Base"), part("b", "200", "Addon")], new Set([pageRowId("nexus:225")]))).toEqual(
		new Set(["a", "b"]),
	);
	expect(tableItems(items, new Set([pageRowId("nexus:225")])).map((row) => row.kind)).toEqual(["page", "mod"]);
});

test("the grid keeps a page's cards under one header, and folding hides only the cards", () => {
	const items = listItems(
		[
			part("nexus:225", "100", "Base"),
			part("nexus:225#200", "200", "Addon"),
			listed({ id: "local:other", folder: "Other", page_id: null }),
		],
		[],
	);
	const grid = gridItems(tableItems(items), 2);
	expect(grid.map((unit) => unit.kind)).toEqual(["page", "cards", "cards"]);
	expect(grid[1]).toMatchObject({ kind: "cards", mods: [{ id: "nexus:225" }, { id: "nexus:225#200" }] });
	expect(grid[2]).toMatchObject({ kind: "cards", mods: [{ id: "local:other" }] });

	const folded = gridItems(tableItems(items, new Set([pageRowId("nexus:225")])), 2);
	expect(folded.map((unit) => unit.kind)).toEqual(["page", "cards"]);
	expect(folded[1]).toMatchObject({ kind: "cards", mods: [{ id: "local:other" }] });
});

test("a page with one visible part is a plain card in the grid", () => {
	const items = listItems([part("nexus:225", "100", "Base"), listed({ id: "local:other", folder: "Other", page_id: null })], []);
	const grid = gridItems(tableItems(items), 2);
	expect(grid.map((unit) => unit.kind)).toEqual(["cards"]);
	expect(grid[0]).toMatchObject({ kind: "cards", mods: [{ id: "nexus:225" }, { id: "local:other" }] });
});

test("the grid packs a long page into rows and keeps its cards after the header", () => {
	const items = listItems(
		[part("a", "100", "A"), part("b", "200", "B"), part("c", "300", "C"), part("d", "400", "D"), part("e", "500", "E")],
		[],
	);
	const grid = gridItems(tableItems(items), 2);
	expect(grid.map((unit) => (unit.kind === "cards" ? unit.mods.length : unit.kind))).toEqual(["page", 2, 2, 1]);
});

test("a page's installed parts are found by page id, in list order", () => {
	const pageMods = [
		part("nexus:225", "100", "Base"),
		listed({ id: "local:other", folder: "Other" }),
		part("nexus:225#200", "200", "Addon"),
	];
	expect(pagePartsOf(pageMods, "nexus:225").map((mod) => mod.id)).toEqual(["nexus:225", "nexus:225#200"]);
	expect(pagePartsOf(pageMods, "nexus:999")).toEqual([]);
});

test("two user groups each get one header with their parts under it", () => {
	const groups = [userGroup("g1", ["mod-a", "mod-b"]), userGroup("g2", ["mod-c", "mod-d"])];
	const shown = [member("mod-a"), member("mod-b"), member("mod-c"), member("mod-d"), member("solo")];
	const table = tableItems(listItems(shown, []), new Set(), groups);
	expect(table.map((row) => row.kind)).toEqual(["group", "mod", "mod", "group", "mod", "mod", "mod"]);
	expect(table[0]).toMatchObject({ kind: "group", id: groupRowId("g1"), group: { name: "Group g1" } });
	if (table[0]?.kind === "group") expect(table[0].group.parts.map((p) => p.id)).toEqual(["mod-a", "mod-b"]);
	const grid = gridItems(table, 2);
	expect(grid.map((unit) => unit.kind)).toEqual(["group", "cards", "group", "cards", "cards"]);
	expect(grid[1]).toMatchObject({ kind: "cards", mods: [{ id: "mod-a" }, { id: "mod-b" }] });
});

test("a user group wins over a page header over the same rows", () => {
	const groups = [userGroup("g1", ["mod-a", "mod-b"])];
	const pageMods = ["mod-a", "mod-b", "mod-c", "mod-d"].map((id) => listed({ id, folder: id, page_id: "nexus:9", page_name: "Shared" }));
	const table = tableItems(listItems(pageMods, []), new Set(), groups);
	expect(table.map((row) => row.kind)).toEqual(["group", "mod", "mod", "page", "mod", "mod"]);
	expect(table[0]?.id).toBe(groupRowId("g1"));
	expect(table[3]?.id).toBe(pageRowId("nexus:9"));
});

test("folding a user group hides its parts but keeps the header", () => {
	const groups = [userGroup("g1", ["mod-a", "mod-b"])];
	const shown = [member("mod-a"), member("mod-b"), member("solo")];
	const folded = new Set([groupRowId("g1")]);
	expect(tableItems(listItems(shown, []), folded, groups).map((row) => row.kind)).toEqual(["group", "mod"]);
	expect(foldedPageParts(shown, folded, groups)).toEqual(new Set(["mod-a", "mod-b"]));
});

test("a user group header resolves to every member family for a header toggle", () => {
	const groups = [userGroup("g1", ["mod-a", "mod-b"])];
	const shown = ["mod-a", "mod-b", "solo"].map((id) => ({ ...member(id), enabled: false }));
	const items = listItems(shown, []);
	expect(groupMemberIds(items, groupRowId("g1"), groups)).toEqual(["mod-a", "mod-b"]);
});

test("a user group with one visible member stays a plain row", () => {
	const groups = [userGroup("g1", ["mod-a", "mod-b"])];
	const shown = [member("mod-a"), member("solo")];
	expect(userGroups(shown, groups)).toEqual([]);
	expect(tableItems(listItems(shown, []), new Set(), groups).map((row) => row.kind)).toEqual(["mod", "mod"]);
});

test("a removed member is ignored and parts follow the list order, not the member order", () => {
	const reversed = userGroup("g1", ["mod-b", "mod-a", "gone"]);
	const shown = [member("mod-a"), member("mod-b")];
	const result = userGroups(shown, [reversed]);
	expect(result).toHaveLength(1);
	expect(result[0]?.parts.map((p) => p.id)).toEqual(["mod-a", "mod-b"]);
	expect(userGroups([member("mod-a")], [reversed])).toEqual([]);
});

test("the requirements of a group share a list, and the others stand alone", () => {
	const lists = sourceLists([sourceRequirement(1, 0), sourceRequirement(2, null), sourceRequirement(3, 0), sourceRequirement(4, null)]);
	expect(lists.map((alternatives) => alternatives.map((item) => item.source.mod_id))).toEqual([["1", "3"], ["2"], ["4"]]);
});

test("instructions sit under their mod in every format", () => {
	const withText: ListedMod[] = [
		{
			priority: 1,
			enabled: true,
			sources: [{ provider: "nexus", mod_id: "4311", page_url: "https://www.nexusmods.com/examplegame/mods/4311", file_id: null }],
			info: { name: "Cool Mod", version: "1.2", author: null, description: null },
			instructions: "Copy into Mods.\nPick the 4K option.",
			requires_names: [],
			requirement_groups: [],
		},
	];
	expect(formatModList(withText, "text", labels, sourceName)).toBe("1. Cool Mod 1.2 [enabled]\n   Copy into Mods.\n   Pick the 4K option.");
	expect(formatModList(withText, "markdown", labels, sourceName)).toBe(
		"1. **Cool Mod** 1.2 ([Nexus](https://www.nexusmods.com/examplegame/mods/4311)) — enabled\n   Copy into Mods.\n   Pick the 4K option.",
	);
	expect(formatModList(withText, "bbcode", labels, sourceName)).toBe(
		"[list=1]\n[*][b]Cool Mod[/b] 1.2 [url=https://www.nexusmods.com/examplegame/mods/4311]Nexus[/url] - enabled\nCopy into Mods.\nPick the 4K option.\n[/list]",
	);
});

test("plain text keeps the numbered list with the state in brackets", () => {
	expect(formatModList(mods, "text", labels, sourceName)).toBe("1. Cool Mod 1.2 [enabled]\n2. Plain_Mod [v2] [disabled]");
});

test("Markdown bolds the name, links the Nexus page and escapes markup in the name", () => {
	expect(formatModList(mods, "markdown", labels, sourceName)).toBe(
		"1. **Cool Mod** 1.2 ([Nexus](https://www.nexusmods.com/examplegame/mods/4311)) — enabled\n2. **Plain\\_Mod \\[v2\\]** — disabled",
	);
});

test("BBCode wraps the list and links the Nexus page, without brackets in the name", () => {
	expect(formatModList(mods, "bbcode", labels, sourceName)).toBe(
		"[list=1]\n" +
			"[*][b]Cool Mod[/b] 1.2 [url=https://www.nexusmods.com/examplegame/mods/4311]Nexus[/url] - enabled\n" +
			"[*][b]Plain_Mod v2[/b] - disabled\n" +
			"[/list]",
	);
});

test("the link label comes from the name function, not a fixed provider name", () => {
	expect(formatModList(mods.slice(0, 1), "bbcode", labels, (provider) => `Provider ${provider}`)).toContain(
		"[url=https://www.nexusmods.com/examplegame/mods/4311]Provider nexus[/url]",
	);
});

test("what a mod needs is one line under it, with the Nexus alternatives joined by or", () => {
	const needy: ListedMod[] = [
		{
			priority: 1,
			enabled: true,
			sources: [],
			info: { name: "Cool Mod", version: null, author: null, description: null },
			instructions: "",
			requires_names: ["Base"],
			requirement_groups: [["Mod 1", "Mod 2"], ["Mod 3"]],
		},
	];
	expect(formatModList(needy, "text", labels, sourceName)).toBe("1. Cool Mod [enabled]\n   Requires: Base, Mod 1 or Mod 2, Mod 3");
	expect(formatModList(needy, "markdown", labels, sourceName)).toBe("1. **Cool Mod** — enabled\n   Requires: Base, Mod 1 or Mod 2, Mod 3");
	expect(formatModList(needy, "bbcode", labels, sourceName)).toBe(
		"[list=1]\n[*][b]Cool Mod[/b] - enabled\nRequires: Base, Mod 1 or Mod 2, Mod 3\n[/list]",
	);
});

test("a source without a saved page URL gets no link, because no URL is built from its id", () => {
	const unlinked: ListedMod[] = [
		{
			priority: 1,
			enabled: false,
			sources: [{ provider: "nexus", mod_id: "4311", page_url: null, file_id: null }],
			info: { name: "Plain_Mod [v2]", version: null, author: null, description: null },
			instructions: "",
			requires_names: [],
			requirement_groups: [],
		},
	];
	expect(formatModList(unlinked, "markdown", labels, sourceName)).toBe("1. **Plain\\_Mod \\[v2\\]** — disabled");
	expect(formatModList(unlinked, "bbcode", labels, sourceName)).toBe("[list=1]\n[*][b]Plain_Mod v2[/b] - disabled\n[/list]");
});

/// A mod with the fields the filters read, and safe defaults for the rest.
function listed(overrides: Partial<Mod> = {}): Mod {
	return {
		id: "local:cool-mod",
		folder: "Cool Mod",
		fomod_installer: false,
		version_count: 1,
		installed_at: 0,
		slot: "Mods",
		update: null,
		enabled: true,
		priority: 1,
		load_priority: null,
		files: 2,
		size_kb: 10,
		wins: 0,
		loses: 0,
		archive: null,
		archive_file: null,
		skipped: 0,
		info: { name: "Cool Mod", version: "1.0", author: null, description: null },
		source: null,
		sources: [],
		page_id: null,
		page_name: null,
		part_id: null,
		part_name: null,
		part_category: null,
		tags: [],
		instructions: "",
		unmet_requirements: 0,
		profiles: [],
		collections: [],
		requires_names: [],
		requirement_groups: [],
		...overrides,
	};
}

const update: UpdateReport = {
	id: "local:cool-mod",
	provider: "nexus",
	mod_id: "1",
	current: "1.0",
	latest: "2.0",
	latest_file_id: null,
	state: "newer",
	latest_installed: false,
	installed_folder: null,
	has_update: true,
	error: null,
};

test("a mod counts as updatable when the backend says it has an update", () => {
	expect(hasUpdate({ "local:cool-mod": update }, "local:cool-mod")).toBe(true);
	const different = { ...update, state: "different" as const, has_update: true };
	expect(hasUpdate({ "local:cool-mod": different }, "local:cool-mod")).toBe(true);
	const ignored = { ...update, state: "ignored" as const, has_update: false };
	expect(hasUpdate({ "local:cool-mod": ignored }, "local:cool-mod")).toBe(false);
	expect(hasUpdate({}, "local:cool-mod")).toBe(false);
});

test("matches filters by state, text and tag", () => {
	const mod = listed({ wins: 1, tags: ["graphics"] });
	expect(matches(mod, "all", "", null, {})).toBe(true);
	expect(matches(mod, "enabled", "", null, {})).toBe(true);
	expect(matches(mod, "disabled", "", null, {})).toBe(false);
	expect(matches(mod, "missing", "", null, {})).toBe(false);
	expect(matches(listed({ unmet_requirements: 2 }), "missing", "", null, {})).toBe(true);
	expect(matches(mod, "conflict", "", null, {})).toBe(true);
	expect(matches(mod, "updates", "", null, { [mod.id]: update })).toBe(true);
	expect(matches(mod, "updates", "", null, {})).toBe(false);
	expect(matches(mod, "all", "cool", null, {})).toBe(true);
	expect(matches(mod, "all", "mod", null, {})).toBe(true);
	expect(matches(mod, "all", "other", null, {})).toBe(false);
	expect(matches(mod, "all", "", "graphics", {})).toBe(true);
	expect(matches(mod, "all", "", "audio", {})).toBe(false);
});

test("a disabled mod wins and loses nothing", () => {
	expect(activeConflicts(listed({ enabled: true, wins: 3, loses: 2 }))).toEqual({ wins: 3, loses: 2 });
	expect(activeConflicts(listed({ enabled: false, wins: 3, loses: 2 }))).toEqual({ wins: 0, loses: 0 });
});

/// A status with the fields the deployment blocker reads, and safe defaults for the rest.
function status(overrides: Partial<Status> = {}): Status {
	return {
		game_name: "Game",
		game_subtitle: "",
		stage: { kind: "ready", enabled: 1, total: 1 },
		next_action: "apply",
		settings: {
			game_root: "/games/Game",
			settings_dir: "/games/Game",
			mods_root: "",
			proton_dir: "",
			launch_env: "",
			launch_args: "",
			profile_data: [],
			proton_options: [],
		},
		game_ready: true,
		settings_ready: true,
		needs_mods_root: false,
		mods_ready: true,
		mods_root: "/games/Game",
		needs_settings_dir: false,
		deployed: false,
		draft_changes: 0,
		deployed_at: null,
		deployed_list: [],
		profiles: [],
		mods: [listed()],
		revisions: [],
		build: 0,
		patch: false,
		removed: [],
		order: null,
		separators: [],
		groups: [],
		history: { can_undo: false, can_redo: false },
		bisect: null,
		has_overwrite: false,
		conflict_count: 0,
		problems: [],
		warnings: [],
		library_dir: "/data/library",
		archives_dir: "/data/archives",
		profile_dir: null,
		overwrite_dir: "/data/state/p1/overwrite",
		game_profile_data: [],
		user_data: [],
		launch_hint: "",
		executables: [],
		tools: [],
		...overrides,
	};
}

test("a patch keeps unchanged rows, replaces changed ones and follows the sent order", () => {
	const a = listed({ id: "a", folder: "fa" });
	const b = listed({ id: "b", folder: "fb" });
	const before = status({ mods: [a, b], revisions: ["1", "2"], build: 1 });
	const changed = { ...b, enabled: false };
	const patch = status({ mods: [changed], revisions: ["3"], build: 2, patch: true });
	const merged = applyStatus(before, patch);
	expect(merged.mods[0]).toBe(a);
	expect(merged.mods[1]).toEqual(changed);
	expect(merged.revisions).toEqual(["1", "3"]);
	expect(merged.build).toBe(2);
	expect(merged.patch).toBe(false);
	expect(knownRows(merged)).toEqual({ ids: ["a", "b"], revisions: ["1", "3"] });

	const c = listed({ id: "c", folder: "fc" });
	const reordered = status({ mods: [c], revisions: ["4"], build: 3, patch: true, order: ["c", "a", "b"] });
	const withNew = applyStatus(merged, reordered);
	expect(withNew.mods.map((mod) => mod.id)).toEqual(["c", "a", "b"]);
	expect(withNew.revisions).toEqual(["4", "1", "3"]);

	const removed = status({ mods: [], revisions: [], build: 4, patch: true, removed: ["a"], order: ["b"] });
	expect(applyStatus(withNew, removed).mods.map((mod) => mod.id)).toEqual(["b"]);
});

/// Three mods A, B, C in list order; only A is enabled.
function list(): Mod[] {
	return ["A", "B", "C"].map((name, index) =>
		listed({ id: name, folder: `f${name}`, enabled: index === 0, info: { name, version: null, author: null, description: null } }),
	);
}

test("rows keep the folder, enabled flag and separator of each item", () => {
	expect(
		toRows(
			listItems(list(), [
				{ id: "s1", name: "Graphics", before: "B" },
				{ id: "s2", name: "End", before: null },
			]),
		),
	).toEqual([
		{ folder: "fA", enabled: true, separator: null },
		{ folder: "s1", enabled: false, separator: "Graphics" },
		{ folder: "fB", enabled: false, separator: null },
		{ folder: "fC", enabled: false, separator: null },
		{ folder: "s2", enabled: false, separator: "End" },
	]);
});

test("missing mods are listed with their version when known", () => {
	expect(
		missingList([
			{ name: "A", version: "1.0" },
			{ name: "B", version: null },
		]),
	).toBe("A 1.0, B");
	expect(missingList([])).toBe("");
});

/// A, B and C with a separator above B and one at the end.
const grouped = () =>
	listItems(list(), [
		{ id: "s1", name: "Graphics", before: "B" },
		{ id: "s2", name: "End", before: null },
	]);

test("separators sit above their mod, and the ones without a mod at the end", () => {
	expect(grouped().map((item) => item.id)).toEqual(["A", "s1", "B", "C", "s2"]);
	// A separator naming a mod that is gone goes to the end.
	expect(listItems(list(), [{ id: "s1", name: "Lost", before: "gone" }]).map((item) => item.id)).toEqual(["A", "B", "C", "s1"]);
});

test("sizes and times left read in the largest unit that fits", () => {
	expect(formatSize(512, "en")).toBe("512 KB");
	expect(formatSize(1536, "en")).toBe("1.5 MB");
	expect(formatSize(3 * 1024 * 1024, "en")).toBe("3.0 GB");
	expect(formatDuration(0.2, "en")).toBe("1 sec");
	expect(formatDuration(45, "en")).toBe("45 sec");
	expect(formatDuration(150, "en")).toBe("3 min");
	expect(formatDuration(7200, "en")).toBe("2 hr");
});

test("the update state is the saved check of each mod, and a failure of this session shows instead", () => {
	const checked = listed({ id: "local:cool-mod", update });
	const unchecked = listed({ id: "local:plain", update: null });
	const error = { code: "nexus.down", params: {}, kind: "retryable" as const };
	expect(updatesOf([checked, unchecked], {})).toEqual({ [checked.id]: update });
	// A failed check shows over the saved result, and a mod with no saved check shows its failure alone.
	const failure = { ...update, id: unchecked.id, latest: null, state: "unknown" as const, has_update: false, error };
	const failed = updatesOf([checked, unchecked], { [checked.id]: { ...update, error }, [unchecked.id]: failure });
	expect(failed[checked.id]?.error?.code).toBe("nexus.down");
	expect(failed[unchecked.id]).toEqual(failure);
});

/// A profile with the collections it installed, each owning the given mod families.
function profileWith(families: string[], active: boolean): ProfileView {
	return {
		id: active ? "active" : "other",
		name: "Profile",
		mod_count: families.length,
		enabled_count: 0,
		active,
		deployed: false,
		has_user_data: false,
		collections:
			families.length > 0
				? [
						{
							provider: "nexus",
							game_key: "example",
							slug: "pack",
							revision: 1,
							revision_label: "1",
							name: "Pack",
							url: "",
							families,
							update: false,
						},
					]
				: [],
	};
}

test("a disabled mod is hidden when the user hid it or another profile's collection uses it alone", () => {
	const off = listed({ id: "local:off", enabled: false });
	const on = listed({ id: "local:on", enabled: true });
	const shared = listed({ id: "local:shared", enabled: false, collections: ["Pack"] });
	const other = [profileWith(["local:off", "local:on", "local:shared"], false)];
	// An enabled mod is never hidden, even when the user hid it or another collection claims it.
	expect(hiddenModIds([on], other, new Map([[on.id, true]]))).toEqual(new Set());
	// A disabled mod the user hid is hidden, whatever the profiles say.
	expect(hiddenModIds([off], [], new Map([[off.id, true]]))).toEqual(new Set([off.id]));
	// A disabled mod that only another profile's collection uses is hidden.
	expect(hiddenModIds([off], other, new Map())).toEqual(new Set([off.id]));
	// A disabled mod that this profile's collection also uses stays visible.
	expect(hiddenModIds([shared], other, new Map())).toEqual(new Set());
	// A mod claimed only by the active profile's collection stays visible.
	expect(hiddenModIds([off], [profileWith([off.id], true)], new Map())).toEqual(new Set());
	// Showing a mod by hand wins over the collection rule.
	expect(hiddenModIds([off], other, new Map([[off.id, false]]))).toEqual(new Set());
});

test("the inspector shows the full page description, not the short preview", () => {
	expect(pageDescriptionOf([{ meta: { description: "Full page text" } }], "Short preview")).toBe("Full page text");
	// A provider with only a summary (Thunderstore) leaves the preview in place until the details arrive.
	expect(pageDescriptionOf([{ meta: { description: null } }], "Short preview")).toBe("Short preview");
	expect(pageDescriptionOf([], "Short preview")).toBe("Short preview");
	expect(pageDescriptionOf([], null)).toBeNull();
});
