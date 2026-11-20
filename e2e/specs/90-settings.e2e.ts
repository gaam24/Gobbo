import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import type { AppSettings_Serialize as AppSettings } from "../../src/bindings";
import { invoke, reloadWindow } from "../support/bridge";
import { dataDir, definitionsDir, fixturesDir } from "../support/env";
import { listDir, readJson, removeTree, writeFile, zipNames } from "../support/files";
import { cleanupGame, seedGame, statusOf, uniqueId, waitForStatus } from "../support/seed";
import {
	bodyText,
	chooseSelect,
	clickByLabel,
	clickByText,
	clickTab,
	openTooltipByLabel,
	pressKeyOn,
	setSwitch,
	typeInto,
	waitForHeading,
	waitForText,
} from "../support/ui";

async function openAppSettings(): Promise<void> {
	await clickByLabel("Gobbo Mods Organizer settings");
	await waitForHeading("Gobbo Mods Organizer settings");
	// The tab is component state: when the settings page is already open, clicking the rail again leaves the last tab
	// selected. Every test below starts from the interface section.
	await clickTab("Interface");
}

function appSettings(): Promise<AppSettings> {
	return invoke<AppSettings>("app_settings");
}

async function waitForAppSetting(check: (settings: AppSettings) => boolean): Promise<AppSettings> {
	const deadline = Date.now() + 30_000;
	let current = await appSettings();

	while (!check(current)) {
		if (Date.now() > deadline) throw new Error(`the app settings did not change: ${JSON.stringify(current)}`);
		await new Promise<void>((resolve) => {
			setTimeout(resolve, 100);
		});
		current = await appSettings();
	}

	return current;
}

/// The suite asserts English labels, so a test that changes the language must leave it English again, also when it
/// fails halfway.
async function ensureEnglish(): Promise<void> {
	const settings = await appSettings();

	if (settings.language === "en") return;
	await invoke("save_app_settings", { settings: { ...settings, language: "en" } });
	await reloadWindow();
}

describe("settings", () => {
	it("shows the shared settings, the providers and the custom data folder", async () => {
		await ensureEnglish();
		await reloadWindow();
		await openAppSettings();
		// The app settings are split into vertical tabs. The interface section is the one open by default.
		// The page hint lives behind its question mark, so open the tooltip before reading it.
		await openTooltipByLabel("More information");
		let text = await bodyText();
		assert.ok(text.includes("Shared by every game"), "the app settings hint is missing");
		assert.ok(text.includes("Remember the window"), "the window setting is missing");
		await clickTab("Mod providers");
		text = await bodyText();
		assert.ok(text.includes("API key"), "the provider key section is missing");
		assert.ok(
			text.includes("No key saved. Update checks are off.") || text.includes("A key is saved. Update checks use it."),
			"the provider key state is missing",
		);
		assert.ok(text.includes("Download links"), "the link section is missing");
		await clickTab("Files and folders");
		text = await bodyText();
		assert.ok(text.includes(dataDir), "the custom data folder is not shown");
		assert.ok(text.includes("Custom"), "the custom storage mode is not shown");
	});
	it("switches the language at once and keeps it across a reload", async () => {
		await ensureEnglish();
		await openAppSettings();

		try {
			await chooseSelect("#app-language", "Polski");
			await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.lang)) === "pl", {
				timeout: 30_000,
				timeoutMsg: "the window did not switch to Polish",
			});
			await waitForText("Język");
			await waitForAppSetting((settings) => settings.language === "pl");
			await reloadWindow();
			await clickByLabel("Ustawienia Gobbo Mods Organizer");
			await waitForText("Ustawienia Gobbo Mods Organizer");
			await chooseSelect("#app-language", "English");
			await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.lang)) === "en", {
				timeout: 30_000,
				timeoutMsg: "the window did not switch back to English",
			});
			await waitForText("Language");
			await waitForAppSetting((settings) => settings.language === "en");
		} finally {
			await ensureEnglish();
		}
	});
	it("saves the window setting and reads it back from the file", async () => {
		await openAppSettings();
		await setSwitch("remember-window", false);
		await waitForAppSetting((settings) => !settings.remember_window);
		assert.equal(readJson<{ remember_window: boolean }>(path.join(dataDir, "settings.json")).remember_window, false);
		await setSwitch("remember-window", true);
		await waitForAppSetting((settings) => settings.remember_window);
	});
	it("writes a support bundle from the settings page", async () => {
		await openAppSettings();
		await clickTab("Support");
		await clickByText("Create support bundle", "button");
		await waitForText("Show the bundle");
		const dir = path.join(dataDir, "support");
		await browser.waitUntil(() => listDir(dir).some((name) => name.endsWith(".zip")), {
			timeout: 30_000,
			timeoutMsg: "the support bundle was not written",
		});
		const file = path.join(dir, listDir(dir).find((name) => name.endsWith(".zip")) ?? "");
		const names = zipNames(file);
		assert.ok(names.includes("system.txt"), `the system file is missing from ${names.join(", ")}`);
		assert.ok(names.includes("README.txt"), "the readme is missing from the bundle");
		assert.ok(
			names.every((name) => !name.includes("library")),
			"the bundle contains the mod library",
		);
	});
	it("saves the game folders and start options, auto-saves a draft and edits per-profile files", async () => {
		await ensureEnglish();
		const id = uniqueId("settings-game");
		const name = `E2E Settings ${id}`;
		const { fixture } = await seedGame({ id, name, userData: true });
		await clickByLabel("Game settings");
		await waitForHeading(`${name} settings`);
		// The game settings are split into vertical tabs; the start options live in Start options.
		await clickTab("Start");
		await typeInto("#launch-args", "-e2e -fast");
		await typeInto("#launch-env", "E2E_SETTINGS=one\nE2E_SECOND=two");
		// Text fields commit when they lose focus. There is no Save button and no draft to discard any more.
		await pressKeyOn("#launch-args", "Enter");
		const saved = await waitForStatus(
			id,
			(current) => current.settings.launch_args === "-e2e -fast" && current.settings.launch_env.includes("E2E_SECOND=two"),
		);
		assert.equal(saved.settings.launch_args, "-e2e -fast");
		const config = readJson<{ launch_args: string; launch_env: string }>(path.join(dataDir, "games", id, "config.json"));
		assert.equal(config.launch_args, "-e2e -fast");
		assert.ok(config.launch_env.includes("E2E_SETTINGS=one"));
		await clickTab("Saves");
		await setSwitch("user-data-saves", true);
		const isolated = await waitForStatus(id, (current) => current.user_data.some((set) => set.id === "saves" && set.isolated));
		assert.ok(isolated.user_data[0]?.isolated);
		await setSwitch("user-data-saves", false);
		await waitForStatus(id, (current) => current.user_data.some((set) => set.id === "saves" && !set.isolated));
		const extra = path.join(fixturesDir, `settings-${id}.ini`);
		writeFile(extra, "setting=1\n");
		await browser.execute((file: string) => {
			window.__GOBBO_E2E_DIALOGS__ = { file };
		}, extra);
		await clickByText("Add file", "button");
		await waitForText(extra);
		await waitForStatus(id, (current) => current.settings.profile_data.includes(extra));
		await clickByLabel(`Remove ${extra} from per-profile data`);
		await waitForStatus(id, (current) => !current.settings.profile_data.includes(extra));
		// A second edit is committed on blur, so leaving the page cannot lose it.
		await clickTab("Start");
		await typeInto("#launch-args", "-e2e -auto");
		await pressKeyOn("#launch-args", "Enter");
		await waitForStatus(id, (current) => current.settings.launch_args === "-e2e -auto");
		await clickByLabel(name);
		await waitForHeading(name);
		assert.equal((await statusOf(id)).settings.launch_args, "-e2e -auto");
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
});
