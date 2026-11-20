import { afterAll, expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";
import type { UpdateReport } from "@/bindings";
import { applyLanguage } from "@/i18n";
import type { Group, Mod } from "@/lib/mods";

// A menu draws its items in a portal, which has no container without a browser. The header's menu is what these checks
// read, so the dropdown draws its children inline. Every export is replaced: Bun keeps a module mock for later files,
// and a half mocked menu would give a real item no menu context.
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

await applyLanguage("en");

const { COLUMNS } = await import("@/lib/mod-columns");
const { PageRow } = await import("@/views/mod-table/page-row");

const nothing = () => {
	// The header's actions are not part of this check.
};

/// One installed part of the header, with the fields the cells read.
function part(id: string, name: string, version: string): Mod {
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
		info: { name, version, author: null, description: null },
		source: null,
		sources: [],
		page_id: "nexus:225",
		page_name: "KS Page",
		part_id: id,
		part_name: name,
		part_category: null,
		tags: [],
		instructions: "",
		unmet_requirements: 0,
		profiles: [],
		collections: [],
		requires_names: [],
		requirement_groups: [],
	};
}

const nameColumns = COLUMNS.slice(0, 3);
const withCategory = [...nameColumns, ...COLUMNS.filter((column) => column.id === "category")];

function render(group: Group, columns = nameColumns, updates: Record<string, UpdateReport> = {}) {
	return renderToString(
		<PageRow
			group={group}
			columns={columns}
			folded={false}
			filtering={false}
			updates={updates}
			extras={new Map()}
			locale="en"
			onToggle={nothing}
			onFold={nothing}
			onOpenSource={nothing}
			onDeletePage={nothing}
			onRenameGroup={nothing}
			onUngroup={nothing}
			onSetTranslationWinner={nothing}
		/>,
	);
}

test("a page header still shows its name, page menu and delete action", () => {
	const page = {
		id: "page:nexus:225",
		pageId: "nexus:225",
		name: "KS Page",
		author: null,
		category: null,
		mods: [part("nexus:225", "Base", "1.0"), part("nexus:225#200", "Addon", "2.0")],
	};
	const group: Group = {
		source: "page",
		id: page.id,
		name: page.name,
		parts: page.mods,
		page,
	};
	const html = render(group);
	expect(html).toContain("KS Page");
	expect(html).toContain("2 parts");
	expect(html).toContain('aria-label="Actions for KS Page"');
	expect(html).toContain("Fold KS Page");
	expect(html).toContain("Delete page…");
});

test("a user-group header shows its name and a mod count, without the page actions", () => {
	const group: Group = {
		source: "user",
		id: "group:g1",
		name: "Quality tweaks",
		parts: [part("one", "One", "1.0"), part("two", "Two", "2.0")],
		group: {
			id: "g1",
			name: "Quality tweaks",
			members: ["one", "two"],
			kind: "bundle",
		},
	};
	const html = render(group);
	expect(html).toContain("Quality tweaks");
	expect(html).toContain("2 mods");
	expect(html).not.toContain("Delete page…");
	expect(html).toContain("Rename group");
	expect(html).toContain("Remove from group");
	expect(html).not.toContain("Set translation as winner");
});

test("a translation group shows its language and the translation action", () => {
	const group: Group = {
		source: "user",
		id: "group:g2",
		name: "German pack",
		parts: [part("one", "One", "1.0"), part("two", "Two", "2.0")],
		group: {
			id: "g2",
			name: "German pack",
			members: ["one", "two"],
			kind: "translation",
			language: "German",
		},
	};
	const html = render(group, withCategory);
	expect(html).toContain("German");
	expect(html).toContain("Set translation as winner");
});

afterAll(() => mock.restore());

test("a page header says the check failed when a part's check failed, not up to date", () => {
	const page = {
		id: "page:nexus:225",
		pageId: "nexus:225",
		name: "KS Page",
		author: null,
		category: null,
		mods: [part("nexus:225", "Base", "1.0"), part("nexus:225#200", "Addon", "2.0")],
	};
	const group: Group = {
		source: "page",
		id: page.id,
		name: page.name,
		parts: page.mods,
		page,
	};
	const failed: UpdateReport = {
		id: "nexus:225",
		provider: "nexus",
		mod_id: "225",
		current: "1.0",
		latest: null,
		latest_file_id: null,
		state: "unknown",
		latest_installed: false,
		installed_folder: null,
		has_update: false,
		error: { code: "source.check_failed", params: {}, kind: "user_action" },
	};
	const versionColumns = COLUMNS.filter((column) => column.id === "version");
	const html = render(group, versionColumns, { "nexus:225": failed });
	expect(html).toContain("Check failed");
	expect(html).not.toContain("Up to date");
});
