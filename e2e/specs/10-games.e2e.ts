import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import type { ManagedGame } from "../../src/bindings";
import { invoke, invokeError, reloadWindow } from "../support/bridge";
import { dataDir, definitionsDir } from "../support/env";
import { exists, removeTree, writeFile, writeZip } from "../support/files";
import { prepareGame, seedSteamGame } from "../support/fixtures";
import { apply, enableMods, importMods, seedGame, uniqueId } from "../support/seed";
import {
	bodyText,
	clickByLabel,
	clickByText,
	clickInRow,
	clickMenuItem,
	clickMenuTrigger,
	closeTopDialog,
	dialogCount,
	openContextMenuByLabel,
	waitForDialog,
	waitForGame,
	waitForHeading,
	waitForText,
} from "../support/ui";

async function openGames(): Promise<void> {
	await reloadWindow();
	await clickByLabel("Add game");
	await waitForHeading("Games");
}

describe("games", () => {
	it("adds a user game through the Locate dialog and opens its empty library", async () => {
		const id = uniqueId("games-locate");
		const name = `E2E Locate ${id}`;
		const fixture = prepareGame({ id, name });
		await openGames();
		await waitForText(name);
		await clickInRow(name, "Locate");
		await waitForDialog("Locate");
		const input = await browser.$("#locate-folder");
		await input.setValue(fixture.root);
		await waitForText("Found bin/game.exe. This is the game folder.");
		await clickByText("Add game", "button", "[role='dialog']");
		// The first game opens the setup check; the dialog is not the assertion of this test.
		if ((await dialogCount()) > 0) await closeTopDialog();
		await waitForText(name);
		await waitForText(`No mods for ${name} yet`);
		const managed = await invoke<ManagedGame[]>("managed_games");
		assert.ok(
			managed.some((game) => game.id === id),
			"the game was not added",
		);
		assert.ok(exists(path.join(dataDir, "games", id, "config.json")), "the game data folder was not created");
		await invoke("remove_game", { game: id });
		await invoke("delete_removed_game", { game: id });
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("finds a game in a fake Steam library and adds it from the catalog", async () => {
		const id = uniqueId("games-steam");
		const name = `E2E Steam ${id}`;
		const steam = seedSteamGame(987_654, "E2E Steam Game");
		const fixture = prepareGame({
			id,
			name,
			root: steam.gameRoot,
			steamAppId: steam.appId,
			steamInstallDir: "E2E Steam Game",
		});
		await openGames();
		await waitForText("Found on this computer");
		await waitForText("E2E Steam Game");
		await clickInRow(name, "Add");

		if ((await dialogCount()) > 0) await closeTopDialog();
		await waitForText(name);
		const managed = await invoke<ManagedGame[]>("managed_games");
		assert.ok(
			managed.some((game) => game.id === id),
			"the detected game was not added",
		);
		await invoke("remove_game", { game: id });
		await invoke("delete_removed_game", { game: id });
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("removes a game to Storage, restores it and deletes its data", async () => {
		const id = uniqueId("games-storage");
		const name = `E2E Storage ${id}`;
		const { fixture } = await seedGame({ id, name });
		await openContextMenuByLabel(name);
		await clickByText("Remove from Gobbo Mods Organizer", "[role='menuitem']");
		await waitForDialog(`Remove ${name} from Gobbo Mods Organizer?`);
		await clickByText("Remove", "button", "[role='alertdialog']");
		await waitForHeading("Games");
		assert.ok(!exists(path.join(dataDir, "games", id)), "the active data folder was not moved");
		assert.ok(exists(path.join(dataDir, "games", `${id}.removed`)), "the removed data folder is missing");
		await clickByLabel("Storage");
		await waitForHeading("Storage");
		await waitForText(name);
		await waitForText("Removed");
		await clickInRow(name, "Restore");
		await waitForText(name);
		assert.ok(exists(path.join(dataDir, "games", id)), "the restored data folder is missing");
		// The remove flow above already exercised the menu and the confirmation. This second removal is setup for the
		// delete action, so it uses the command directly and reloads instead of repeating the menu dance.
		await invoke("remove_game", { game: id });
		await reloadWindow();
		await waitForHeading("Games");
		await clickByLabel("Storage");
		await waitForHeading("Storage");
		await waitForText(name);
		// The delete action now lives in the row's More actions dropdown.
		await clickMenuTrigger("More actions");
		await clickMenuItem("Delete data");
		await waitForText(`Delete the data of ${name}?`);
		await clickByText("Delete data", "button", "[role='alertdialog']");
		await waitForText("No game data yet");
		assert.ok(!exists(path.join(dataDir, "games", `${id}.removed`)), "the removed data folder was not deleted");
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("reports a user definition that cannot be read and keeps the other games", async () => {
		const id = uniqueId("games-broken");
		const file = path.join(definitionsDir, id, "game.toml");
		writeFile(file, "schema = 1\nid = [\n");
		await openGames();
		await waitForText("Some user definitions could not be loaded");
		await waitForText(file);
		await waitForText("Games");
		removeTree(path.join(definitionsDir, id));
		await openGames();
		await waitForText("Games");
		assert.ok(!(await bodyText()).includes("Some user definitions could not be loaded"), "the broken definition stayed in the catalog");
	});
	it("shows Verified and Unverified badges on user definitions", async () => {
		const verifiedId = uniqueId("games-verified");
		const unverifiedId = uniqueId("games-unverified");
		const verified = prepareGame({
			id: verifiedId,
			name: `E2E Verified ${verifiedId}`,
			verified: true,
		});
		const unverified = prepareGame({
			id: unverifiedId,
			name: `E2E Unverified ${unverifiedId}`,
		});
		await openGames();
		await waitForText(`E2E Verified ${verifiedId}`);
		await waitForText("Verified");
		await waitForText(`E2E Unverified ${unverifiedId}`);
		await waitForText("Unverified");
		removeTree(path.join(definitionsDir, verifiedId));
		removeTree(path.join(definitionsDir, unverifiedId));
		removeTree(verified.root);
		removeTree(unverified.root);
	});
	it("refuses to remove a game whose mods are applied", async () => {
		const id = uniqueId("games-deployed");
		const name = `E2E Deployed ${id}`;
		const { fixture } = await seedGame({ id, name });
		const zip = path.join(dataDir, "fixtures", "games-deployed.zip");
		writeZip(zip, { "Mods/Deployed/content/x.ws": "x" });
		await importMods(id, [zip]);
		await enableMods(id);
		await apply(id);
		const error = await invokeError("remove_game", { game: id });
		assert.equal(error.code, "deploy.active");
		await invoke("undo", { game: id });
		await invoke("remove_game", { game: id });
		await invoke("delete_removed_game", { game: id });
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("opens a game with a missing settings folder in the setup state", async () => {
		const id = uniqueId("games-setup");
		const name = `E2E Setup ${id}`;
		const fixture = prepareGame({ id, name, loadOrder: true });
		await invoke<ManagedGame[]>("add_game", {
			game: id,
			gameRoot: fixture.root,
		});
		await reloadWindow();
		await waitForGame(name);
		await clickByLabel(name);
		await waitForText("Set the settings folder");
		await invoke("remove_game", { game: id });
		await invoke("delete_removed_game", { game: id });
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
});
