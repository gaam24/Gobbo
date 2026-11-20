import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { ModGroup } from "@/bindings";
import { applyLanguage } from "@/i18n";
import type { ListItem, Mod } from "@/lib/mods";
import { groupRowId } from "@/lib/mods";
import type { GroupMenuActions, ModListProps } from "@/views/mod-table/mod-list-props";
import { ModTable } from "@/views/mod-table/mod-table";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

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

function tableProps(overrides: Partial<ModListProps> = {}): ModListProps {
	return {
		gameId: "test",
		groupMenu,
		rows: [],
		groups: [],
		collapsed: new Set(),
		hidden: new Set(),
		foldedPages: new Set(),
		profileId: "p1",
		groupSizes: new Map(),
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
		firstModId: null,
		lastModId: null,
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

test("an empty result says so", () => {
	const html = renderToString(<ModTable {...tableProps()} />);
	expect(html).toContain("No mods match the filter.");
});

/// One installed mod, with the fields the row and the group header read.
function mod(id: string, name: string, enabled = true): Mod {
	return {
		id,
		folder: name,
		fomod_installer: false,
		version_count: 1,
		installed_at: 0,
		slot: "Mods",
		update: null,
		enabled,
		priority: 1,
		load_priority: null,
		files: 2,
		size_kb: 10,
		wins: 0,
		loses: 0,
		archive: null,
		archive_file: null,
		skipped: 0,
		info: { name, version: "1.0", author: null, description: null },
		source: null,
		sources: [],
		tags: [],
		instructions: "",
		unmet_requirements: 0,
		profiles: [],
		collections: [],
		requires_names: [],
		requirement_groups: [],
	};
}

const alpha = mod("alpha", "Alpha");
const beta = mod("beta", "Beta", false);
const groupRows: ListItem[] = [
	{ kind: "mod", id: alpha.id, mod: alpha },
	{ kind: "mod", id: beta.id, mod: beta },
];
const bundle: ModGroup[] = [{ id: "g1", name: "Quality tweaks", members: ["alpha", "beta"], kind: "bundle" }];

test("a user group gets one full-width header with its name and mod count", () => {
	const html = renderToString(<ModTable {...tableProps({ rows: groupRows, groups: bundle })} />);
	expect(html).toContain("Quality tweaks");
	expect(html).toContain("2 mods");
	expect(html).toContain('aria-label="Turn every mod in this group on or off"');
	expect(html).toContain('aria-label="Actions for Quality tweaks"');
	// The parts stay real rows below the header.
	expect(html).toContain('data-mod="alpha"');
	expect(html).toContain('data-mod="beta"');
});

test("a folded user group keeps its header and hides both parts", () => {
	const html = renderToString(<ModTable {...tableProps({ rows: groupRows, groups: bundle, foldedPages: new Set([groupRowId("g1")]) })} />);
	expect(html).toContain("Quality tweaks");
	expect(html).not.toContain('data-mod="alpha"');
	expect(html).not.toContain('data-mod="beta"');
});

test("a translation group shows its language in the header", () => {
	const groups: ModGroup[] = [{ id: "g2", name: "German pack", members: ["alpha", "beta"], kind: "translation", language: "German" }];
	const html = renderToString(<ModTable {...tableProps({ rows: groupRows, groups })} />);
	expect(html).toContain("German pack");
	expect(html).toContain("German");
	expect(html).toContain('aria-label="Actions for German pack"');
});
