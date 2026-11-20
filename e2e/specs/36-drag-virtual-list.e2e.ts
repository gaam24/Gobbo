import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import { reloadWindow } from "../support/bridge";
import { definitionsDir } from "../support/env";
import { removeTree } from "../support/files";
import { modZip } from "../support/fixtures";
import { cleanupGame, enableMods, importMods, seedGame, statusOf, uniqueId } from "../support/seed";
import { clickByLabel, waitForMod, waitForSelector } from "../support/ui";

const MODS = 20;
interface Point {
	x: number;
	y: number;
}

/// The list and the grid only mount the rows around the viewport. Dragging must still reach a row inside the window
/// and keep working when auto-scroll carries the pointer past it.
async function points(fromSelector: string, fromIndex: number, toSelector: string, toIndex: number): Promise<{ from: Point; to: Point }> {
	const found = (await browser.execute(
		(fromSel: string, fromAt: number, toSel: string, toAt: number) => {
			const from = document.querySelectorAll<HTMLElement>(fromSel)[fromAt]?.getBoundingClientRect();
			const to = document.querySelectorAll<HTMLElement>(toSel)[toAt]?.getBoundingClientRect();

			if (!(from && to)) return null;
			return {
				from: {
					x: Math.round(from.left + from.width / 2),
					y: Math.round(from.top + from.height / 2),
				},
				to: {
					x: Math.round(to.left + to.width / 2),
					y: Math.round(to.top + to.height / 2),
				},
			};
		},
		fromSelector,
		fromIndex,
		toSelector,
		toIndex,
	)) as { from: Point; to: Point } | null;
	assert.ok(found, "the drag handles were not rendered");
	return found;
}

async function drag(from: Point, to: Point) {
	// A short nudge first: dnd-kit's pointer sensor starts after a 4 px move, and the drop needs a settled frame.
	await browser
		.action("pointer")
		.move({ duration: 0, x: from.x, y: from.y })
		.down({ button: 0 })
		.pause(120)
		.move({ duration: 60, x: from.x + 4, y: from.y + 12 })
		.pause(80)
		.move({ duration: 400, x: to.x, y: to.y })
		.pause(600)
		.perform(true);
	await browser.action("pointer").up({ button: 0 }).perform();
}

async function orderAfter(id: string, before: string[]): Promise<string[]> {
	for (let attempt = 0; attempt < 30; attempt += 1) {
		const after = (await statusOf(id)).mods.map((mod) => mod.folder);

		if (after.join(",") !== before.join(",")) return after;
		await browser.pause(200);
	}

	return (await statusOf(id)).mods.map((mod) => mod.folder);
}

async function game(prefix: string) {
	const id = uniqueId(prefix);
	const { fixture } = await seedGame({ id, name: `E2E Drag ${id}` });
	const archives = Array.from({ length: MODS }, (_, index) => modZip(`drag-${id}-${index}.zip`, `Drag ${index}`));
	await importMods(id, archives);
	await enableMods(id);
	await reloadWindow();
	return { id, fixture };
}

describe("drag in a virtualized mod list", () => {
	it("moves a table row onto a row inside the window", async () => {
		const { id, fixture } = await game("drag-table");

		try {
			await waitForMod(`drag-${id}-0`);
			const before = (await statusOf(id)).mods.map((mod) => mod.folder);
			const moved = before[1];
			assert.ok(moved, "the list is too short");
			const { from, to } = await points("[aria-label^='Drag to reorder']", 1, "[data-grid-row][role='row']", 5);
			await drag(from, to);
			const after = await orderAfter(id, before);
			assert.notEqual(before.join(","), after.join(","), "the row was not reordered");
			assert.ok(after.indexOf(moved) > 1, "the row did not move towards the drop target");
		} finally {
			await cleanupGame(id);
			removeTree(path.join(definitionsDir, id));
			removeTree(fixture.root);
		}
	});

	it("moves a grid card onto a card inside the window", async () => {
		const { id, fixture } = await game("drag-grid");

		try {
			await clickByLabel("Grid view");
			await waitForSelector("[data-grid-card]");
			const before = (await statusOf(id)).mods.map((mod) => mod.folder);
			const moved = before[1];
			assert.ok(moved, "the list is too short");
			const { from, to } = await points("[aria-label^='Drag to reorder']", 1, "[data-grid-card]", 5);
			await drag(from, to);
			const after = await orderAfter(id, before);
			assert.notEqual(before.join(","), after.join(","), "the card was not reordered");
			assert.ok(after.indexOf(moved) > 1, "the card did not move towards the drop target");
		} finally {
			await cleanupGame(id);
			removeTree(path.join(definitionsDir, id));
			removeTree(fixture.root);
		}
	});

	it("keeps dragging while auto-scroll passes the window", async () => {
		const { id, fixture } = await game("drag-scroll");

		try {
			await waitForMod(`drag-${id}-0`);
			const before = (await statusOf(id)).mods.map((mod) => mod.folder);
			const moved = before[0];
			assert.ok(moved, "the list is too short");
			const edge = (await browser.execute(() => {
				const row = document.querySelector<HTMLElement>("[data-mod]");
				let element: HTMLElement | null = row;

				while (element && !["auto", "scroll"].includes(getComputedStyle(element).overflowY)) element = element.parentElement;

				if (!element) return null;
				const box = element.getBoundingClientRect();
				return {
					x: Math.round(box.left + box.width / 2),
					bottom: Math.round(box.bottom),
				};
			})) as { x: number; bottom: number } | null;
			assert.ok(edge, "the scroll container was not found");
			const { from } = await points("[aria-label^='Drag to reorder']", 0, "[aria-label^='Drag to reorder']", 0);
			await browser
				.action("pointer")
				.move({ duration: 0, x: from.x, y: from.y })
				.down({ button: 0 })
				.pause(120)
				.move({ duration: 150, x: edge.x, y: edge.bottom - 25 })
				.pause(1200)
				.move({ duration: 150, x: edge.x, y: edge.bottom - 25 })
				.pause(1200)
				.perform(true);
			await browser.action("pointer").up({ button: 0 }).perform();
			const after = await orderAfter(id, before);
			assert.notEqual(before.join(","), after.join(","), "auto-scroll cancelled the drag");
			assert.ok(after.indexOf(moved) > 0, "the dragged row stayed at the top");
		} finally {
			await cleanupGame(id);
			removeTree(path.join(definitionsDir, id));
			removeTree(fixture.root);
		}
	});
});
