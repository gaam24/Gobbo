import { expect, test } from "bun:test";

import { type GridCell, gridNeighbour } from "@/lib/grid-neighbour";

/// A card 100 px wide and 60 px tall; column and row are its place in a grid with no gaps.
function cell(id: string, row: number, column: number): GridCell {
	return { id, top: row * 60, left: column * 100, right: column * 100 + 100 };
}

const grid = [cell("a", 0, 0), cell("b", 0, 1), cell("c", 0, 2), cell("d", 1, 0), cell("e", 1, 1)];

test("the card below and above is the one in the same column", () => {
	expect(gridNeighbour(grid, "a", "down")).toBe("d");
	expect(gridNeighbour(grid, "d", "up")).toBe("a");
});

test("a card with no card in the same column takes the one nearest its centre", () => {
	// c sits over the empty third column of the second row: e (centre 150) is nearer than d (centre 50).
	expect(gridNeighbour(grid, "c", "down")).toBe("e");
	expect(gridNeighbour(grid, "e", "up")).toBe("b");
});

test("the edges of the grid have no neighbour", () => {
	expect(gridNeighbour(grid, "a", "up")).toBeNull();
	expect(gridNeighbour(grid, "e", "down")).toBeNull();
});

test("a row further away is never chosen while a nearer row exists", () => {
	const tall = [cell("top", 0, 0), cell("near", 1, 0), cell("far", 2, 0)];
	expect(gridNeighbour(tall, "top", "down")).toBe("near");
	expect(gridNeighbour(tall, "far", "up")).toBe("near");
});

test("a card that is not on the screen has no neighbour", () => {
	expect(gridNeighbour(grid, "missing", "down")).toBeNull();
});
