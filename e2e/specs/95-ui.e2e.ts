import { strict as assert } from "node:assert";
import { browser } from "@wdio/globals";
import type { AppSettings_Serialize as AppSettings } from "../../src/bindings";
import { invoke, reloadWindow } from "../support/bridge";
import { modZip } from "../support/fixtures";
import { cleanupFixture, enableMods, importMods, seedGame, uniqueId } from "../support/seed";
import {
	clearGobboStorage,
	clickByLabel,
	clickByLabelPrefix,
	clickNativeByText,
	press,
	pressKeyOn,
	textOf,
	typeInto,
	waitForDialog,
	waitForHeading,
	waitForMod,
	waitForNoSelector,
	waitForPlaceholder,
	waitForSelector,
	waitForText,
} from "../support/ui";

async function waitForInspector(label: string): Promise<void> {
	await browser.waitUntil(
		async () =>
			browser.execute(
				(wanted: string) => [...document.querySelectorAll("[aria-label]")].some((element) => element.getAttribute("aria-label") === wanted),
				label,
			),
		{
			timeout: 30_000,
			timeoutMsg: `the inspector toggle ${label} did not appear`,
		},
	);
}

describe("window and keyboard", () => {
	it("opens the command palette with Ctrl+K, runs an action and finds a mod", async () => {
		const id = uniqueId("ui-palette");
		const name = `E2E Palette ${id}`;
		const { fixture } = await seedGame({ id, name });
		await importMods(id, [modZip(`palette-${id}.zip`, `Palette ${id}`)]);
		await enableMods(id);
		await reloadWindow();
		await waitForMod(`palette-${id}`);
		await pressKeyOn("body", "k", { control: true });
		await waitForDialog("Commands");
		// The search hint is the input's placeholder, not text in the document.
		await waitForPlaceholder("Search actions, mods, profiles…");
		await typeInto("Commands", "Game settings");
		await clickNativeByText("Game settings", "[cmdk-item]", "[role='dialog']");
		await waitForHeading(`${name} settings`);
		await pressKeyOn("body", "k", { control: true });
		await waitForDialog("Commands");
		await typeInto("Commands", `Mod list of ${name}`);
		await clickNativeByText(`Mod list of ${name}`, "[cmdk-item]", "[role='dialog']");
		await waitForHeading(name);
		await pressKeyOn("body", "k", { control: true });
		await waitForDialog("Commands");
		await typeInto("Commands", `palette-${id}`);
		await clickNativeByText(`palette-${id}`, "[cmdk-item]", "[role='dialog']");
		await browser.waitUntil(async () => (await textOf("aside h2")).includes(`palette-${id}`), {
			timeout: 30_000,
			timeoutMsg: "the palette did not select the mod",
		});
		await cleanupFixture(id, fixture.root);
	});
	it("shows the shortcut list with ? and F1 and closes it with Escape", async () => {
		await reloadWindow();
		await pressKeyOn("body", "?", { shift: true });
		await waitForDialog("Keyboard shortcuts");
		await waitForText("Commands: search actions, mods and games");
		await waitForText("Show or hide the details panel");
		await waitForText("With the mouse");
		await press("Escape");
		await waitForNoSelector("[role='dialog']");
		await pressKeyOn("body", "F1");
		await waitForDialog("Keyboard shortcuts");
		await press("Escape");
		await waitForNoSelector("[role='dialog']");
	});
	it("toggles the details panel and the console with Ctrl+I and Ctrl+J", async () => {
		const id = uniqueId("ui-shortcuts");
		const name = `E2E Shortcuts ${id}`;
		const { fixture } = await seedGame({ id, name });
		await waitForInspector("Hide details panel");
		await pressKeyOn("body", "i", { control: true });
		await waitForInspector("Show details panel");
		const hidden = await invoke<AppSettings>("app_settings");
		assert.equal(hidden.inspector_collapsed, true, "Ctrl+I did not save the collapsed details panel");
		await pressKeyOn("body", "i", { control: true });
		await waitForInspector("Hide details panel");
		const shown = await invoke<AppSettings>("app_settings");
		assert.equal(shown.inspector_collapsed, false);
		await pressKeyOn("body", "j", { control: true });
		await waitForSelector("[aria-label='Close console']");
		await pressKeyOn("body", "j", { control: true });
		await waitForNoSelector("[aria-label='Close console']");
		await cleanupFixture(id, fixture.root);
	});
	it("keeps the rail and the views usable after a narrow window", async () => {
		await browser.setWindowSize(900, 700);
		await reloadWindow();
		await clickByLabel("Storage");
		await waitForHeading("Storage");
		await clickByLabel("Downloads");
		await waitForHeading("Downloads");
		await clickByLabelPrefix("Notifications", "nav");
		// The rail opens the Notifications page; the notice panel is no longer a popover.
		await waitForHeading("Notifications");
		await browser.setWindowSize(1440, 900);
		await clearGobboStorage();
	});
});
