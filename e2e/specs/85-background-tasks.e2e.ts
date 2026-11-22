import { strict as assert } from "node:assert";
import { browser } from "@wdio/globals";
import { cleanupFixture, seedGame, uniqueId } from "../support/seed";
import { clickByText, clickGame, railView, textOf, waitForSelector, waitForText } from "../support/ui";

const WORKING = "[aria-label='Working']";
/// The title the window derives from the backend `collection` kind code (the suite runs in English).
const TITLE = "Installing collection…";

/// MOCK-DRIVEN: a real install needs a provider, a key and network, which the suite does not have. Instead of injecting
/// an event with WebDriver's `plugin:event|emit`, the spec calls the window's e2e seam, which emits through the same
/// lazy Tauri event module the registry listener uses. The payload is the backend snapshot shape: the real adapter
/// derives the title from `kind` and the game name from `game`, so the row under test is the app's own.
async function emitTask(event: E2eTaskEvent, payload: E2eTaskPayload): Promise<void> {
	await browser.execute(
		async (name: E2eTaskEvent, data: E2eTaskPayload) => {
			// oxlint-disable-next-line eslint/no-underscore-dangle -- the hook follows the existing __GOBBO_E2E_* naming convention.
			const hook = window.__GOBBO_E2E_TASKS__;

			if (!hook) throw new Error("the E2E task event hook is missing");
			await hook(name, data);
		},
		event,
		payload,
	);
}

function taskFor(game: string, state = "running"): E2eTaskPayload {
	return { id: `collection:${game}:main`, game, kind: "collection", state };
}

async function waitForWorkingText(text: string, wanted = true) {
	await browser.waitUntil(async () => (await textOf(WORKING)).includes(text) === wanted, {
		timeout: 30_000,
		timeoutMsg: `Working ${wanted ? "did not show" : "still shows"} ${JSON.stringify(text)}`,
	});
}

/// A task started on one game stays in Working when another game is active, and both Stop and Open act on the task's
/// own game, not on the one currently shown.
describe("background tasks", () => {
	it("keeps a game's task in Working across a switch and controls it from there", async () => {
		const idA = uniqueId("alpha");
		const nameA = "Alpha Game";
		const idB = uniqueId("beta");
		const nameB = "Beta Game";
		const { fixture: fixtureA } = await seedGame({ id: idA, name: nameA });
		const { fixture: fixtureB } = await seedGame({ id: idB, name: nameB });

		try {
			// Start the work on game A.
			await clickGame(nameA);
			await emitTask("task-started", taskFor(idA));
			await waitForSelector(WORKING);
			await waitForWorkingText(TITLE);
			assert.ok((await textOf(WORKING)).includes(nameA), "the Working row does not name its game");

			// Switch to game B while A's task is still running. The row and its game name stay.
			await clickGame(nameB);
			assert.equal(await railView(), nameB, "game B was not activated");
			await waitForWorkingText(TITLE);
			const shown = await textOf(WORKING);
			assert.ok(shown.includes(nameA), `Working lost game A's name after the switch: ${shown}`);

			// MOCK-DRIVEN: there is no real install behind the row, so Cancel cannot stop a backend task. The click is a
			// real UI interaction; the spec then emits the `task-cancelled` answer the controller would have published.
			// A running backend row offers Cancel (tasks-store's generic cancel action), not the feature-owned Stop.
			await clickByText("Cancel", "button", WORKING);
			await emitTask("task-cancelled", taskFor(idA, "cancelled"));
			await waitForWorkingText(TITLE, false);

			// MOCK-DRIVEN: reopening a stopped task starts it again with the same key. What is asserted is the Open
			// action's contract — the task's own game is activated, not the one currently shown.
			await emitTask("task-started", taskFor(idA));
			await waitForWorkingText(TITLE);
			await clickByText("Open", "button", WORKING);
			assert.equal(await railView(), nameA, "Open did not switch back to the task's game");
			await waitForText(nameA);
		} finally {
			// Clear the injected row so it cannot leak into the next spec in the same window.
			await emitTask("task-finished", taskFor(idA, "done"));
			await cleanupFixture([idA, idB], [fixtureA.root, fixtureB.root]);
		}
	});
});
