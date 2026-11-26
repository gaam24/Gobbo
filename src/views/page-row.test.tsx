import { afterAll, expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";
import type { UpdateReport } from "@/bindings";
import { applyLanguage } from "@/i18n";
import type { Group } from "@/lib/mods";
import { modFixture } from "@/test/fixtures/mod";

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
		id: "page:provider:225",
		pageId: "provider:225",
		name: "Example Page",
		author: null,
		category: null,
		mods: [
			modFixture({
				id: "provider:225",
				folder: "Base",
				page_id: "provider:225",
				page_name: "Example Page",
				part_id: "provider:225",
				part_name: "Base",
				info: { version: "1.0" },
			}),
			modFixture({
				id: "provider:225#200",
				folder: "Addon",
				page_id: "provider:225",
				page_name: "Example Page",
				part_id: "provider:225#200",
				part_name: "Addon",
				info: { version: "2.0" },
			}),
		],
	};
	const group: Group = {
		source: "page",
		id: page.id,
		name: page.name,
		parts: page.mods,
		page,
	};
	const html = render(group);
	expect(html).toContain("Example Page");
	expect(html).toContain("2 parts");
	expect(html).toContain('aria-label="Actions for Example Page"');
	expect(html).toContain("Fold Example Page");
	expect(html).toContain("Delete page…");
});

test("a user-group header shows its name and a mod count, without the page actions", () => {
	const group: Group = {
		source: "user",
		id: "group:g1",
		name: "Quality tweaks",
		parts: [
			modFixture({ id: "one", folder: "One", info: { version: "1.0" } }),
			modFixture({ id: "two", folder: "Two", info: { version: "2.0" } }),
		],
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
	expect(html).not.toContain("Let the translation override the base");
});

test("a translation group shows its language and the translation action", () => {
	const group: Group = {
		source: "user",
		id: "group:g2",
		name: "German pack",
		parts: [
			modFixture({ id: "one", folder: "One", info: { version: "1.0" } }),
			modFixture({ id: "two", folder: "Two", info: { version: "2.0" } }),
		],
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
	expect(html).toContain("Let the translation override the base");
});

afterAll(() => mock.restore());

test("a page header says the check failed when a part's check failed, not up to date", () => {
	const page = {
		id: "page:provider:225",
		pageId: "provider:225",
		name: "Example Page",
		author: null,
		category: null,
		mods: [
			modFixture({
				id: "provider:225",
				folder: "Base",
				page_id: "provider:225",
				page_name: "Example Page",
				part_id: "provider:225",
				part_name: "Base",
				info: { version: "1.0" },
			}),
			modFixture({
				id: "provider:225#200",
				folder: "Addon",
				page_id: "provider:225",
				page_name: "Example Page",
				part_id: "provider:225#200",
				part_name: "Addon",
				info: { version: "2.0" },
			}),
		],
	};
	const group: Group = {
		source: "page",
		id: page.id,
		name: page.name,
		parts: page.mods,
		page,
	};
	const failed: UpdateReport = {
		id: "provider:225",
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
	const html = render(group, versionColumns, { "provider:225": failed });
	expect(html).toContain("Check failed");
	expect(html).not.toContain("Up to date");
});
