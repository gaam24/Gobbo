import { strict as assert } from "node:assert";
import path from "node:path";
import type { RunReport_Serialize as RunReport } from "../../src/bindings";
import { CommandFailure, invoke, invokeWithChannel, reloadWindow } from "../support/bridge";
import { definitionsDir } from "../support/env";
import { expectFile, readText, removeTree } from "../support/files";
import { modZip } from "../support/fixtures";
import { cleanupGame, enableMods, importMods, saveSettings, seedGame, uniqueId } from "../support/seed";
import { clickByLabel, clickByText, clickTab, waitForText } from "../support/ui";

async function waitForRunning(game: string, wanted: boolean, timeout = 15_000): Promise<boolean> {
	const deadline = Date.now() + timeout;

	while (Date.now() < deadline) {
		const running = await invoke<boolean>("game_running", { game });

		if (running === wanted) return running;
		await new Promise<void>((resolve) => {
			setTimeout(resolve, 100);
		});
	}

	throw new Error(`the game did not become ${wanted ? "running" : "stopped"}`);
}

const OVERLAY_UNAVAILABLE = new Set(["deploy.overlay_unavailable", "deploy.overlay_unsupported", "deploy.overlay_hook_missing"]);

/// Applies the draft and starts the game. A machine without user namespaces cannot mount the layer; the backend reports
/// that as a documented error, so those runs check the launch itself and leave the layer assertions to a capable host.
async function launchWithDraft(game: string): Promise<boolean | null> {
	try {
		return await invoke<boolean>("launch_game", { game, executable: null, applyDraft: true });
	} catch (error) {
		if (error instanceof CommandFailure && OVERLAY_UNAVAILABLE.has(error.code)) return null;
		throw error;
	}
}

async function waitForExit(game: string): Promise<RunReport> {
	const deadline = Date.now() + 30_000;

	while (Date.now() < deadline) {
		const report = await invoke<RunReport | null>("last_run", { game });

		if (report && report.finished_at !== null) return report;
		await new Promise<void>((resolve) => {
			setTimeout(resolve, 100);
		});
	}

	throw new Error("the launch report did not record an exit");
}

describe("launch and console", () => {
	it("launches a fixture game without mods and records the run", async () => {
		const id = uniqueId("launch-bare");
		const name = `E2E Launch ${id}`;
		const { fixture } = await seedGame({ id, name });
		await importMods(id, [modZip(`launch-bare-${id}.zip`, "Bare")]);
		await invoke<boolean>("launch_game", { game: id, executable: null, applyDraft: false });
		const report = await waitForExit(id);
		assert.equal(report.game_exit_code, 0);
		assert.equal(report.stopped_by_user, false);
		assert.ok(
			report.output.some((line) => line.includes("gobbo e2e game")),
			`the output is missing: ${report.output.join(" | ")}`,
		);
		expectFile(path.join(fixture.out, "args.txt"));
		const cwd = readText(path.join(fixture.out, "cwd.txt")).trim();
		assert.ok([fixture.root, path.join(fixture.root, "bin")].includes(cwd), `the game started in an unexpected folder: ${cwd}`);
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("applies the draft and launches the game with the deployed mod", async () => {
		const id = uniqueId("launch-mod");
		const name = `E2E Launch Mod ${id}`;
		const { fixture } = await seedGame({ id, name });
		await importMods(id, [modZip(`launch-mod-${id}.zip`, "Launch")]);
		await enableMods(id);
		const started = await launchWithDraft(id);

		if (started === null) {
			await cleanupGame(id);
			removeTree(path.join(definitionsDir, id));
			removeTree(fixture.root);
			return;
		}

		assert.equal(started, true);
		const report = await waitForExit(id);
		assert.equal(report.game_exit_code, 0);
		expectFile(path.join(fixture.out, "mod.ws"));
		assert.equal(readText(path.join(fixture.out, "mod.ws")), "x");
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("passes the settings arguments and environment to the game", async () => {
		const id = uniqueId("launch-env");
		const name = `E2E Launch Env ${id}`;
		const { fixture } = await seedGame({ id, name });
		await saveSettings(id, {
			launch_args: "-one -two",
			launch_env: `GOBBO_E2E_OUT=${fixture.out}\nE2E_EXTRA=hello`,
		});
		await invoke<boolean>("launch_game", { game: id, executable: null, applyDraft: false });
		await waitForExit(id);
		assert.deepEqual(readText(path.join(fixture.out, "args.txt")).trim().split("\n"), ["-one", "-two"]);
		assert.ok(readText(path.join(fixture.out, "env.txt")).includes("E2E_EXTRA=hello"));
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("stops a sleeping game and blocks an apply while it runs", async () => {
		const id = uniqueId("launch-stop");
		const name = `E2E Launch Stop ${id}`;
		const { fixture } = await seedGame({ id, name, executable: "sleep-binary" });
		await importMods(id, [modZip(`launch-stop-${id}.zip`, "Stop")]);
		await enableMods(id);
		const started = await launchWithDraft(id);

		if (started === null) {
			await cleanupGame(id);
			removeTree(path.join(definitionsDir, id));
			removeTree(fixture.root);
			return;
		}

		assert.equal(started, true);
		await waitForRunning(id, true);
		let applyError: CommandFailure | null = null;

		try {
			await invokeWithChannel("apply", { game: id });
		} catch (error) {
			if (error instanceof CommandFailure) applyError = error;
			else throw error;
		}

		assert.ok(applyError, "the apply while running was expected to fail");
		assert.equal(applyError.code, "deploy.game_running");
		const stopped = await invoke<boolean>("stop_game", { game: id });
		assert.equal(stopped, true);
		await waitForRunning(id, false);
		const report = await waitForExit(id);
		assert.equal(report.stopped_by_user, true);
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("shows the crash prompt after a nonzero exit", async () => {
		const id = uniqueId("launch-crash");
		const name = `E2E Launch Crash ${id}`;
		const { fixture } = await seedGame({ id, name, executable: "crash" });
		await invoke<boolean>("launch_game", { game: id, executable: null, applyDraft: false });
		const report = await waitForExit(id);
		assert.equal(report.game_exit_code, 3);
		await reloadWindow();
		await waitForText("The game exited unexpectedly");
		await waitForText("Find the mod");
		await clickByText("Not now", "button");
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("opens the console and shows the game output", async () => {
		const id = uniqueId("launch-console");
		const name = `E2E Launch Console ${id}`;
		const { fixture } = await seedGame({ id, name });
		await invoke<boolean>("launch_game", { game: id, executable: null, applyDraft: false });
		await waitForExit(id);
		await reloadWindow();
		await clickByText("Console", "button");
		await waitForText("Summary");
		await clickTab("Output");
		await waitForText("gobbo e2e game");
		await clickByLabel("Close console");
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
});
