import { strict as assert } from "node:assert";
import path from "node:path";
import type {
	PackExportView,
	PackImportView_Serialize as PackImportView,
	PackPreview,
	Status_Serialize as Status,
} from "../../src/bindings";
import { CommandFailure, invoke, invokeWithChannel } from "../support/bridge";
import { dataDir } from "../support/env";
import { expectFile, zipNames } from "../support/files";
import { modZip, packPath } from "../support/fixtures";
import { cleanupFixture, enableMods, importMods, seedGame, statusOf, uniqueId } from "../support/seed";

interface PackGame {
	id: string;
	name: string;
	fixture: { root: string; out: string };
}

function packOptions(name: string, profiles: string[], archives: "none" | "unlisted" | "all") {
	return {
		info: {
			id: "",
			name,
			version: "1.0",
			author: "E2E",
			description: "A pack from the suite",
		},
		profiles,
		archives,
	};
}

async function sourceGame(prefix: string): Promise<PackGame> {
	const id = uniqueId(prefix);
	const name = `E2E Pack ${id}`;
	const { fixture } = await seedGame({ id, name });
	await importMods(id, [modZip(`pack-${id}-one.zip`, "Pack One"), modZip(`pack-${id}-two.zip`, "Pack Two")]);
	await enableMods(id);
	return { id, name, fixture };
}

async function activeProfileId(game: string): Promise<string> {
	const status = await statusOf(game);
	const active = status.profiles.find((profile) => profile.active);
	assert.ok(active, "the game has no active profile");
	return active.id;
}

/// A pack is checked against the same game, so the test removes the library copies first. That is the state of a fresh
/// installation receiving the pack.
async function removeLibraryMods(game: string): Promise<void> {
	const status = await statusOf(game);

	for (const mod of status.mods) await invoke("delete_mod", { game, id: mod.id });
	const empty = await statusOf(game);
	assert.equal(empty.mods.length, 0, "the library still has mods");
	await invoke("sync_profiles", { game });
}

describe("mod packs", () => {
	it("exports a pack with its archives and reads them back", async () => {
		const game = await sourceGame("pack-export");
		const profile = await activeProfileId(game.id);
		const file = packPath(`pack-${game.id}`);
		const exported = await invoke<PackExportView>("export_pack", {
			game: game.id,
			path: file,
			options: packOptions("E2E Pack", [profile], "all"),
		});
		assert.equal(exported.mods, 2);
		assert.equal(exported.archives, 2);
		expectFile(file);
		assert.ok(zipNames(file).includes("pack.json"), "the pack has no manifest");
		await removeLibraryMods(game.id);
		const preview = await invoke<PackPreview>("inspect_pack", {
			game: game.id,
			path: file,
		});
		assert.equal(preview.total, 2);
		assert.equal(preview.have, 0);
		assert.equal(preview.bundled, 2);
		assert.equal(preview.missing.length, 0);
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("writes a pack without archives and reports the missing mods", async () => {
		const game = await sourceGame("pack-missing");
		const profile = await activeProfileId(game.id);
		const file = packPath(`pack-missing-${game.id}`);
		const exported = await invoke<PackExportView>("export_pack", {
			game: game.id,
			path: file,
			options: packOptions("E2E Missing Pack", [profile], "none"),
		});
		assert.equal(exported.archives, 0);
		await removeLibraryMods(game.id);
		const preview = await invoke<PackPreview>("inspect_pack", {
			game: game.id,
			path: file,
		});
		assert.equal(preview.total, 2);
		assert.equal(preview.have, 0);
		assert.equal(preview.bundled, 0);
		assert.equal(preview.missing.length, 2);
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("imports a pack into a second game and creates its profile", async () => {
		const source = await sourceGame("pack-source");
		const profile = await activeProfileId(source.id);
		const file = packPath(`pack-import-${source.id}`);
		await invoke<PackExportView>("export_pack", {
			game: source.id,
			path: file,
			options: packOptions("E2E Import Pack", [profile], "all"),
		});
		await removeLibraryMods(source.id);
		const imported = await invoke<PackImportView>("import_pack", {
			game: source.id,
			path: file,
		});
		assert.equal(imported.imported, 2);
		assert.equal(imported.profiles_created, 1);
		assert.equal(imported.failed.length, 0);
		const status: Status = await statusOf(source.id);
		assert.equal(status.mods.length, 2);
		// The pack keeps its own profile name; the target already had `Default`, so the imported one gets a suffix.
		assert.equal(status.profiles.length, 2, "the pack did not create a second profile");
		const packProfile = status.profiles.find((entry) => !entry.active);
		assert.ok(packProfile, "the pack profile is missing");
		assert.equal(packProfile.enabled_count, 2);

		for (const mod of status.mods) {
			expectFile(path.join(dataDir, "games", source.id, "library", mod.folder, "content/x.ws"));
		}

		await cleanupFixture(source.id, source.fixture.root);
	});
	it("rejects a pack link that is not an https gobbo link", async () => {
		const game = await sourceGame("pack-link");

		try {
			await invokeWithChannel("download_pack_link", {
				game: game.id,
				link: "http://example.invalid/pack.gobbopack",
			});
			throw new Error("the link was expected to be refused");
		} catch (error) {
			assert.ok(error instanceof CommandFailure, "the link failed with an unexpected error");
			assert.equal(error.code, "pack.link_invalid");
		}

		const pending = await invoke<string | null>("take_pack_link");
		assert.equal(pending, null);
		await cleanupFixture(game.id, game.fixture.root);
	});
});
