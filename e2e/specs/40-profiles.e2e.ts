import { strict as assert } from "node:assert";
import { invoke, reloadWindow } from "../support/bridge";
import { modZip } from "../support/fixtures";
import {
	cleanupFixture,
	createProfile,
	enableMods,
	importMods,
	seedGame,
	statusOf,
	switchProfile,
	uniqueId,
	waitForStatus,
} from "../support/seed";
import {
	activateMenuItem,
	clickByLabel,
	clickByText,
	clickMenuItem,
	clickMenuTrigger,
	press,
	toggleMod,
	typeInto,
	waitForDialog,
	waitForMod,
	waitForText,
} from "../support/ui";

interface ProfileGame {
	id: string;
	name: string;
	fixture: { root: string };
}

async function profileGame(prefix: string): Promise<ProfileGame> {
	const id = uniqueId(prefix);
	const name = `E2E Profiles ${id}`;
	const { fixture } = await seedGame({ id, name });
	const archives = ["a", "b", "c"].map((suffix) => modZip(`profiles-${id}-${suffix}.zip`, `Profile ${suffix.toUpperCase()}`));
	await importMods(id, archives);
	await enableMods(id);
	await reloadWindow();
	await waitForMod(`profiles-${id}-a`);
	return { id, name, fixture };
}

describe("profiles", () => {
	it("creates and duplicates a profile from the header menus", async () => {
		const game = await profileGame("profiles-create");
		await clickMenuTrigger("Profiles");
		await activateMenuItem("Add profile");
		await waitForText("Profile 1");
		let status = await waitForStatus(game.id, (current) => current.profiles.length === 2);
		const active = status.profiles.find((profile) => profile.active);
		assert.equal(active?.name, "Profile 1");
		await clickMenuTrigger("Profile actions");
		await activateMenuItem("Duplicate");
		status = await waitForStatus(game.id, (current) => current.profiles.length === 3);
		const copied = status.profiles.find((profile) => profile.name.includes("copy"));
		assert.ok(copied, `the duplicated profile is missing: ${status.profiles.map((profile) => profile.name).join(", ")}`);
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("renames, switches and deletes a profile", async () => {
		const game = await profileGame("profiles-edit");
		await createProfile(game.id, "Other");
		await reloadWindow();
		await clickMenuTrigger("Profiles");
		// e2e-locale-exception: the profile name this spec created, not interface prose.
		await activateMenuItem("Other");
		await waitForStatus(game.id, (current) => current.profiles.find((profile) => profile.active)?.name === "Other");
		await clickMenuTrigger("Profile actions");
		await activateMenuItem("Rename");
		await waitForDialog("Rename profile");
		await typeInto("#rename-profile", "Renamed Profile");
		await clickByText("Rename", "button", "[role='dialog']");
		// e2e-locale-exception: the name this spec gave the generated profile, not interface prose.
		await waitForText("Renamed Profile");
		await waitForStatus(game.id, (current) => current.profiles.some((profile) => profile.name === "Renamed Profile"));
		await clickMenuTrigger("Profiles");
		// e2e-locale-exception: the built-in default profile's name, not interface prose.
		await activateMenuItem("Default");
		await waitForStatus(game.id, (current) => current.profiles.find((profile) => profile.active)?.name === "Default");
		await clickMenuTrigger("Profiles");
		// e2e-locale-exception: the name this spec gave the generated profile, not interface prose.
		await activateMenuItem("Renamed Profile");
		await waitForStatus(game.id, (current) => current.profiles.find((profile) => profile.active)?.name === "Renamed Profile");
		await clickMenuTrigger("Profile actions");
		await activateMenuItem("Delete profile");
		await waitForDialog("Delete profile");
		await clickByText("Delete profile", "button", "[role='alertdialog']");
		const status = await waitForStatus(game.id, (current) => current.profiles.length === 1);
		assert.equal(status.profiles[0]?.name, "Default", "the wrong profile was deleted");
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("undoes and redoes a list change from the toolbar", async () => {
		const game = await profileGame("profiles-undo");
		await toggleMod(`profiles-${game.id}-a`);
		await waitForStatus(game.id, (current) => current.mods.filter((mod) => mod.enabled).length === 2);
		await clickByLabel("Undo the last list change (Ctrl+Z)");
		await waitForStatus(game.id, (current) => current.mods.filter((mod) => mod.enabled).length === 3);
		await clickByLabel("Redo (Ctrl+Shift+Z)");
		await waitForStatus(game.id, (current) => current.mods.filter((mod) => mod.enabled).length === 2);
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("starts and stops the guided search for a broken mod", async () => {
		const game = await profileGame("profiles-bisect");
		await clickMenuTrigger("More actions");
		await clickMenuItem("Find the mod that breaks the game");
		await waitForText("Finding the mod that breaks the game");
		let status = await waitForStatus(game.id, (current) => current.bisect !== null);
		assert.ok(status.bisect, "the backend did not start a bisect");
		await clickByText("Still broken", "button");
		status = await waitForStatus(game.id, (current) => current.bisect !== null);
		assert.ok(status.bisect, "the first answer closed the bisect too early");
		await clickByText("Stop the search", "button");
		status = await waitForStatus(game.id, (current) => current.bisect === null);
		assert.equal(status.mods.filter((mod) => mod.enabled).length, 3, "the list was not restored");
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("compares the active profile with another one", async () => {
		const game = await profileGame("profiles-compare");
		const status = await statusOf(game.id);
		const active = status.profiles.find((profile) => profile.active);
		assert.ok(active);
		await createProfile(game.id, "Other");
		await switchProfile(game.id, status.profiles.find((profile) => profile.name === "Other")?.id ?? active.id);
		const other = await statusOf(game.id);
		const otherProfile = other.profiles.find((profile) => profile.active);
		assert.ok(otherProfile);
		await invoke("save_profile", {
			game: game.id,
			rows: other.mods.map((mod) => ({
				folder: mod.folder,
				enabled: mod.folder.endsWith("-b"),
				separator: null,
			})),
			known: null,
		});
		await reloadWindow();
		await clickMenuTrigger("Profile actions");
		await activateMenuItem("Compare profiles");
		await waitForDialog("Compare profiles");
		await waitForText("The same in both:");
		await waitForText("Different version or on/off");
		await press("Escape");
		await cleanupFixture(game.id, game.fixture.root);
	});
	it("keeps the profile list and the active profile after a reload", async () => {
		const game = await profileGame("profiles-reload");
		await createProfile(game.id, "Second Profile");
		const status = await statusOf(game.id);
		const second = status.profiles.find((profile) => profile.name === "Second Profile");
		assert.ok(second);
		await switchProfile(game.id, second.id);
		await reloadWindow();
		// e2e-locale-exception: the name this spec gave the generated profile, not interface prose.
		await waitForText("Second Profile");
		const after = await statusOf(game.id);
		assert.equal(after.profiles.find((profile) => profile.active)?.name, "Second Profile");
		await cleanupFixture(game.id, game.fixture.root);
	});
});
