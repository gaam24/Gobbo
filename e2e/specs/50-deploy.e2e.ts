import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import type { ConflictExplorer, DeployPreview } from "../../src/bindings";
import { CommandFailure, invoke, invokeWithChannel } from "../support/bridge";
import { dataDir, definitionsDir } from "../support/env";
import { exists, expectFile, expectNoFile, expectText, readText, removeTree, writeFile } from "../support/files";
import { type GameOptions, modZip } from "../support/fixtures";
import { apply, cleanupGame, enableMods, importMods, seedGame, statusOf, uniqueId, waitForStage, waitForStatus } from "../support/seed";

interface DeployGame {
	id: string;
	name: string;
	fixture: { root: string; out: string };
}

async function deployGame(prefix: string, options: Partial<GameOptions> = {}): Promise<DeployGame> {
	const id = uniqueId(prefix);
	const name = `E2E Deploy ${id}`;
	const { fixture } = await seedGame({ ...options, id, name });
	const archive = modZip(`deploy-${id}.zip`, `Deploy ${id}`);
	await importMods(id, [archive]);
	await enableMods(id);
	return { id, name, fixture };
}

async function cleanup(game: DeployGame): Promise<void> {
	await cleanupGame(game.id);
	removeTree(path.join(definitionsDir, game.id));
	removeTree(game.fixture.root);
}

/// A file of a mod in the slot, as the game sees it: `Mods/mod<folder>/<relative>`.
function slotPath(root: string, folder: string, relative: string): string {
	return path.join(root, "Mods", `mod${folder}`, relative);
}

/// The same path inside the active profile's overlay layer.
function layerPath(game: string, profile: string, relative: string): string {
	return path.join(dataDir, "games", game, "state", profile, "mods", relative);
}

/// The active profile's id. A deployment always belongs to it.
async function activeProfile(game: string): Promise<string> {
	const status = await statusOf(game);
	const profile = status.profiles.find((entry) => entry.active);
	assert.ok(profile, "the status has no active profile");
	return profile.id;
}

describe("deploy", () => {
	it("applies the overlay and undoes it without touching the game folder", async () => {
		const game = await deployGame("deploy-overlay");
		const status = await statusOf(game.id);
		const mod = status.mods[0];
		assert.ok(mod);
		const gameFile = slotPath(game.fixture.root, mod.folder, "content/x.ws");
		expectNoFile(gameFile);
		const preview = await invoke<DeployPreview>("preview_deployment", { game: game.id });
		assert.ok(preview.files >= 1, "the preview has no files");
		await apply(game.id);
		await waitForStage(game.id, "applied");

		// The mount lives in the app's namespace, so `stat` from this process cannot see it. The layer is a real folder
		// in the profile state, and it is what the game sees on top of its own folder.
		const staged = layerPath(game.id, await activeProfile(game.id), path.join("Mods", `mod${mod.folder}`, "content/x.ws"));
		expectText(staged, "x");
		expectNoFile(gameFile);

		await invoke("undo", { game: game.id });
		await waitForStage(game.id, "ready");
		// The layer stays for the next apply of this profile; the game folder was never written.
		expectFile(staged);
		expectNoFile(gameFile);
		await cleanup(game);
	});
	it("applies the overlay and launches the game inside it when the platform supports it", async () => {
		const game = await deployGame("deploy-launch");
		const status = await statusOf(game.id);
		assert.ok(status.mods[0]);
		try {
			await apply(game.id);
		} catch (error) {
			assert.ok(error instanceof CommandFailure, "the overlay failed with an unexpected error");
			// A container without user namespaces cannot mount overlayfs. The failure is the documented contract then.
			assert.ok(
				["deploy.overlay_unavailable", "deploy.overlay_unsupported", "deploy.overlay_hook_missing"].includes(error.code),
				`the overlay failed with ${error.code}`,
			);
			await cleanup(game);
			return;
		}
		await waitForStage(game.id, "applied");
		// The mount lives in the app's namespace, so `stat` from this process cannot see it. The game started through
		// Gobbo can: its script copies the mod file it finds in the merged game folder.
		const started = await invoke<boolean>("launch_game", { game: game.id, executable: null, applyDraft: false });
		assert.equal(started, true, "the overlay launch did not report a game with mods");
		const seen = path.join(game.fixture.out, "mod.ws");
		await browser.waitUntil(() => exists(seen), {
			timeout: 60_000,
			timeoutMsg: "the game inside the overlay did not see the mod file",
		});
		assert.equal(readText(seen), "x");
		await browser.waitUntil(async () => !(await invoke<boolean>("game_running", { game: game.id })), {
			timeout: 30_000,
			timeoutMsg: "the overlay game did not exit",
		});
		await invoke("undo", { game: game.id });
		await waitForStage(game.id, "ready");
		await cleanup(game);
	});
	it("lets the user choose the winner of a shared file and applies it", async () => {
		const id = uniqueId("deploy-conflict");
		const name = `E2E Conflict ${id}`;
		const { fixture } = await seedGame({ id, name });
		const first = modZip(`conflict-${id}-one.zip`, "One", { "content/shared.ws": "one" });
		const second = modZip(`conflict-${id}-two.zip`, "Two", { "content/shared.ws": "two" });
		await importMods(id, [first, second]);
		await enableMods(id);
		const before = await statusOf(id);
		assert.equal(before.conflict_count, 1);
		const explorer = await invoke<ConflictExplorer>("conflict_explorer", { game: id });
		assert.equal(explorer.total, 1);
		const entry = explorer.entries[0];
		assert.ok(entry);
		await apply(id);
		const profile = await activeProfile(id);
		const winner = before.mods.find((mod) => mod.id === entry.winner);
		assert.ok(winner, "the conflict winner is not in the status");
		const deployed = readText(layerPath(id, profile, path.join("Mods", `mod${winner.folder}`, entry.path)));
		assert.ok(["one", "two"].includes(deployed), `unexpected deployed content ${deployed}`);
		const other = entry.losers[0];
		assert.ok(other, "the conflict has no loser");
		await invoke("set_file_winner", { game: id, path: entry.path, family: other });
		await apply(id);
		const loser = before.mods.find((mod) => mod.id === other);
		assert.ok(loser, "the conflict loser is not in the status");
		const changed = readText(layerPath(id, profile, path.join("Mods", `mod${loser.folder}`, entry.path)));
		assert.notEqual(changed, deployed, "the winner change did not reach the game");
		await invoke("undo", { game: id });
		await cleanup({
			id,
			name,
			fixture,
		});
	});
	it("writes the game's load order file from the list and the priority", async () => {
		const game = await deployGame("deploy-order", { loadOrder: true });
		const status = await waitForStatus(game.id, (current) => current.mods.length === 1);
		const mod = status.mods[0];
		assert.ok(mod);
		await invoke("set_mod_priority", { game: game.id, id: mod.id, priority: 5 });
		await apply(game.id);
		const settings = path.join(game.fixture.root, "settings/mods.settings");
		expectText(settings, `[mod${mod.folder}]`);
		expectText(settings, "Enabled=1");
		expectText(settings, "Priority=5");
		await invoke("undo", { game: game.id });
		await cleanup(game);
	});
	it("covers a file the game already has and leaves it untouched", async () => {
		const id = uniqueId("deploy-foreign");
		const name = `E2E Foreign ${id}`;
		const { fixture } = await seedGame({ id, name });
		await importMods(id, [modZip(`deploy-${id}.zip`, "Deploy")]);
		await enableMods(id);
		const status = await statusOf(id);
		const mod = status.mods[0];
		assert.ok(mod);
		const userFile = slotPath(fixture.root, mod.folder, "content/x.ws");
		writeFile(userFile, "user file");
		const preview = await invoke<DeployPreview>("preview_deployment", { game: id });
		assert.equal(preview.problems.length, 0, "the overlay must not block on a game file");
		await invokeWithChannel("apply", { game: id });
		const staged = layerPath(id, await activeProfile(id), path.join("Mods", `mod${mod.folder}`, "content/x.ws"));
		expectText(staged, "x");
		assert.equal(readText(userFile), "user file");
		await cleanup({ id, name, fixture });
	});
});
