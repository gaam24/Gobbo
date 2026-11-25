import { expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";

// The Radix menu mounts its content only after the first open, which a server render cannot do. Inline primitives keep
// every entry in the markup, so the check sees what the list offers.
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

const { SuggestionInput } = await import("@/components/suggestion-input");

type Props = Parameters<typeof SuggestionInput>[0];

function render(overrides: Partial<Props>): string {
	return renderToString(
		<SuggestionInput id="field" value="" onChange={() => undefined} suggestions={[]} menuLabel="Known" placeholder="Type" {...overrides} />,
	);
}

test("every entry is listed, whatever the field holds", () => {
	const html = render({ value: "fan", suggestions: ["fantasy", "horror"] });

	expect(html).toContain('value="fan"');
	expect(html).toContain(">fantasy<");
	expect(html).toContain(">horror<");
});

test("the empty label is listed when there is nothing to offer", () => {
	const html = render({ suggestions: [], emptyLabel: "Nothing here yet." });

	expect(html).toContain(">Nothing here yet.<");
});

test("an empty list without an empty label shows no entry at all", () => {
	const html = render({ suggestions: [] });

	expect(html).toContain(">Known<");
	expect(html).not.toContain("Nothing here yet.");
});

test("the controls passed as children come after the list button", () => {
	const html = render({ suggestions: ["fantasy"], children: <span>Browse it</span> });

	expect(html.indexOf("Browse it")).toBeGreaterThan(html.indexOf(">fantasy<"));
});

test("the field keeps its id and placeholder and has no datalist", () => {
	const html = render({ suggestions: ["fantasy"] });

	expect(html).toContain('id="field"');
	expect(html).toContain('placeholder="Type"');
	expect(html).not.toContain("list=");
	expect(html).not.toContain("datalist");
});
