import { strict as assert } from "node:assert";
import { browser } from "@wdio/globals";
import { reloadWindow } from "../support/bridge";
import { modZip } from "../support/fixtures";
import { cleanupFixture, enableMods, importMods, seedGame, uniqueId } from "../support/seed";
import { waitForMod } from "../support/ui";

const MODS = 60;
const FRAMES = 40;
const STEP = 40;

/// A programmatic scroll is not wheel input, but it exercises the same path that made the list jump: the virtual
/// window changes while scrolling, and scroll anchoring used to cancel most of each step. With the virtualizer and
/// `overflow-anchor: none` every frame must advance by the full step.
async function scrollSteps(): Promise<{ steps: number[]; scrollTop: number }> {
	return (await browser.execute(
		(frames: number, step: number) =>
			new Promise<{ steps: number[]; scrollTop: number }>((resolve) => {
				const row = document.querySelector<HTMLElement>("[data-mod]");
				let container: HTMLElement | null = row;

				while (container && !["auto", "scroll", "overlay"].includes(getComputedStyle(container).overflowY)) {
					container = container.parentElement;
				}

				if (!container) {
					resolve({ steps: [], scrollTop: -1 });
					return;
				}
				const steps: number[] = [];
				let count = 0;
				const tick = () => {
					const before = container ? container.scrollTop : 0;

					if (container) container.scrollTop += step;
					steps.push((container ? container.scrollTop : 0) - before);
					count += 1;

					if (count < frames) requestAnimationFrame(tick);
					else resolve({ steps, scrollTop: container ? container.scrollTop : -1 });
				};
				requestAnimationFrame(tick);
			}),
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
			await cleanupFixture(id, fixture.root);
		}
	});
});
