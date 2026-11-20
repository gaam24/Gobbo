import { strict as assert } from "node:assert";
import path from "node:path";
import type { SnapshotView, Status_Serialize as Status } from "../../src/bindings";
import { CommandFailure, invoke, invokeError, invokeWithChannel } from "../support/bridge";
import { definitionsDir } from "../support/env";
import { removeTree } from "../support/files";
import { type GameOptions, prepareGame } from "../support/fixtures";
import { cleanupGame, statusOf, uniqueId } from "../support/seed";

/// A game added and configured through the commands only. The contract tests do not need a rendered row, so this keeps
/// each one fast and independent of the UI.
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

async function dispose(id: string, root: string): Promise<void> {
	await cleanupGame(id);
	removeTree(path.join(definitionsDir, id));
	removeTree(root);
}

describe("command contracts", () => {
	it("answers a stable code for a game that does not exist", async () => {
		const status = await invokeError("get_status", { game: "e2e-missing" });
		assert.equal(status.code, "io.failed");
		assert.ok(status.params.path?.includes("e2e-missing"), `the path is missing: ${JSON.stringify(status.params)}`);
		// A game whose definition is missing fails while reading that definition, before any game logic runs.
		for (const [command, args] of [
			["add_game", { game: "e2e-missing", gameRoot: "/tmp" }],
			["check_game_folder", { game: "e2e-missing", gameRoot: "/tmp" }],
			["remove_game", { game: "e2e-missing" }],
			["preview_deployment", { game: "e2e-missing" }],
			["undo", { game: "e2e-missing" }],
		] as const) {
			const error = await invokeError(command, args);
			assert.equal(error.code, "io.failed", `${command} answered ${error.code}`);
		}

		assert.equal((await invokeError("export_definition", { game: "e2e-missing", path: "/tmp/never.zip" })).code, "games.unknown");
		assert.deepEqual(await invoke("snapshots", { game: "e2e-missing" }), []);

		try {
			await invokeWithChannel("apply", { game: "e2e-missing" });
			throw new Error("apply was expected to fail");
		} catch (error) {
			assert.ok(error instanceof CommandFailure);
			assert.equal(error.code, "io.failed");
		}
	});
	it("names the mod and the profile state that a stale list points at", async () => {
		const id = uniqueId("contract-stale");
		const game = await directGame({ id, name: `E2E Contract ${id}` });
		const active = game.status.profiles.find((profile) => profile.active);
		assert.ok(active);
		assert.equal((await invokeError("rename_profile", { game: id, id: "p99", name: "Nope" })).code, "profiles.not_found");
		assert.equal((await invokeError("switch_profile", { game: id, id: "p99" })).code, "profiles.not_found");
		assert.equal((await invokeError("delete_profile", { game: id, id: active.id })).code, "profiles.last_profile");
		assert.equal(
			(await invokeError("create_profile", { game: id, name: "x".repeat(200), copyFrom: null, copyData: false })).code,
			"profiles.name_too_long",
		);
		assert.equal((await invokeError("set_mod_tags", { game: id, id: "local:nope", tags: [] })).code, "library.mod_missing");
		assert.equal((await invokeError("set_mod_priority", { game: id, id: "local:nope", priority: 1 })).code, "profile.priority_unsupported");
		assert.equal((await invokeError("set_mod_priority", { game: id, id: "local:nope", priority: null })).code, "library.mod_missing");
		assert.equal((await invokeError("delete_mod", { game: id, id: "local:nope" })).code, "library.mod_missing");
		assert.equal((await invokeError("set_mod_version", { game: id, id: "local:nope", folder: "nope" })).code, "library.version_unknown");
		assert.equal((await invokeError("delete_mod_version", { game: id, folder: "nope", replacement: null })).code, "library.mod_missing");

		try {
			await invokeWithChannel("undo", { game: id });
			throw new Error("undo was expected to fail");
		} catch (error) {
			assert.ok(error instanceof CommandFailure);
			assert.equal(error.code, "deploy.nothing_to_undo");
		}

		assert.equal((await invokeError("preview_restore", { game: id, id: 999 })).code, "history.unknown");
		assert.equal((await invokeError("restore_list", { game: id, id: 999 })).code, "history.unknown");
		await dispose(id, game.root);
	});
	it("sends the recovery action with a missing game or settings folder", async () => {
		const id = uniqueId("contract-folders");
		const game = await directGame({ id, name: `E2E Folders ${id}`, loadOrder: true });
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

		await dispose(id, game.root);
	});
	it("refuses an unknown profile data set, an unknown tool and an unsafe path", async () => {
		const id = uniqueId("contract-tools");
		const game = await directGame({
			id,
			name: `E2E Tools ${id}`,
			userData: true,
			tools: true,
		});
		const active = game.status.profiles.find((profile) => profile.active);
		assert.ok(active);
		assert.equal(
			(await invokeError("set_profile_user_data", { game: id, profile: active.id, set: "nope", isolated: true })).code,
			"user_data.unknown_set",
		);
		assert.equal((await invokeError("run_tool", { game: id, tool: "nope" })).code, "tools.unknown");
		assert.equal((await invokeError("install_tool_prerequisites", { game: id, tool: "nope" })).code, "tools.unknown");
		const outside = await invokeError("open_path", { game: id, path: "/etc", reveal: false });
		assert.equal(outside.code, "io.path_not_allowed");
		// An empty profile now creates an empty snapshot instead of failing with snapshot.nothing.
		const snapshot = await invoke<SnapshotView>("create_snapshot", { game: id, profile: active.id });
		assert.equal(snapshot.profile, active.id);
		assert.equal(snapshot.mods, 0);
		assert.equal(snapshot.files, 0);
		assert.equal(snapshot.overwrite_files, 0);
		const snapshots = await invoke<SnapshotView[]>("snapshots", { game: id });
		assert.ok(
			snapshots.some((item) => item.id === snapshot.id),
			"the empty snapshot was not listed",
		);
		assert.equal((await invokeError("preview_snapshot_restore", { game: id, id: "nope" })).code, "snapshot.not_found");
		assert.equal((await invokeError("delete_snapshot", { game: id, id: "nope" })).code, "snapshot.not_found");
		await dispose(id, game.root);
	});
	it("keeps optional reads optional instead of failing", async () => {
		const id = uniqueId("contract-optional");
		const game = await directGame({ id, name: `E2E Optional ${id}` });
		assert.equal(await invoke("take_pack_link"), null);
		assert.equal(await invoke("last_run", { game: id }), null);
		assert.equal(await invoke("game_running", { game: id }), false);
		assert.deepEqual(await invoke("snapshots", { game: id }), []);
		assert.equal(await invoke("stop_game", { game: id }), false);
		await dispose(id, game.root);
	});
});
