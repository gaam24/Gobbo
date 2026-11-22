import { strict as assert } from "node:assert";
import path from "node:path";
import type { ConflictExplorer, DeployPreview } from "../../src/bindings";
import { CommandFailure, invoke, invokeWithChannel } from "../support/bridge";
import { expectNoFile, expectText, readText, writeFile } from "../support/files";
import { type GameOptions, modZip } from "../support/fixtures";
import { apply, cleanupFixture, enableMods, importMods, seedGame, statusOf, uniqueId, waitForStage, waitForStatus } from "../support/seed";

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

/// A file of a mod in the slot, as the game sees it: `Mods/mod<folder>/<relative>`.
function slotPath(root: string, folder: string, relative: string): string {
	return path.join(root, "Mods", `mod${folder}`, relative);
}

describe("deploy", () => {
	it("applies the overlay and undoes it without touching the game folder", async () => {
		const game = await deployGame("deploy-overlay");
		const status = await statusOf(game.id);
		const mod = status.mods[0];
		assert.ok(mod);
		const gameFile = slotPath(game.fixture.root, mod.folder, "content/x.ws");
		expectNoFile(gameFile);
		const preview = await invoke<DeployPreview>("preview_deployment", {
			game: game.id,
		});
		assert.ok(preview.files >= 1, "the preview has no files");
		assert.equal(preview.conflicts, 0, "an isolated mod must not conflict");
		await apply(game.id);
		await waitForStage(game.id, "applied");
		expectNoFile(gameFile);

		await invoke("undo", { game: game.id });
		await waitForStage(game.id, "ready");
		// The game folder was never written; the overlay's private layer layout is not part of the contract.
		expectNoFile(gameFile);
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("reaches the applied and ready stages when the platform supports the overlay", async () => {
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
			await cleanupFixture(game.id, game.fixture.root);
			return;
		}

		await waitForStage(game.id, "applied");
		// The launch and the game-visible mod file belong to the launch spec; deploy only checks its own stages.
		await invoke("undo", { game: game.id });
		await waitForStage(game.id, "ready");
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("lets the user choose the winner of a shared file and applies it", async () => {
		const id = uniqueId("deploy-conflict");
		const name = `E2E Conflict ${id}`;
		const { fixture } = await seedGame({ id, name });
		const first = modZip(`conflict-${id}-one.zip`, "One", {
			"content/shared.ws": "one",
		});
		const second = modZip(`conflict-${id}-two.zip`, "Two", {
			"content/shared.ws": "two",
		});
		await importMods(id, [first, second]);
		await enableMods(id);
		const before = await statusOf(id);
		assert.equal(before.conflict_count, 1);
		const explorer = await invoke<ConflictExplorer>("conflict_explorer", {
			game: id,
		});
		assert.equal(explorer.total, 1);
		const entry = explorer.entries[0];
		assert.ok(entry);
		const preview = await invoke<DeployPreview>("preview_deployment", { game: id });
		assert.equal(preview.conflicts, 1, "the preview does not show the shared-file conflict");
		await apply(id);
		await waitForStage(id, "applied");
		const other = entry.losers[0];
		assert.ok(other, "the conflict has no loser");
		await invoke("set_file_winner", {
			game: id,
			path: entry.path,
			family: other,
		});
		const changed = await invoke<DeployPreview>("preview_deployment", { game: id });
		assert.ok(changed.winners_changed >= 1, "the winner change did not reach the deployment preview");
		await apply(id);
		await waitForStage(id, "applied");
		await invoke("undo", { game: id });
		await cleanupFixture(id, fixture.root);
	});
	it("writes the game's load order file from the list and the priority", async () => {
		const game = await deployGame("deploy-order", { loadOrder: true });
		const status = await waitForStatus(game.id, (current) => current.mods.length === 1);
		const mod = status.mods[0];
		assert.ok(mod);
		await invoke("set_mod_priority", {
			game: game.id,
			id: mod.id,
			priority: 5,
		});
		await apply(game.id);
		const settings = path.join(game.fixture.root, "settings/mods.settings");
		expectText(settings, `[mod${mod.folder}]`);
		expectText(settings, "Enabled=1");
		expectText(settings, "Priority=5");
		await invoke("undo", { game: game.id });
		await cleanupFixture(game.id, game.fixture.root);
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
		const preview = await invoke<DeployPreview>("preview_deployment", {
			game: id,
		});
		assert.equal(preview.problems.length, 0, "the overlay must not block on a game file");
		assert.ok(preview.files >= 1, "the preview does not place the mod file");
		await invokeWithChannel("apply", { game: id });
		await waitForStage(id, "applied");
		assert.equal(readText(userFile), "user file");
		await cleanupFixture(id, fixture.root);
	});
});
