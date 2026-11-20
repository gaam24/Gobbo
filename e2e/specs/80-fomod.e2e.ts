import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import type { ModExtras_Serialize as ModExtras } from "../../src/bindings";
import { CommandFailure, invoke, invokeWithChannel, reloadWindow } from "../support/bridge";
import { dataDir, definitionsDir } from "../support/env";
import { expectFile, expectNoFile, removeTree } from "../support/files";
import { fomodZip } from "../support/fixtures";
import { cleanupGame, seedGame, statusOf, uniqueId, waitForStatus } from "../support/seed";
import { clickByText, clickMenuItem, openModMenu, waitForDialog, waitForMod, waitForNoSelector, waitForText } from "../support/ui";

interface FomodGame {
	id: string;
	name: string;
	archive: string;
	fixture: { root: string };
}

function game(prefix: string): FomodGame {
	const id = uniqueId(prefix);
	return {
		id,
		name: `E2E Fomod ${id}`,
		archive: fomodZip(`fomod-${id}.zip`),
		fixture: { root: "" },
	};
}

async function start(prefix: string): Promise<FomodGame> {
	const prepared = game(prefix);
	const { fixture } = await seedGame({ id: prepared.id, name: prepared.name });
	return { ...prepared, fixture };
}

async function cleanup(prepared: FomodGame): Promise<void> {
	await cleanupGame(prepared.id);
	removeTree(path.join(definitionsDir, prepared.id));
	removeTree(prepared.fixture.root);
}

async function importWithChoices(prepared: FomodGame, option: string): Promise<void> {
	await invokeWithChannel("import_zip", {
		game: prepared.id,
		path: prepared.archive,
		choices: [{ step: 0, group_index: 0, group: "Quality", option }],
		source: null,
	});
	await invoke("sync_profiles", { game: prepared.id });
}

async function extrasOf(prepared: FomodGame, folder: string): Promise<ModExtras | undefined> {
	const extras = await invoke<ModExtras[]>("mod_extras", { game: prepared.id, folders: [folder] });
	return extras[0];
}

async function waitForInstalledOption(prepared: FomodGame, folder: string, option: string): Promise<ModExtras> {
	const deadline = Date.now() + 30_000;
	let extras = await extrasOf(prepared, folder);

	while (extras?.fomod[0]?.option !== option) {
		if (Date.now() > deadline) {
			throw new Error(`the installer answer did not become ${option}: ${JSON.stringify(extras?.fomod)}`);
		}

		await new Promise<void>((resolve) => {
			setTimeout(resolve, 100);
		});
		extras = await extrasOf(prepared, folder);
	}

	return extras;
}

describe("fomod installer", () => {
	it("opens the wizard from the Import button, starts with a valid group and installs the chosen option", async () => {
		const prepared = await start("fomod-ui");
		await browser.execute((file: string) => {
			window.__GOBBO_E2E_DIALOGS__ = { files: [file] };
		}, prepared.archive);
		// The toolbar's labelled Import button was replaced by the Import button in the empty library.
		await clickByText("Import archive", "button");
		await waitForDialog("E2E FOMOD");
		await waitForText("Step 1 of 1");
		await waitForText("Quality");
		// A SelectExactlyOne group with no recommended option now starts with its first usable option picked, so the
		// wizard is valid before the user clicks anything. The group rule is enforced by the default, not by an error.
		const defaultChecked = await browser.execute(() =>
			[...document.querySelectorAll<HTMLElement>("[role='radio']")]
				.find((element) => (element.textContent ?? "").includes("Low"))
				?.getAttribute("aria-checked"),
		);
		assert.equal(defaultChecked, "true", "the exactly-one group did not start with a pick");
		await clickByText("High", "[role='radio']", "[role='dialog']");
		// The pick is answered by a command; install only after the wizard shows the new answer.
		await browser.waitUntil(
			() =>
				browser.execute(
					() =>
						[...document.querySelectorAll<HTMLElement>("[role='radio']")]
							.find((element) => (element.textContent ?? "").includes("High"))
							?.getAttribute("aria-checked") === "true",
				),
			{ timeout: 30_000, timeoutMsg: "the High option did not stay selected" },
		);
		await clickByText("Install selected", "button", "[role='dialog']");
		await waitForNoSelector("[role='dialog']");
		// The import starts after the dialog closes, so wait for the library instead of reading the status once.
		const status = await waitForStatus(prepared.id, (current) => current.mods.length > 0);
		const mod = status.mods[0];
		assert.ok(mod, "the FOMOD import added no mod");
		await waitForMod(mod.folder);
		const library = path.join(dataDir, "games", prepared.id, "library", mod.folder);
		// The installer's `<folder destination="content/textures">` is now applied, so the chosen file lands at the game
		// path it declares, not at the path it had inside the archive.
		expectFile(path.join(library, "content/textures/high.ws"));
		expectNoFile(path.join(library, "content/textures/low.ws"));
		expectNoFile(path.join(library, "Mods/Fomod/content/base.ws"));
		const extras = await extrasOf(prepared, mod.folder);
		assert.equal(extras?.fomod_installer, true);
		assert.deepEqual(
			extras?.fomod.map((choice) => choice.option),
			["High"],
		);
		await cleanup(prepared);
	});
	it("asks before closing a changed wizard and skips the archive", async () => {
		const prepared = await start("fomod-skip");
		await browser.execute((file: string) => {
			window.__GOBBO_E2E_DIALOGS__ = { files: [file] };
		}, prepared.archive);
		// The toolbar's labelled Import button was replaced by the Import button in the empty library.
		await clickByText("Import archive", "button");
		await waitForDialog("E2E FOMOD");
		await clickByText("High", "[role='radio']", "[role='dialog']");
		await clickByText("Skip archive", "button", "[role='dialog']");
		await waitForText("Close without installing?");
		await clickByText("Skip archive", "button", "[role='alert']");
		await waitForNoSelector("[role='dialog']");
		const status = await statusOf(prepared.id);
		assert.equal(status.mods.length, 0, "the skipped archive was imported");
		await cleanup(prepared);
	});
	it("refuses a choice that the installer does not define", async () => {
		const prepared = await start("fomod-bad-choice");

		try {
			await invokeWithChannel("import_zip", {
				game: prepared.id,
				path: prepared.archive,
				choices: [{ step: 0, group_index: 0, group: "Quality", option: "Nope" }],
				source: null,
			});
			throw new Error("the unknown choice was expected to fail");
		} catch (error) {
			assert.ok(error instanceof CommandFailure, "the import failed with an unexpected error");
			assert.equal(error.code, "fomod.unknown_option");
		}

		assert.equal((await statusOf(prepared.id)).mods.length, 0);
		await cleanup(prepared);
	});
	it("changes the installer options of an installed mod and replaces its files", async () => {
		const prepared = await start("fomod-reinstall");
		await importWithChoices(prepared, "High");
		await reloadWindow();
		const status = await statusOf(prepared.id);
		const mod = status.mods[0];
		assert.ok(mod, "the FOMOD import added no mod");
		await waitForMod(mod.folder);
		const library = path.join(dataDir, "games", prepared.id, "library", mod.folder);
		expectFile(path.join(library, "content/textures/high.ws"));
		await openModMenu(mod.folder);
		await clickMenuItem("Change installer options");
		await waitForDialog("E2E FOMOD");
		const highChecked = await browser.execute(() =>
			[...document.querySelectorAll<HTMLElement>("[role='radio']")]
				.find((element) => (element.textContent ?? "").includes("High"))
				?.getAttribute("aria-checked"),
		);
		assert.equal(highChecked, "true", "the saved answer did not start checked");
		await clickByText("Low", "[role='radio']", "[role='dialog']");
		await clickByText("Install selected", "button", "[role='dialog']");
		await waitForNoSelector("[role='dialog']");
		await waitForInstalledOption(prepared, mod.folder, "Low");
		expectFile(path.join(library, "content/textures/low.ws"));
		expectNoFile(path.join(library, "content/textures/high.ws"));
		await cleanup(prepared);
	});
});
