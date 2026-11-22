import { strict as assert } from "node:assert";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { browser } from "@wdio/globals";

import { invoke, reloadWindow } from "../support/bridge";
import { definitionsDir, GOBBO_E2E_OUT, root } from "../support/env";
import { modZip, prepareGame } from "../support/fixtures";
import { type ScreenshotMod, screenshotGame, screenshotMods } from "../support/screenshot-data";
import { cleanupGame, createProfile, enableMods, importMods, saveSettings, statusOf } from "../support/seed";
import {
	activateMenuItem,
	clickByText,
	clickGame,
	clickMenuTrigger,
	clickMod,
	pressKeyOn,
	waitForDialog,
	waitForGame,
	waitForMod,
	waitForText,
} from "../support/ui";

/// The README needs a real window, but not the user's library. This spec builds a small demo game under `target/`,
/// imports readable mod names from `support/screenshot-data.ts`, and saves the screenshots the README embeds. It is not
/// part of the normal suite.
const GAME_ID = screenshotGame.id;
const GAME_NAME = screenshotGame.name;
const OUT = path.join(root, "target/readme-screenshots");

/// A demo archive with the readable name and version an `info.json` supplies, so the screenshot does not show the
/// archive's file name.
function demoModZip(mod: ScreenshotMod): string {
	return modZip(mod.archive, mod.folder, {
		"info.json": JSON.stringify({ modName: mod.name, version: mod.version }),
		[`content/${mod.folder}.ws`]: "x",
		...mod.files,
	});
}

describe("README screenshots", () => {
	it("captures the mod list, the apply preview and the conflict explorer", async () => {
		mkdirSync(OUT, { recursive: true });
		const fixture = prepareGame({
			id: GAME_ID,
			name: GAME_NAME,
			verified: true,
		});
		copyFileSync(path.join(root, screenshotGame.icon), path.join(definitionsDir, GAME_ID, "icon.webp"));
		await invoke("add_game", { game: GAME_ID, gameRoot: fixture.root });
		await saveSettings(GAME_ID, {
			settings_dir: "",
			mods_root: "",
			launch_env: `${GOBBO_E2E_OUT}=${fixture.out}`,
			launch_args: "",
			profile_data: [],
		});
		await reloadWindow();
		await waitForGame(GAME_NAME);
		await clickGame(GAME_NAME);

		const archives = screenshotMods.map((mod) => demoModZip(mod));
		await importMods(GAME_ID, archives);
		const imported = await statusOf(GAME_ID);
		await enableMods(
			GAME_ID,
			imported.mods.slice(0, 7).map((mod) => mod.folder),
		);

		const before = await statusOf(GAME_ID);
		const active = before.profiles.find((profile) => profile.active) ?? before.profiles[0];

		if (active && active.name !== "Main") {
			await invoke("rename_profile", {
				game: GAME_ID,
				id: active.id,
				name: "Main",
			});
		}

		await createProfile(GAME_ID, "Testing", null);

		await reloadWindow();
		await waitForGame(GAME_NAME);
		await clickGame(GAME_NAME);
		// e2e-locale-exception: mod names from the screenshot fixture, not interface prose.
		await waitForMod("Community Patch");
		// e2e-locale-exception: mod names from the screenshot fixture, not interface prose.
		await clickMod("HD Reworked Project");
		await waitForText("Overview");
		// The automatic width clips the last inspector tab in a 1440 px window; give the panel a little more room.
		await pressKeyOn("[role='separator'][aria-label='Resize the details panel']", "ArrowLeft", { shift: true });
		await pressKeyOn("[role='separator'][aria-label='Resize the details panel']", "ArrowLeft", { shift: true });
		await browser.pause(300);
		await browser.saveScreenshot(path.join(OUT, "mods.png"));

		// The primary action opens the preview; the dialog title is "Apply the mods" or "Apply the mods and start the game".
		await clickByText("Apply", "button");
		await waitForDialog("Apply the mods");
		await browser.pause(500);
		await browser.saveScreenshot(path.join(OUT, "apply-preview.png"));
		await browser.keys("Escape");

		// The game header menu has the conflict explorer.
		await clickMenuTrigger("Launch menu");
		await activateMenuItem("File conflicts");
		await waitForDialog("File conflicts");
		await browser.pause(500);
		await browser.saveScreenshot(path.join(OUT, "conflicts.png"));
		await browser.keys("Escape");

		const status = await statusOf(GAME_ID);
		assert.ok(status.conflict_count > 0, "the demo game must have a shared-file conflict");

		await cleanupGame(GAME_ID);
	});
});
