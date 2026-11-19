import { expect, test } from "bun:test";

import {
	type ColumnId,
	type ColumnLayout,
	DEFAULT_LAYOUT,
	isSavedLayout,
	moveColumnTo,
	orderedColumns,
	restoreLayout,
	stepColumn,
} from "@/lib/mod-columns";

test("a new list shows the columns a player reads first, and pins the switch and the name", () => {
	const shown = orderedColumns(DEFAULT_LAYOUT.order)
		.filter((column) => !DEFAULT_LAYOUT.hidden.includes(column.id))
		.map((column) => column.id);
	expect(shown).toEqual(["order", "enabled", "mod", "version", "conflicts", "missing", "category", "size"]);
	expect(DEFAULT_LAYOUT.hidden).not.toContain("enabled");
	expect(DEFAULT_LAYOUT.hidden).not.toContain("mod");
});

test("a saved layout without an order gets the default one, and a pinned column is never hidden", () => {
	const restored = restoreLayout({ widths: { version: 200 }, hidden: ["mod", "author"] });
	expect(restored.order).toEqual(DEFAULT_LAYOUT.order);
	expect(restored.hidden).toEqual(["author"]);
	expect(restored.widths).toEqual({ version: 200 });
	expect(restoreLayout(null)).toBe(DEFAULT_LAYOUT);
});

test("a saved width below the column minimum is raised to the minimum", () => {
	const restored = restoreLayout({ widths: { version: 1 }, hidden: [] });
	expect(restored.widths.version).toBe(120);
});

test("columns the saved order does not name come last, each once", () => {
	const restored = restoreLayout({ widths: {}, hidden: [], order: ["version", "mod"] });
	expect(restored.order.slice(0, 2)).toEqual(["version", "mod"]);
	expect(new Set(restored.order).size).toBe(DEFAULT_LAYOUT.order.length);
	expect(restored.order).toHaveLength(DEFAULT_LAYOUT.order.length);
});

test("a dragged column lands where the target was, and the others shift over", () => {
	const order: ColumnId[] = ["order", "enabled", "mod", "version"];
	expect(moveColumnTo(order, "version", "order")).toEqual(["version", "order", "enabled", "mod"]);
	expect(moveColumnTo(order, "order", "version")).toEqual(["enabled", "mod", "version", "order"]);
	expect(moveColumnTo(order, "mod", "mod")).toBe(order);
});

test("a step swaps with the neighbour among the visible columns, and keeps hidden ones in place", () => {
	const order: ColumnId[] = ["order", "files", "mod", "version"];
	const visible: ColumnId[] = ["order", "mod", "version"];
	expect(stepColumn(order, visible, "mod", -1)).toEqual(["mod", "files", "order", "version"]);
	expect(stepColumn(order, visible, "order", -1)).toBe(order);
	expect(stepColumn(order, visible, "version", 1)).toBe(order);
	expect(stepColumn(order, visible, "files", 1)).toBe(order);
});

test("only a layout of the right shape is read back", () => {
	expect(isSavedLayout({ widths: { mod: 300 }, hidden: ["notes"] })).toBe(true);
	expect(isSavedLayout({ widths: {}, hidden: [], order: ["mod", "mod"] })).toBe(false);
	expect(isSavedLayout({ widths: {}, hidden: ["unknown"] })).toBe(false);
	expect(isSavedLayout({ widths: { mod: -1 }, hidden: [] })).toBe(false);
});

test("a layout written to storage and read back after a restart is the same one", () => {
	const layout: ColumnLayout = {
		widths: { version: 220, archive: 300 },
		hidden: ["files", "notes"],
		order: moveColumnTo(DEFAULT_LAYOUT.order, "version", "order"),
	};
	// Storage keeps text: `writePref` stores the JSON, and `readPref` parses it again after a restart.
	const text = JSON.stringify(layout);
	const stored: unknown = JSON.parse(text);
	expect(isSavedLayout(stored)).toBe(true);
	expect(isSavedLayout(stored) ? restoreLayout(stored) : null).toEqual(layout);
});
