import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import { AvailableRows, OrderedRows, type OrderRowMod } from "@/views/load-order/load-order-rows";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const range = { priority_min: 1, priority_max: 10 };
const ordered: OrderRowMod[] = [
	{ id: "first", name: "First Mod", enabled: true },
	{ id: "second", name: "Second Mod", enabled: false },
];
const available: OrderRowMod[] = [{ id: "third", name: "Third Mod", enabled: true }];

const DISABLED = /\sdisabled=""/;

const nothing = () => {
	// The rows are drawn in these structural checks, never clicked.
};

/// The opening tags of the buttons with one `data-action`, in the order they are drawn.
function buttonsOf(html: string, action: string): string[] {
	return (html.match(/<button[^>]*>/g) ?? []).filter((tag) => tag.includes(`data-action="${action}"`));
}

test("while a save runs, every control of the order is off", () => {
	const html = renderToString(<OrderedRows mods={ordered} range={range} busy onMove={nothing} onRemove={nothing} />);
	const controls = ["up", "down", "remove"].flatMap((action) => buttonsOf(html, action));
	expect(controls).toHaveLength(6);
	for (const control of controls) expect(control).toMatch(DISABLED);
});

test("an idle order turns off only the moves past either end", () => {
	const html = renderToString(<OrderedRows mods={ordered} range={range} busy={false} onMove={nothing} onRemove={nothing} />);
	const off = (action: string) => buttonsOf(html, action).map((tag) => DISABLED.test(tag));
	expect(off("up")).toEqual([true, false]);
	expect(off("down")).toEqual([false, true]);
	expect(off("remove")).toEqual([false, false]);
});

test("while a save runs, the add buttons of the other mods are off", () => {
	const saving = buttonsOf(renderToString(<AvailableRows mods={available} busy onAdd={nothing} />), "add");
	const idle = buttonsOf(renderToString(<AvailableRows mods={available} busy={false} onAdd={nothing} />), "add");
	expect(saving).toHaveLength(1);
	expect(saving[0]).toMatch(DISABLED);
	expect(idle).toHaveLength(1);
	expect(idle[0]).not.toMatch(DISABLED);
});
