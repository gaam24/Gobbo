import { afterAll, expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";

import type { Status_Serialize } from "@/bindings";
import { applyLanguage } from "@/i18n";
import type { Mod } from "@/lib/mods";
import { statusFixture } from "@/test/fixtures/status";
import { type GroupMenuActions, menuGroupFor } from "@/views/mod-table/mod-list-props";

// The Radix menu only mounts its content after the first open, which a server render cannot do. Render the items with
// inline primitives instead, so the group section and its operations are visible to the check.
const inline = ({ children }: { children?: ReactNode }) => <div>{children}</div>;
const checkbox = ({ children, checked }: { children?: ReactNode; checked?: boolean }) => (
	<div role="menuitemcheckbox" aria-checked={checked} tabIndex={0}>
		{children}
	</div>
);
await mock.module("@/components/ui/context-menu", () => ({
	ContextMenu: inline,
	ContextMenuCheckboxItem: checkbox,
	ContextMenuContent: inline,
	ContextMenuItem: inline,
	ContextMenuLabel: inline,
	ContextMenuSeparator: inline,
	ContextMenuTrigger: inline,
}));
await applyLanguage("en");

const { ModMenuContent } = await import("@/views/mod-table/mod-menu");

const nothing = () => {
	// The menu's actions are not part of a structure check.
};

/// One installed mod, with the fields the menu reads. The fixture takes the serialized status shape.
function mod(id: string, name: string): Status_Serialize["mods"][number] {
	return {
		id,
		folder: name,
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
		page_id: null,
		page_name: null,
		part_id: null,
		part_name: null,
		part_category: null,
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
const beta = mod("beta", "Beta");

const bundle: Status_Serialize["groups"] = [{ id: "grp1", name: "Quality tweaks", members: [alpha.id, beta.id], kind: "bundle" }];
const translation: Status_Serialize["groups"] = [
	{ id: "grp2", name: "German pack", members: [alpha.id, beta.id], kind: "translation", language: "German" },
];

function menuFor(target: Mod, groups: Status_Serialize["groups"], selection: string[] = []): string {
	const status = statusFixture({ kind: "ready", enabled: 0, total: 0 }, { mods: [alpha, beta], groups });
	const groupMenu: GroupMenuActions = {
		groupForMenu: (id) => menuGroupFor(id, status.groups, selection),
		onEditGroup: nothing,
		onUngroupMods: nothing,
		onSetModGroup: nothing,
		onSetTranslationWinner: nothing,
		groupSelection: () => selection,
		onGroupSelected: nothing,
	};
	return renderToString(
		<ModMenuContent
			mod={target}
			filtering={false}
			hidden={false}
			isFirst
			isLast
			libraryDir={status.library_dir}
			archivesDir={status.archives_dir}
			groupMenu={groupMenu}
			readyProviders={new Set<string>()}
			onShowDetails={nothing}
			onToggle={nothing}
			onMove={nothing}
			onAddSeparatorAbove={nothing}
			onOpenFolder={nothing}
			onRevealFile={nothing}
			onReinstall={nothing}
			onArchiveReport={nothing}
			onChangeInstaller={nothing}
			onOpenSource={nothing}
			onCheckUpdate={nothing}
			onUpdateMod={nothing}
			onIgnoreVersion={nothing}
			onUnignoreVersion={nothing}
			onSetHidden={nothing}
			onRemoveFromLibrary={nothing}
			onDeleteVersion={nothing}
			deferClose={nothing}
		/>,
	);
}

test("a grouped mod's menu opens with a group section above the single-mod actions", () => {
	const html = menuFor(alpha, bundle);
	expect(html).toContain("Groups");
	expect(html).toContain("Quality tweaks");
	expect(html).toContain("Rename group");
	expect(html).toContain("Remove from group");
	expect(html).toContain('role="menuitemcheckbox" aria-checked="true"');
	// The single-mod sections are still there, just below the group section.
	expect(html).toContain("Show details");
	expect(html).toContain(">Order<");
	expect(html).toContain(">Files<");
	expect(html).toContain(">Delete<");
	expect(html.indexOf("Rename group")).toBeLessThan(html.indexOf("Show details"));
});

test("a translation group's menu offers the translation winner action", () => {
	const html = menuFor(beta, translation);
	expect(html).toContain("German pack");
	expect(html).toContain("Set translation as winner");
});

test("a mod outside a group keeps the single-mod menu", () => {
	const html = menuFor(alpha, []);
	expect(html).not.toContain("Rename group");
	expect(html).not.toContain("Remove from group");
	expect(html).not.toContain("Set translation as winner");
	expect(html).not.toContain("menuitemcheckbox");
	expect(html).toContain("Show details");
});

test("a right click on one of several selected mods describes that mod's own group", () => {
	const separate: Status_Serialize["groups"] = [
		{ id: "g1", name: "First pair", members: [alpha.id, "gamma"], kind: "bundle" },
		{ id: "g2", name: "Second pair", members: [beta.id, "delta"], kind: "bundle" },
	];
	const html = menuFor(alpha, separate, [alpha.id, beta.id]);
	expect(html).toContain("First pair");
	expect(html).toContain("Rename group");
	expect(html).not.toContain("Second pair");
	expect(html.indexOf("Rename group")).toBeLessThan(html.indexOf("Show details"));
});

test("a right click on a selected mod with no group offers to group the selection", () => {
	const ungrouped = statusFixture({ kind: "ready", enabled: 0, total: 0 }, { mods: [alpha, beta] });
	const html = renderToString(
		<ModMenuContent
			mod={alpha}
			filtering={false}
			hidden={false}
			isFirst
			isLast
			libraryDir={ungrouped.library_dir}
			archivesDir={ungrouped.archives_dir}
			groupMenu={{
				groupForMenu: () => null,
				groupSelection: () => [alpha.id, beta.id],
				onGroupSelected: nothing,
				onEditGroup: nothing,
				onUngroupMods: nothing,
				onSetModGroup: nothing,
				onSetTranslationWinner: nothing,
			}}
			readyProviders={new Set<string>()}
			onShowDetails={nothing}
			onToggle={nothing}
			onMove={nothing}
			onAddSeparatorAbove={nothing}
			onOpenFolder={nothing}
			onRevealFile={nothing}
			onReinstall={nothing}
			onArchiveReport={nothing}
			onChangeInstaller={nothing}
			onOpenSource={nothing}
			onCheckUpdate={nothing}
			onUpdateMod={nothing}
			onIgnoreVersion={nothing}
			onUnignoreVersion={nothing}
			onSetHidden={nothing}
			onRemoveFromLibrary={nothing}
			onDeleteVersion={nothing}
			deferClose={nothing}
		/>,
	);
	expect(html).toContain("Group selected mods");
	expect(html.indexOf("Group selected mods")).toBeLessThan(html.indexOf("Show details"));
});

afterAll(() => mock.restore());
