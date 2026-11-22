import { strict as assert } from "node:assert";
import { browser } from "@wdio/globals";
import type { StorageInfo } from "../../src/bindings";
import { invoke, waitForShell } from "../support/bridge";
import { dataDir } from "../support/env";
import { bodyText, clickByLabel, clickTab, count, openTooltipByLabel, railView, waitForText } from "../support/ui";

/// The first run: no game is added, so the Games page is the only screen with content. The suite writes an English
/// settings file before the app starts, so these assertions do not depend on the machine's locale.
describe("first run", () => {
	it("opens on the Games page with no game added", async () => {
		await waitForShell();
		const view = await railView();
		assert.equal(view, "Games", `the first run opened ${view ?? "no view"} instead of Games`);
		await waitForText("Games");
		// The first-run explanation is a HelpHint tooltip, so it is only in the DOM once the trigger is opened.
		await openTooltipByLabel("More information");
		await waitForText(
			"Gobbo finds games from Steam, GOG, Epic, Heroic and Lutris on this PC. Only games you add get mods; removed games wait on the Storage page.",
		);
		const text = await bodyText();
		assert.ok(text.includes("Add") || text.includes("Locate"), "the catalog has no action for a game");
		assert.equal(await count("nav [aria-label^='E2E']"), 0, "a game was already added");
	});
	it("opens the app settings on the expected page", async () => {
		await clickByLabel("Gobbo Mods Organizer settings");
		const view = await railView();
		assert.equal(view, "Gobbo Mods Organizer settings");
	});
	it("shows the custom data folder and its storage mode", async () => {
		const info = await invoke<StorageInfo>("storage_info");
		assert.equal(info.mode, "custom");
		assert.equal(info.root, dataDir);
		await clickTab("Files and folders");
		const text = await bodyText();
		assert.ok(text.includes(dataDir), `the settings do not show the data folder: ${dataDir}`);
		assert.ok(text.includes("Custom"), "the custom storage mode is not shown");
	});
	it("opens the empty notifications, downloads and storage screens", async () => {
		await clickByLabel("Notifications");
		await waitForText("No notifications.");
		await clickByLabel("Downloads");
		await waitForText("No downloads yet.");
		await clickByLabel("Storage");
		await waitForText("No game data yet");
		await waitForText("Add a game and its data folder shows up here.");
	});
	it("shows the language setting in English", async () => {
		await clickByLabel("Gobbo Mods Organizer settings");
		await waitForText("Language");
		await waitForText("English");
		const language = await browser.execute(() => document.documentElement.lang);
		assert.equal(language, "en");
	});
});
