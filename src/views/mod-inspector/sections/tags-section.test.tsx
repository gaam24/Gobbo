import { expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";

// The Radix menu mounts its content only after the first open, which a server render cannot do. Inline primitives keep
// the entries in the markup, so the check sees what the list offers.
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

// The labels come from the translations, so the test starts a plain English instance.
const { applyLanguage } = await import("@/i18n");
await applyLanguage("en");

const { TagsSection } = await import("@/views/mod-inspector/sections/tags-section");
const { modFixture } = await import("@/test/fixtures/mod");

const noop = () => undefined;

test("the list offers the game's tags the mod does not have yet", () => {
	const mod = modFixture({ id: "local:example-mod", tags: ["fantasy"] });
	const html = renderToString(<TagsSection mod={mod} allTags={["fantasy", "horror"]} onSetTags={noop} />);

	// `fantasy` is only the chip: the list leaves out the tags the mod has.
	expect(html.split(">fantasy<").length - 1).toBe(1);
	expect(html).toContain(">horror<");
	expect(html).toContain(">Existing tags<");
});

test("the list says so when the mod has every game tag", () => {
	const mod = modFixture({ id: "local:example-mod", tags: ["fantasy", "horror"] });
	const html = renderToString(<TagsSection mod={mod} allTags={["fantasy", "horror"]} onSetTags={noop} />);

	expect(html).toContain(">No other tags yet.<");
});

test("the tag field has no datalist, so the browser cannot filter the tags away", () => {
	const mod = modFixture({ id: "local:example-mod", tags: [] });
	const html = renderToString(<TagsSection mod={mod} allTags={["horror"]} onSetTags={noop} />);

	expect(html).not.toContain("datalist");
	expect(html).not.toContain("list=");
	expect(html).toContain('id="mod-tags"');
});
