import path from "node:path";

import type {
	ApplyView,
	ImportView_Serialize as ImportView,
	ManagedGame,
	Settings_Deserialize as Settings,
	Status_Serialize as Status,
} from "../../src/bindings";
import { invoke, invokeWithChannel, reloadWindow } from "./bridge";
import { dataDir, definitionsDir, outDir, rootsDir } from "./env";
import { removeTree } from "./files";
import { type GameFixture, type GameOptions, prepareGame } from "./fixtures";
import { clickGame, waitForGame, waitForText } from "./ui";

let counter = 0;

/// A game id that is valid for a definition and unique in one run: lower-case ASCII, digits and hyphens.
export function uniqueId(prefix: string): string {
	counter += 1;
	const slug = prefix
		.toLowerCase()
		.replaceAll(/[^a-z0-9]+/g, "-")
		.replaceAll(/^-+|-+$/g, "")
		.slice(0, 36);
	return `e2e-${slug || "game"}-${counter}`;
}

export interface SeededGame {
	fixture: GameFixture;
	status: Status;
}

/// Creates a definition, adds the game through the real command, reloads the window and selects the game in the rail.
/// The reload is what makes the frontend see a game the bridge added; the alternative is duplicating the Games page add
/// flow in every spec.
export async function seedGame(options: GameOptions, settings: Partial<Settings> = {}): Promise<SeededGame> {
	await cleanupGame(options.id);
	const fixture = prepareGame(options);
	await invoke<ManagedGame[]>("add_game", {
		game: options.id,
		gameRoot: fixture.root,
	});
	const status = await saveSettings(options.id, {
		settings_dir: options.loadOrder ? path.join(fixture.root, "settings") : "",
		mods_root: "",
		launch_env: `GOBBO_E2E_OUT=${fixture.out}`,
		launch_args: "",
		profile_data: [],
		...settings,
	});
	await reloadWindow();
	await waitForGame(options.name);
	await clickGame(options.name);
	await waitForText(options.name);
	return { fixture, status };
}

/// Saves a full settings object: the command replaces the file, so a spec that changes one field keeps the rest.
export async function saveSettings(game: string, patch: Partial<Settings>): Promise<Status> {
	const current = await statusOf(game);
	const settings: Settings = { ...current.settings, ...patch };
	return invoke<Status>("save_settings", { game, settings });
}

export function statusOf(game: string): Promise<Status> {
	return invoke<Status>("get_status", { game });
}

export function syncProfiles(game: string): Promise<Status> {
	return invoke<Status>("sync_profiles", { game });
}

export function rowsFor(status: Status, enabled: (mod: Status["mods"][number], index: number) => boolean = () => true) {
	return status.mods.map((mod, index) => ({
		folder: mod.folder,
		enabled: enabled(mod, index),
		separator: null,
	}));
}

/// Imports one or more archives through the same command the import button uses. `source` links the archive to a
/// provider without a network request; a null source is a loose archive.
export async function importMods(
	game: string,
	paths: string[],
	source: {
		provider: string;
		mod_id: string;
		part_id?: string | null;
	} | null = null,
): Promise<ImportView[]> {
	const imported: ImportView[] = [];

	for (const archive of paths) {
		const { value } = await invokeWithChannel<ImportView>("import_zip", {
			game,
			path: archive,
			choices: null,
			source,
		});
		imported.push(value);
	}

	await syncProfiles(game);
	return imported;
}

/// Enables the named folders (or every imported folder) and saves the list in one change.
export async function enableMods(game: string, folders?: string[]): Promise<Status> {
	const status = await statusOf(game);
	const wanted = folders ? new Set(folders) : null;
	await invoke<Status>("save_profile", {
		game,
		rows: rowsFor(status, (mod) => (wanted ? wanted.has(mod.folder) : true)),
		known: null,
	});
	return statusOf(game);
}

export async function apply(game: string): Promise<ApplyView> {
	const { value } = await invokeWithChannel<ApplyView>("apply", { game });
	return value;
}

export function createProfile(game: string, name: string, copyFrom: string | null = null): Promise<Status> {
	// The command now requires the explicit copy-data flag. The suite creates empty profiles, as it did before the flag
	// was added, so copying a source profile's data stays off.
	return invoke<Status>("create_profile", {
		game,
		name,
		copyFrom,
		copyData: false,
	});
}

export function switchProfile(game: string, id: string): Promise<Status> {
	return invoke<Status>("switch_profile", { game, id });
}

export async function waitForStage(game: string, stage: string, timeout = 30_000): Promise<Status> {
	const deadline = Date.now() + timeout;
	let current = await statusOf(game);

	while (current.stage.kind !== stage) {
		if (Date.now() > deadline) throw new Error(`the game did not reach ${stage}; it is ${current.stage.kind}`);
		await new Promise<void>((resolve) => {
			setTimeout(resolve, 100);
		});
		current = await statusOf(game);
	}

	return current;
}

/// Waits for a status answer that satisfies a predicate. A UI action returns before its command has saved, so a spec
/// that reads the status right after a click would race the queue.
export async function waitForStatus(game: string, predicate: (status: Status) => boolean, timeout = 30_000): Promise<Status> {
	const deadline = Date.now() + timeout;
	let current = await statusOf(game);

	while (!predicate(current)) {
		if (Date.now() > deadline) throw new Error(`the status of ${game} did not reach the expected state`);
		await new Promise<void>((resolve) => {
			setTimeout(resolve, 100);
		});
		current = await statusOf(game);
	}

	return current;
}

/// Removes a game and everything the suite created for it. Every step is best effort: a test that fails halfway must not
/// hide the next failure behind a cleanup error.
export async function cleanupGame(game: string): Promise<void> {
	try {
		await invoke("stop_game", { game });
	} catch {
		// A game that is not running is the normal case.
	}

	try {
		await invoke("undo", { game });
	} catch {
		// Nothing to undo is the normal case.
	}

	try {
		await invoke<ManagedGame[]>("remove_game", { game });
	} catch {
		// A game that is still deployed refuses removal; the next cleanup of the same id will try again.
	}

	try {
		await invoke("delete_removed_game", { game });
	} catch {
		// A game that was not removed has no removed folder.
	}

	removeTree(path.join(definitionsDir, game));
	removeTree(path.join(rootsDir, game));
	removeTree(path.join(dataDir, "e2e-user", game));
	removeTree(path.join(dataDir, "e2e-profile", game));
	removeTree(path.join(outDir, game));
}
