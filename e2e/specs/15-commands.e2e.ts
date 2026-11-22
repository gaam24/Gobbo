import { strict as assert } from "node:assert";
import path from "node:path";
import type { Status_Serialize as Status } from "../../src/bindings";
import { CommandFailure, invoke, invokeWithChannel } from "../support/bridge";
import { type GameOptions, prepareGame } from "../support/fixtures";
import { cleanupFixture, statusOf, uniqueId } from "../support/seed";

/// A game added and configured through the commands only. The recovery checks do not need a rendered row, so this
/// keeps each one fast and independent of the UI.
async function directGame(options: GameOptions): Promise<{ root: string; status: Status }> {
	const fixture = prepareGame(options);
	await invoke("add_game", { game: options.id, gameRoot: fixture.root });
	const status = await statusOf(options.id);
	await invoke("save_settings", {
		game: options.id,
		settings: {
			...status.settings,
			settings_dir: options.loadOrder ? path.join(fixture.root, "settings") : "",
		},
	});
	return { root: fixture.root, status: await statusOf(options.id) };
}

describe("command contracts", () => {
	it("sends the recovery action with a missing game or settings folder", async () => {
		const id = uniqueId("contract-folders");
		const game = await directGame({
			id,
			name: `E2E Folders ${id}`,
			loadOrder: true,
		});
		await invoke("save_settings", {
			game: id,
			settings: { ...game.status.settings, game_root: "", settings_dir: "" },
		});

		try {
			await invokeWithChannel("apply", { game: id });
			throw new Error("the apply without a game folder was expected to fail");
		} catch (error) {
			assert.ok(error instanceof CommandFailure);
			assert.equal(error.code, "settings.game_folder_missing");
			assert.equal((error.recovery as { action?: string } | undefined)?.action, "set_game_folder");
		}

		await invoke("save_settings", {
			game: id,
			settings: { ...game.status.settings, settings_dir: "" },
		});

		try {
			await invokeWithChannel("apply", { game: id });
			throw new Error("the apply without a settings folder was expected to fail");
		} catch (error) {
			assert.ok(error instanceof CommandFailure);
			assert.equal(error.code, "settings.settings_folder_missing");
			assert.equal((error.recovery as { action?: string } | undefined)?.action, "set_settings_folder");
		}

		await cleanupFixture(id, game.root);
	});
	it("keeps optional reads optional instead of failing", async () => {
		const id = uniqueId("contract-optional");
		const game = await directGame({ id, name: `E2E Optional ${id}` });
		assert.equal(await invoke("take_pack_link"), null);
		assert.equal(await invoke("last_run", { game: id }), null);
		assert.equal(await invoke("game_running", { game: id }), false);
		assert.deepEqual(await invoke("snapshots", { game: id }), []);
		assert.equal(await invoke("stop_game", { game: id }), false);
		await cleanupFixture(id, game.root);
	});
});
