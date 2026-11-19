import { strict as assert } from "node:assert";
import path from "node:path";
import type { ModExtras_Serialize as ModExtras } from "../../src/bindings";
import { invoke, reloadWindow } from "../support/bridge";
import { definitionsDir } from "../support/env";
import { removeTree } from "../support/files";
import { modZip } from "../support/fixtures";
import { cleanupGame, enableMods, importMods, seedGame, statusOf, uniqueId, waitForStatus } from "../support/seed";
import {
	bodyText,
	clickByLabel,
	clickByText,
	clickMenuItem,
	clickMenuTrigger,
	clickMod,
	count,
	openModMenu,
	press,
	pressKeyOn,
	pressKeyOnMod,
	typeInto,
	waitForCount,
	waitForDialog,
	waitForMod,
	waitForNoText,
	waitForSelector,
	waitForText,
} from "../support/ui";

async function listGame(prefix: string): Promise<{ id: string; name: string; fixture: { root: string } }> {
	const id = uniqueId(prefix);
	const name = `E2E List ${id}`;
	const { fixture } = await seedGame({ id, name });
	const archives = ["a", "b", "c"].map((suffix) => modZip(`list-${id}-${suffix}.zip`, `List ${suffix.toUpperCase()}`));
	await importMods(id, archives);
	await enableMods(id, [`list-${id}-a`]);
	await reloadWindow();
	await waitForMod(`list-${id}-a`);
	return { id, name, fixture };
}

async function cleanup(id: string, fixture: { root: string }): Promise<void> {
	await cleanupGame(id);
	removeTree(path.join(definitionsDir, id));
	removeTree(fixture.root);
}

describe("mod list", () => {
	it("renders the imported rows and filters them by search and state", async () => {
		const { id, fixture } = await listGame("list-filter");
		assert.equal(await count("[data-mod]"), 3);
		await typeInto("Search mods", `list-${id}-b`);
		await waitForNoText(`list-${id}-a`);
		assert.equal(await count("[data-mod]"), 1);
		await clickByLabel("Clear the search");
		await waitForMod(`list-${id}-a`);
		await clickByText("Disabled", "[role='radio']");
		await waitForCount("[data-mod]", 2);
		await clickByText("Enabled", "[role='radio']");
		await waitForCount("[data-mod]", 1);
		await clickByText("All", "[role='radio']");
		await waitForCount("[data-mod]", 3);
		await cleanup(id, fixture);
	});
	it("selects rows with Ctrl and toggles them with Space", async () => {
		const { id, fixture } = await listGame("list-select");
		await clickMod(`list-${id}-a`);
		await clickMod(`list-${id}-b`, { control: true });
		await waitForText("2 mods selected");
		await pressKeyOnMod(`list-${id}-b`, " ");
		const status = await waitForStatus(id, (current) => current.mods.filter((mod) => mod.enabled).length === 2);
		const enabled = status.mods.filter((mod) => mod.enabled).map((mod) => mod.folder);
		assert.deepEqual(enabled.toSorted(), [`list-${id}-a`, `list-${id}-b`].toSorted());
		await cleanup(id, fixture);
	});
	it("switches between the table and the grid of cards", async () => {
		const { id, fixture } = await listGame("list-grid");
		await clickByLabel("Grid view");
		await waitForSelector("[data-grid-card]");
		assert.equal(await count("[data-grid-card]"), 3);
		await clickByLabel("List view");
		await waitForSelector("[role='table']");
		assert.equal((await count("[role='columnheader']")) > 0, true);
		await cleanup(id, fixture);
	});
	it("hides a column, keeps the layout across a reload and resets it", async () => {
		const { id, fixture } = await listGame("list-columns");
		await clickMenuTrigger("Columns");
		await clickMenuItem("Order");
		await press("Escape");
		await waitForNoText("Order");
		await reloadWindow();
		await waitForMod(`list-${id}-a`);
		assert.ok(!(await bodyText()).includes("Order"), "the hidden column came back after the reload");
		await clickMenuTrigger("Columns");
		await clickMenuItem("Reset the layout");
		await press("Escape");
		await waitForText("Order");
		await cleanup(id, fixture);
	});
	it("edits tags and a note in the inspector and saves them", async () => {
		const { id, fixture } = await listGame("list-inspector");
		await clickMod(`list-${id}-a`);
		await waitForText("Overview");
		await typeInto("Add a tag and press Enter", "e2e-tag");
		await pressKeyOn("#mod-tags", "Enter");
		await waitForText("e2e-tag");
		await typeInto("Your note about this mod", "note from e2e");
		await clickByText("Save note", "button");
		const status = await statusOf(id);
		const mod = status.mods.find((item) => item.folder === `list-${id}-a`);
		assert.ok(mod, "the mod is missing");
		assert.deepEqual(mod.tags, ["e2e-tag"]);
		const extras = await invoke<ModExtras[]>("mod_extras", { game: id, folders: [mod.folder] });
		assert.equal(extras[0]?.notes, "note from e2e");
		await cleanup(id, fixture);
	});
	it("deletes a mod from its context menu after confirmation", async () => {
		const { id, fixture } = await listGame("list-delete");
		await openModMenu(`list-${id}-a`);
		await clickMenuItem("Delete mod");
		await waitForDialog("Delete mod");
		await clickByText("Delete mod", "button", "[role='alertdialog']");
		await waitForNoText(`list-${id}-a`);
		assert.equal((await statusOf(id)).mods.length, 2);
		await cleanup(id, fixture);
	});
});
