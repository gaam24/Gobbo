import { strict as assert } from "node:assert";
import path from "node:path";
import { browser } from "@wdio/globals";
import { CommandFailure, invoke, invokeWithChannel } from "../support/bridge";
import { dataDir, definitionsDir } from "../support/env";
import { expectFile, expectNoFile, removeTree } from "../support/files";
import { invalidZip, modZip, multiModZip, traversalZip, versionModZip, wrappedModZip } from "../support/fixtures";
import { cleanupGame, enableMods, importMods, seedGame, statusOf, uniqueId } from "../support/seed";
import { activateMenuItem, clickMenuTrigger, waitForMod } from "../support/ui";

async function importError(game: string, archive: string): Promise<{ code: string }> {
	try {
		await invokeWithChannel("import_zip", {
			game,
			path: archive,
			choices: null,
			source: null,
		});
		throw new Error("the import was expected to fail");
	} catch (error) {
		if (error instanceof CommandFailure) return error;
		throw error;
	}
}

describe("import", () => {
	it("imports an archive through the real Import button and shows it disabled", async () => {
		const id = uniqueId("import-ui");
		const name = `E2E Import ${id}`;
		const { fixture } = await seedGame({ id, name });
		const archive = modZip("import-ui.zip", "UiMod");
		await browser.execute((file: string) => {
			window.__GOBBO_E2E_DIALOGS__ = { files: [file] };
		}, archive);
		await clickMenuTrigger("More actions");
		await activateMenuItem("Import archive");
		await waitForMod("import-ui");
		const status = await statusOf(id);
		const mod = status.mods.find((item) => item.folder.includes("import-ui"));
		assert.ok(mod, "the imported mod is not in the status");
		assert.equal(mod.enabled, false);
		expectFile(path.join(dataDir, "games", id, "library", mod.folder, "content/x.ws"));
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("imports several archives from one picker answer", async () => {
		const id = uniqueId("import-many");
		const name = `E2E Import Many ${id}`;
		const { fixture } = await seedGame({ id, name });
		const first = modZip("import-many-a.zip", "Alpha");
		const second = modZip("import-many-b.zip", "Beta");
		await browser.execute(
			(files: string[]) => {
				window.__GOBBO_E2E_DIALOGS__ = { files };
			},
			[first, second],
		);
		await clickMenuTrigger("More actions");
		await activateMenuItem("Import archive");
		await waitForMod("import-many-a");
		await waitForMod("import-many-b");
		const status = await statusOf(id);
		assert.equal(status.mods.length, 2);
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("strips one wrapper folder and imports a multi-mod archive as separate mods", async () => {
		const id = uniqueId("import-shapes");
		const name = `E2E Import Shapes ${id}`;
		const { fixture } = await seedGame({ id, name });
		const wrapped = wrappedModZip("import-wrapped.zip", "Wrapper", "Wrapped");
		const multi = multiModZip("import-multi.zip", ["MultiA", "MultiB"]);
		const imported = await importMods(id, [wrapped, multi]);
		const folders = imported.flatMap((view) => view.folders);
		assert.ok(
			folders.some((folder) => folder.includes("import-wrapped")),
			`the wrapper folder stayed: ${folders.join(", ")}`,
		);
		assert.ok(
			folders.some((folder) => folder.includes("MultiA")),
			`MultiA is missing: ${folders.join(", ")}`,
		);
		assert.ok(
			folders.some((folder) => folder.includes("MultiB")),
			`MultiB is missing: ${folders.join(", ")}`,
		);
		assert.equal((await statusOf(id)).mods.length, 3);
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("refuses an archive with no recognised layout and leaves the library empty", async () => {
		const id = uniqueId("import-invalid");
		const name = `E2E Import Invalid ${id}`;
		const { fixture } = await seedGame({ id, name });
		const error = await importError(id, invalidZip("import-invalid.zip"));
		assert.equal(error.code, "install.unknown_layout");
		assert.equal((await statusOf(id)).mods.length, 0);
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("refuses a path that escapes the game and never writes outside the library", async () => {
		const id = uniqueId("import-traversal");
		const name = `E2E Import Traversal ${id}`;
		const { fixture } = await seedGame({ id, name });
		const error = await importError(id, traversalZip("import-traversal.zip"));
		assert.equal(error.code, "install.unsafe_entry");
		expectNoFile(path.join(dataDir, "escaped.txt"));
		assert.equal((await statusOf(id)).mods.length, 0);
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("records a provider source given with the archive", async () => {
		const id = uniqueId("import-source");
		const name = `E2E Import Source ${id}`;
		const { fixture } = await seedGame({ id, name });
		const archive = modZip("import-source.zip", "Sourced");
		await importMods(id, [archive], {
			provider: "thunderstore",
			mod_id: "Owner-Sourced",
		});
		const status = await statusOf(id);
		const mod = status.mods.find((item) => item.folder.includes("import-source"));
		assert.ok(mod, "the sourced mod is missing");
		assert.equal(mod.source?.provider, "thunderstore");
		assert.equal(mod.source?.mod_id, "Owner-Sourced");
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
	it("keeps two provider versions of one mod and switches between them", async () => {
		const id = uniqueId("import-version");
		const name = `E2E Import Version ${id}`;
		const { fixture } = await seedGame({ id, name });
		const first = versionModZip("Versioned-1.0.zip", "Versioned", "1.0");
		const second = versionModZip("Versioned-2.0.zip", "Versioned", "2.0");
		const source = { provider: "thunderstore", mod_id: "Owner-Versioned" };
		await importMods(id, [first], source);
		await enableMods(id);
		await importMods(id, [second], source);
		const status = await statusOf(id);
		const mod = status.mods.find((item) => item.folder.includes("Versioned"));
		assert.ok(mod, "the versioned mod is missing");
		assert.equal(mod.version_count, 2);
		const versions = await invoke<Array<{ folder: string; version: string | null }>>("mod_versions", { game: id, folder: mod.folder });
		assert.equal(versions.length, 2);
		const newer = versions.find((version) => version.version === "2.0");
		assert.ok(newer, `the 2.0 version is missing: ${JSON.stringify(versions)}`);
		await invoke("set_mod_version", {
			game: id,
			id: mod.id,
			folder: newer.folder,
		});
		const switched = await statusOf(id);
		const selected = switched.mods.find((item) => item.id === mod.id);
		assert.equal(selected?.info.version, "2.0");
		assert.ok(switched.history.can_undo, "the version switch did not create a history entry");
		await cleanupGame(id);
		removeTree(path.join(definitionsDir, id));
		removeTree(fixture.root);
	});
});
