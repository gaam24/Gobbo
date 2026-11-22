import { afterAll, expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";

import type { ModGroup, UpdateReport } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { groupRowId, type ListItem, type Mod, pageRowId } from "@/lib/mods";
import { modFixture } from "@/test/fixtures/mod";
import type { GroupMenuActions, ModListProps } from "@/views/mod-table/mod-list-props";

// A menu draws its items in a portal, which has no container without a browser. These checks read the page menu, so the
// dropdown draws its children inline. Every export is replaced: Bun keeps a module mock for later files, and a half
// mocked menu would give a real item no menu context.
const inline = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
await mock.module("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: inline,
	DropdownMenuCheckboxItem: inline,
	DropdownMenuContent: inline,
	DropdownMenuGroup: inline,
	DropdownMenuItem: inline,
	DropdownMenuLabel: inline,
	DropdownMenuPortal: inline,
	DropdownMenuRadioGroup: inline,
	DropdownMenuRadioItem: inline,
	DropdownMenuSeparator: inline,
	DropdownMenuShortcut: inline,
	DropdownMenuSub: inline,
	DropdownMenuSubContent: inline,
	DropdownMenuSubTrigger: inline,
	DropdownMenuTrigger: inline,
}));

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const { ModGrid } = await import("@/views/mod-grid/mod-grid");

const nothing = () => {
	// The list's actions are not part of this check.
};

const groupMenu: GroupMenuActions = {
	groupForMenu: () => null,
	onEditGroup: nothing,
	onUngroupMods: nothing,
	onSetModGroup: nothing,
	onSetTranslationWinner: nothing,
	groupSelection: () => [],
	onGroupSelected: nothing,
};

const alpha = modFixture({
	id: "alpha",
	folder: "Alpha",
	info: { name: "Alpha", version: null, author: null, description: null },
});
const beta = modFixture({
	id: "beta",
	folder: "Beta",
	enabled: false,
	unmet_requirements: 2,
	info: { name: "Beta", version: null, author: null, description: null },
});

const rows: ListItem[] = [
	{ kind: "separator", id: "sep-tweaks", name: "Tweaks" },
	{ kind: "mod", id: alpha.id, mod: alpha },
	{ kind: "mod", id: beta.id, mod: beta },
];

function rowsOf(mods: Mod[]): ListItem[] {
	return mods.map((mod) => ({ kind: "mod", id: mod.id, mod }));
}

const pageRows = rowsOf([
	modFixture({
		id: "nexus:225",
		folder: "Base",
		enabled: true,
		page_id: "nexus:225",
		page_name: "Example Page",
		part_id: "nexus:225",
		part_name: "Base",
		info: { author: "Example Author" },
	}),
	modFixture({
		id: "nexus:225#200",
		folder: "Addon",
		enabled: false,
		page_id: "nexus:225",
		page_name: "Example Page",
		part_id: "nexus:225#200",
		part_name: "Addon",
		info: { author: "Example Author" },
	}),
]);

function gridProps(overrides: Partial<ModListProps> = {}): ModListProps {
	return {
		gameId: "test",
		groupMenu,
		rows,
		groups: [],
		collapsed: new Set(),
		hidden: new Set(),
		foldedPages: new Set(),
		profileId: "p1",
		groupSizes: new Map([["sep-tweaks", 2]]),
		onTogglePage: nothing,
		onDeletePage: nothing,
		onSetEnabled: nothing,
		onRenameGroup: nothing,
		onUngroup: nothing,
		onSetTranslationWinner: nothing,
		selection: [],
		filtering: false,
		updates: {},
		libraryDir: "",
		archivesDir: "",
		locale: "en",
		onPick: nothing,
		onToggle: nothing,
		onMove: nothing,
		onOpenFolder: nothing,
		onRevealFile: nothing,
		onOpenSource: nothing,
		onCheckUpdate: nothing,
		onUpdateMod: nothing,
		onOpenConflicts: nothing,
		onIgnoreVersion: nothing,
		onUnignoreVersion: nothing,
		onSetHidden: nothing,
		readyProviders: new Set<string>(),
		onRemoveFromLibrary: nothing,
		onSelectVersion: nothing,
		onDeleteVersion: nothing,
		onReinstall: nothing,
		onArchiveReport: nothing,
		onChangeInstaller: nothing,
		onReorder: nothing,
		freshSeparator: null,
		firstModId: alpha.id,
		lastModId: beta.id,
		onClearFilters: nothing,
		onShowDetails: nothing,
		onAddSeparatorAbove: nothing,
		onRenameSeparator: nothing,
		onRenameDone: nothing,
		onRemoveSeparator: nothing,
		onGroupEnabled: nothing,
		onSortGroup: nothing,
		onToggleGroup: nothing,
		...overrides,
	};
}

test("the grid draws a card per mod, with its switch and a placeholder, and the separator above them", () => {
	const html = renderToString(<ModGrid {...gridProps()} />);
	expect(html).toContain('data-mod="alpha"');
	expect(html).toContain('aria-label="Enable or disable Alpha"');
	expect(html).toContain('aria-label="Enable or disable Beta"');
	expect(html).toContain(">Tweaks<");
	expect(html).toContain("col-span-full");
	// Neither mod has a provider picture, so no image is drawn.
	expect(html).not.toContain("<img");
});

test("a card shows a missing requirement on its picture", () => {
	const html = renderToString(<ModGrid {...gridProps()} />);
	expect(html).toContain(">Missing<");
});

test("a card shows a newer version it can fetch", () => {
	const update: UpdateReport = {
		id: alpha.id,
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
	const html = renderToString(<ModGrid {...gridProps({ updates: { [alpha.id]: update } })} />);
	expect(html).toContain("Available 2.0");
});

test("an empty result says so and offers to clear the filters", () => {
	const html = renderToString(<ModGrid {...gridProps({ rows: [] })} />);
	expect(html).toContain("No mods match the filter.");
});

test("two adjacent parts of one page get one compact header with its count and menu", () => {
	const html = renderToString(<ModGrid {...gridProps({ rows: pageRows })} />);
	expect(html).toContain("Example Page");
	expect(html).toContain("2 parts");
	expect(html).toContain('aria-label="Turn every part of this page on or off"');
	expect(html).toContain('aria-label="Actions for Example Page"');
	expect(html).toContain("Delete page…");
	expect(html).toContain('data-mod="nexus:225"');
	expect(html).toContain('data-mod="nexus:225#200"');
});

test("the page header tri-state follows its parts", () => {
	const mixed = renderToString(<ModGrid {...gridProps({ rows: pageRows })} />);
	expect(mixed).toContain('aria-checked="mixed"');
	const all = renderToString(
		<ModGrid
			{...gridProps({
				rows: rowsOf([
					modFixture({
						id: "nexus:225",
						folder: "Base",
						enabled: true,
						page_id: "nexus:225",
						page_name: "Example Page",
						part_id: "nexus:225",
						part_name: "Base",
						info: { author: "Example Author" },
					}),
					modFixture({
						id: "nexus:225#200",
						folder: "Addon",
						enabled: true,
						page_id: "nexus:225",
						page_name: "Example Page",
						part_id: "nexus:225#200",
						part_name: "Addon",
						info: { author: "Example Author" },
					}),
				]),
			})}
		/>,
	);
	expect(all).toContain('aria-checked="true"');
});

test("a folded page keeps its header and hides its cards", () => {
	const html = renderToString(
		<ModGrid
			{...gridProps({
				rows: pageRows,
				foldedPages: new Set([pageRowId("nexus:225")]),
			})}
		/>,
	);
	expect(html).toContain("Example Page");
	expect(html).toContain('aria-expanded="false"');
	expect(html).not.toContain('data-mod="nexus:225"');
	expect(html).not.toContain('data-mod="nexus:225#200"');
});

test("a page with one visible part stays a plain card", () => {
	const html = renderToString(
		<ModGrid
			{...gridProps({
				rows: rowsOf([
					modFixture({
						id: "nexus:225",
						folder: "Base",
						enabled: true,
						page_id: "nexus:225",
						page_name: "Example Page",
						part_id: "nexus:225",
						part_name: "Base",
						info: { author: "Example Author" },
					}),
				]),
			})}
		/>,
	);
	expect(html).not.toContain('aria-label="Turn every part of this page on or off"');
	expect(html).toContain('data-mod="nexus:225"');
});

const bundle: ModGroup[] = [
	{
		id: "g1",
		name: "Quality tweaks",
		members: [alpha.id, beta.id],
		kind: "bundle",
	},
];

test("two mods of a user group get one compact header with a mod count and their cards together", () => {
	const html = renderToString(<ModGrid {...gridProps({ rows: rowsOf([alpha, beta]), groups: bundle })} />);
	expect(html).toContain("Quality tweaks");
	expect(html).toContain("2 mods");
	expect(html).toContain('aria-label="Turn every mod in this group on or off"');
	expect(html).toContain('aria-label="Actions for Quality tweaks"');
	expect(html).toContain("Rename group");
	expect(html).toContain('data-mod="alpha"');
	expect(html).toContain('data-mod="beta"');
});

test("a folded user group keeps its compact header and hides its cards", () => {
	const html = renderToString(
		<ModGrid
			{...gridProps({
				rows: rowsOf([alpha, beta]),
				groups: bundle,
				foldedPages: new Set([groupRowId("g1")]),
			})}
		/>,
	);
	expect(html).toContain("Quality tweaks");
	expect(html).not.toContain('data-mod="alpha"');
	expect(html).not.toContain('data-mod="beta"');
});

test("a translation group shows its language in the compact header", () => {
	const groups: ModGroup[] = [
		{
			id: "g2",
			name: "German pack",
			members: [alpha.id, beta.id],
			kind: "translation",
			language: "German",
		},
	];
	const html = renderToString(<ModGrid {...gridProps({ rows: rowsOf([alpha, beta]), groups })} />);
	expect(html).toContain("German pack");
	expect(html).toContain("German");
	expect(html).toContain("Set translation as winner");
});

afterAll(() => mock.restore());
