import { strict as assert } from "node:assert";
import { browser } from "@wdio/globals";
import type { DownloadView } from "../../src/bindings";
import { invoke } from "../support/bridge";
import { cleanupFixture, seedGame, uniqueId } from "../support/seed";

/// On desktop a deep link arrives as a command-line argument of a second app process. The single-instance plugin
/// forwards that argument to the running app, which emits `deep-link://new-url`; the e2e build deliberately has no
/// second instance, so the spec emits the same event. What it proves is Gobbo's half of the hand-off: a link that
/// arrives while the window is open reaches the receiver and becomes a queue entry without a reload.
describe("desktop deep links", () => {
	it("queues a provider link that arrives while the app is running", async () => {
		const id = uniqueId("deeplink");
		const name = `E2E Deep Link ${id}`;
		const { fixture } = await seedGame({ id, name, provider: "nexus" });

		try {
			await invoke("plugin:event|emit", {
				event: "deep-link://new-url",
				payload: [`nxm://${id}/mods/4242/files/99`],
			});

			await browser.waitUntil(
				async () => (await invoke<DownloadView[]>("list_downloads")).some((entry) => entry.source?.mod_id === "4242"),
				{
					timeout: 10_000,
					timeoutMsg: "the link never reached the download queue",
				},
			);
			const queued = (await invoke<DownloadView[]>("list_downloads")).find((entry) => entry.source?.mod_id === "4242");
			assert.ok(queued, "the queued link disappeared");
			assert.equal(queued.source?.provider, "nexus");
			assert.equal(queued.source?.file_id, "99");
			assert.ok(queued.name.includes("4242"), `the queue does not name the link: ${queued.name}`);
			// The suite has no key or network, so the fetch fails. The entry stays in the queue for a later retry.
		} finally {
			await cleanupFixture(id, fixture.root);
		}
	});
});
