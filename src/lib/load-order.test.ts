import { expect, test } from "bun:test";

import type { LoadOrderRange } from "@/bindings";
import {
	availableMods,
	capacity,
	fits,
	moveBy,
	nothingOrderable,
	numberAt,
	orderableMods,
	orderFromMods,
	sameOrder,
	unorderedNumbers,
	withAppended,
	withoutId,
} from "@/lib/load-order";

const range: LoadOrderRange = { priority_min: 1, priority_max: 3 };

test("the order of a profile is the mods with a number, lowest number first", () => {
	const mods = [
		{ id: "mod-a", load_priority: 2 },
		{ id: "mod-b", load_priority: null },
		{ id: "mod-c", load_priority: 1 },
		{ id: "mod-d" },
	];

	expect(orderFromMods(mods)).toEqual(["mod-c", "mod-a"]);
});

test("zero is a number, and mods with the same number keep their list order", () => {
	const mods = [
		{ id: "mod-a", load_priority: 1 },
		{ id: "mod-b", load_priority: 0 },
		{ id: "mod-c", load_priority: 1 },
	];

	expect(orderFromMods(mods)).toEqual(["mod-b", "mod-a", "mod-c"]);
	expect(orderFromMods([{ id: "mod-d", load_priority: null }])).toEqual([]);
});

test("moving a mod swaps it with its neighbour and stops at the ends", () => {
	const order = ["mod-a", "mod-b", "mod-c"];

	expect(moveBy(order, 1, -1)).toEqual(["mod-b", "mod-a", "mod-c"]);
	expect(moveBy(order, 1, 1)).toEqual(["mod-a", "mod-c", "mod-b"]);
	expect(moveBy(order, 0, -1)).toEqual(order);
	expect(moveBy(order, 2, 1)).toEqual(order);
	expect(order).toEqual(["mod-a", "mod-b", "mod-c"]);
});

test("a move from an index with no mod, or of an empty order, changes nothing", () => {
	expect(moveBy([], 0, 1)).toEqual([]);
	expect(moveBy(["mod-a"], 0, 1)).toEqual(["mod-a"]);
	expect(moveBy(["mod-a", "mod-b"], 5, -1)).toEqual(["mod-a", "mod-b"]);
});

test("a mod is added at the end once and removed by id", () => {
	expect(withAppended(["mod-a"], "mod-b")).toEqual(["mod-a", "mod-b"]);
	expect(withAppended(["mod-a", "mod-b"], "mod-a")).toEqual(["mod-a", "mod-b"]);
	expect(withAppended([], "mod-a")).toEqual(["mod-a"]);
	expect(withoutId(["mod-a", "mod-b"], "mod-a")).toEqual(["mod-b"]);
	expect(withoutId(["mod-a"], "mod-z")).toEqual(["mod-a"]);
});

test("the game's range numbers the mods in order and refuses a longer order", () => {
	expect(capacity(range)).toBe(3);
	expect(numberAt(range, 0)).toBe(1);
	expect(numberAt(range, 2)).toBe(3);
	expect(fits(["a", "b", "c"], range)).toBe(true);
	expect(fits(["a", "b", "c", "d"], range)).toBe(false);
	expect(fits([], range)).toBe(true);
});

test("the numbers start at the lowest number of a range that does not start at one", () => {
	const shifted: LoadOrderRange = { priority_min: 10, priority_max: 11 };

	expect(capacity(shifted)).toBe(2);
	expect(numberAt(shifted, 0)).toBe(10);
	expect(numberAt(shifted, 1)).toBe(11);
	expect(fits(["a", "b", "c"], shifted)).toBe(false);
});

test("two orders are the same only when they list the same mods in the same places", () => {
	expect(sameOrder(["a", "b"], ["a", "b"])).toBe(true);
	expect(sameOrder(["a", "b"], ["b", "a"])).toBe(false);
	expect(sameOrder(["a"], ["a", "b"])).toBe(false);
	expect(sameOrder([], [])).toBe(true);
});

test("the window lists only the mods the game reads in order, and offers only the ones not in the order yet", () => {
	const mods = [
		{ id: "mod-a", orderable: true, load_priority: null },
		{ id: "mod-b", orderable: false, load_priority: null },
		{ id: "mod-c", orderable: true, load_priority: 2 },
		{ id: "mod-d", orderable: true, load_priority: null },
	];

	expect(orderableMods(mods).map((mod) => mod.id)).toEqual(["mod-a", "mod-c", "mod-d"]);
	expect(availableMods(mods, ["mod-c"]).map((mod) => mod.id)).toEqual(["mod-a", "mod-d"]);
	expect(availableMods(mods, []).map((mod) => mod.id)).toEqual(["mod-a", "mod-c", "mod-d"]);
});

test("a number on a mod the game cannot read is reported and kept out of the order", () => {
	const mods = [
		{ id: "mod-a", orderable: true, load_priority: 1 },
		{ id: "mod-b", orderable: false, load_priority: 2 },
		{ id: "mod-c", orderable: false, load_priority: null },
	];

	expect(unorderedNumbers(mods).map((mod) => mod.id)).toEqual(["mod-b"]);
	expect(orderFromMods(orderableMods(mods))).toEqual(["mod-a"]);
});

test("a profile with no mod the game can read in order says so", () => {
	expect(nothingOrderable([{ orderable: false }, { orderable: false }])).toBe(true);
	expect(nothingOrderable([{ orderable: false }, { orderable: true }])).toBe(false);
});
