import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import { reloadWindow } from "../support/bridge";
import { definitionsDir } from "../support/env";
import { removeTree } from "../support/files";
import { modZip } from "../support/fixtures";
import { cleanupGame, enableMods, importMods, seedGame, uniqueId } from "../support/seed";
import { waitForMod } from "../support/ui";

const MODS = 60;
const FRAMES = 40;
const STEP = 40;

/// A programmatic scroll is not wheel input, but it exercises the same path that made the list jump: the virtual
/// window changes while scrolling, and scroll anchoring used to cancel most of each step. With the virtualizer and
/// `overflow-anchor: none` every frame must advance by the full step.
async function scrollSteps(): Promise<{ steps: number[]; scrollTop: number }> {
	return (await browser.executeAsync(
		(frames: number, step: number, done: (value: { steps: number[]; scrollTop: number }) => void) => {
			const row = document.querySelector<HTMLElement>("[data-mod]");
			let container: HTMLElement | null = row;
			while (container && !["auto", "scroll", "overlay"].includes(getComputedStyle(container).overflowY)) {
				container = container.parentElement;
			}
			if (!container) return done({ steps: [], scrollTop: -1 });
			const steps: number[] = [];
			let count = 0;
			const tick = () => {
				const before = container ? container.scrollTop : 0;
				if (container) container.scrollTop += step;
				steps.push((container ? container.scrollTop : 0) - before);
				count += 1;
				if (count < frames) requestAnimationFrame(tick);
				else done({ steps, scrollTop: container ? container.scrollTop : -1 });
			};
			requestAnimationFrame(tick);
		},
		FRAMES,
		STEP,
	)) as { steps: number[]; scrollTop: number };
}

describe("mod list scroll stability", () => {
	it("keeps every scroll step when the virtual window changes", async () => {
		const id = uniqueId("scroll-stability");
		const { fixture } = await seedGame({ id, name: `E2E Scroll ${id}` });
		try {
			const archives = Array.from({ length: MODS }, (_, index) => modZip(`scroll-${id}-${index}.zip`, `Scroll ${index}`));
			await importMods(id, archives);
			await enableMods(id);
			await reloadWindow();
			await waitForMod(`scroll-${id}-0`);

			const { steps, scrollTop } = await scrollSteps();
			assert.equal(steps.length, FRAMES, "every frame must report a scroll step");
			assert.ok(scrollTop > 0, "the list did not scroll");
			const lost = steps.filter((step) => step < STEP - 5);
			assert.equal(lost.length, 0, `the browser cancelled ${lost.length} of ${FRAMES} scroll steps (${lost.slice(0, 5).join(", ")} px)`);
		} finally {
			await cleanupGame(id);
			removeTree(path.join(definitionsDir, id));
			removeTree(fixture.root);
		}
	});
});
