import { expect, test } from "bun:test";
import { gamePresentation } from "@/app/selectors/game-presentation";
import type { BlockerKey, GameStage_Serialize, NextAction, Status_Serialize } from "@/bindings";
import { statusFixture } from "@/test/fixtures/status";

/// A status with safe defaults. The selector reads the stage and the profile list; the rest only has to typecheck.
function status(stage: GameStage_Serialize, overrides: Partial<Status_Serialize> = {}): Status_Serialize {
	return statusFixture(stage, { next_action: nextActionOf(stage), blocker: blockerOfStage(stage, overrides), ...overrides });
}

/// The blocker the backend derives for a stage, so a test reads the same value the window does.
function blockerOfStage(stage: GameStage_Serialize, overrides: Partial<Status_Serialize>): BlockerKey | null {
	if (stage.kind === "setup") {
		if (stage.step === "game") return overrides.needs_settings_dir ? "game_and_settings" : "game_folder";

		if (stage.step === "settings") return "game_and_settings";
		return "mods_folder";
	}

	if (stage.kind === "blocked") return "problems";

	if (stage.kind === "empty" || stage.kind === "no_enabled") return "no_mods";
	return null;
}

function nextActionOf(stage: GameStage_Serialize): NextAction {
	switch (stage.kind) {
		case "setup":
			if (stage.step === "game") return "set_game_folder";

			if (stage.step === "settings") return "set_settings_folder";
			return "set_mods_folder";
		case "blocked":
			return "fix_problems";
		case "empty":
			return "import_mods";
		case "no_enabled":
			return "enable_mods";
		case "ready":
		case "draft":
			return "apply";
		case "applied":
			return "play";
		default:
			return "apply";
	}
}

test("setup names the folder that is missing and blocks play", () => {
	const presentation = gamePresentation(status({ kind: "setup", step: "game" }));
	expect(presentation.tone).toBe("warning");
	expect(presentation.stateKey).toBe("header.setupRequired");
	expect(presentation.primary).toMatchObject({ id: "fix-setup", labelKey: "header.setGameFolder" });
	expect(presentation.blockedReason).toBe("deploy.blocker.gameFolder");
	expect(presentation.canPlay).toBe(false);
	expect(presentation.canApply).toBe(false);
	expect(presentation.nextStep?.titleKey).toBe("start.setup.title");
});

test("a game with a settings folder asks for it with the shared blocker", () => {
	const presentation = gamePresentation(status({ kind: "setup", step: "settings" }, { needs_settings_dir: true }));
	expect(presentation.primary.labelKey).toBe("header.setSettingsFolder");
	expect(presentation.blockedReason).toBe("deploy.blocker.gameAndSettings");
});

test("a blocked list offers a fix first and still allows a plain play", () => {
	const presentation = gamePresentation(status({ kind: "blocked", reasons: [{ code: "status.requirement_missing", params: {} }] }));
	expect(presentation.tone).toBe("danger");
	expect(presentation.primary.id).toBe("fix-problems");
	expect(presentation.blockedReason).toBe("deploy.blocker.problems");
	expect(presentation.canPlay).toBe(true);
	expect(presentation.canPlayWithoutMods).toBe(true);
	expect(presentation.menu.map((action) => action.id)).toEqual(["play"]);
});

test("an empty library leads with import and allows a plain play", () => {
	const presentation = gamePresentation(status({ kind: "empty" }));
	expect(presentation.primary).toMatchObject({ id: "import", labelKey: "mods.import" });
	expect(presentation.canPlay).toBe(true);
	expect(presentation.canPlayWithoutMods).toBe(true);
	expect(presentation.nextStep?.titleKey).toBe("start.add.title");
});

test("a library with every mod off offers play and points at the switches", () => {
	const presentation = gamePresentation(status({ kind: "no_enabled", total: 4 }));
	expect(presentation.primary).toMatchObject({ id: "play", labelKey: "header.playWithoutMods" });
	expect(presentation.canApply).toBe(false);
	expect(presentation.menu.map((action) => action.id)).toEqual(["enable"]);
	expect(presentation.nextStep?.titleKey).toBe("start.enable.title");
});

test("a ready list leads with apply and play", () => {
	const presentation = gamePresentation(status({ kind: "ready", enabled: 2, total: 5 }));
	expect(presentation.primary).toMatchObject({ id: "apply-and-play", labelKey: "header.deployAndLaunch" });
	expect(presentation.canApply).toBe(true);
	expect(presentation.canPlay).toBe(true);
	expect(presentation.menu.map((action) => action.id)).toEqual(["apply"]);
});

test("another profile's deployment is named without changing the active list", () => {
	const presentation = gamePresentation(
		status(
			{ kind: "ready", enabled: 1, total: 1 },
			{
				profiles: [
					{
						id: "p2",
						name: "Testing",
						mod_count: 3,
						enabled_count: 3,
						active: false,
						deployed: true,
						has_user_data: false,
						collections: [],
					},
				],
			},
		),
	);
	expect(presentation.stateKey).toBe("header.deployedElsewhere");
	expect(presentation.stateParams).toEqual({ name: "Testing" });
});

test("changes waiting after an apply are one click away", () => {
	const presentation = gamePresentation(status({ kind: "draft", changes: 3, enabled: 4, total: 6 }, { deployed: true, draft_changes: 3 }));
	expect(presentation.tone).toBe("warning");
	expect(presentation.stateKey).toBe("header.draftChanges");
	expect(presentation.stateParams).toEqual({ count: 3 });
	expect(presentation.primary).toMatchObject({ id: "apply-and-play", labelKey: "header.applyAndLaunch" });
	expect(presentation.canDiscard).toBe(true);
	expect(presentation.canUndo).toBe(false);
	expect(presentation.nextStep?.params).toEqual({ count: 3 });
});

test("an applied list leads with play and can undo the apply", () => {
	const presentation = gamePresentation(status({ kind: "applied", at: 10, enabled: 4, total: 6 }, { deployed: true }));
	expect(presentation.tone).toBe("success");
	expect(presentation.stateKey).toBe("header.deployed");
	expect(presentation.primary).toMatchObject({ id: "play", labelKey: "header.launch" });
	expect(presentation.canUndo).toBe(true);
	expect(presentation.canApply).toBe(true);
	expect(presentation.menu.map((action) => action.id)).toEqual(["apply"]);
});

test("a running game replaces every other action with stop", () => {
	const presentation = gamePresentation(status({ kind: "applied", at: 10, enabled: 4, total: 6 }, { deployed: true }), {
		gameRunning: true,
		busy: null,
	});
	expect(presentation.tone).toBe("danger");
	expect(presentation.primary.id).toBe("stop");
	expect(presentation.primary.disabled).toBeUndefined();
	expect(presentation.canApply).toBe(false);
	expect(presentation.canPlay).toBe(false);
	expect(presentation.canUndo).toBe(false);
});

test("a missing mods folder asks for it with its own blocker", () => {
	const presentation = gamePresentation(status({ kind: "setup", step: "mods" }, { needs_mods_root: true, mods_ready: false }));
	expect(presentation.primary).toMatchObject({ id: "fix-setup", labelKey: "header.setModsFolder" });
	expect(presentation.blockedReason).toBe("deploy.blocker.modsFolder");
	expect(presentation.stateHintKey).toBe("deploy.blocker.modsFolder");
	expect(presentation.nextStep?.action.labelKey).toBe("header.setModsFolder");
});

test("the game folder blocker names both folders when the game also needs a settings folder", () => {
	const presentation = gamePresentation(status({ kind: "setup", step: "game" }, { needs_settings_dir: true }));
	expect(presentation.primary.labelKey).toBe("header.setGameFolder");
	expect(presentation.blockedReason).toBe("deploy.blocker.gameAndSettings");
	expect(presentation.nextStep?.action.labelKey).toBe("header.setGameFolder");
});

test("the primary action follows the backend next action, not the stage", () => {
	const presentation = gamePresentation(status({ kind: "ready", enabled: 1, total: 1 }, { next_action: "fix_problems" }));
	expect(presentation.primary).toMatchObject({ id: "fix-problems", labelKey: "header.fixProblems" });
	expect(presentation.nextStep?.titleKey).toBe("start.fix.title");
});

test("a ready list's next step is the deploy step with apply and play", () => {
	const presentation = gamePresentation(status({ kind: "ready", enabled: 2, total: 5 }));
	expect(presentation.nextStep?.titleKey).toBe("start.deploy.title");
	expect(presentation.nextStep?.action).toMatchObject({ id: "apply-and-play", labelKey: "header.deployAndLaunch" });
});

test("a deployed list has no next step, because play is already the next action", () => {
	const presentation = gamePresentation(status({ kind: "applied", at: 10, enabled: 4, total: 6 }, { deployed: true }));
	expect(presentation.primary).toMatchObject({ id: "play", labelKey: "header.launch" });
	expect(presentation.nextStep).toBeUndefined();
});

test("a busy apply disables only the apply action and keeps a legible primary", () => {
	const presentation = gamePresentation(status({ kind: "ready", enabled: 1, total: 1 }), { gameRunning: false, busy: "applying" });
	expect(presentation.stateKey).toBe("header.busyApplying");
	expect(presentation.primary).toMatchObject({ id: "apply-and-play", labelKey: "header.deployAndLaunch" });
	expect(presentation.primary.disabled).toBe(true);
	expect(presentation.primary.reasonKey).toBe("header.busyApplying");
	// The next step and the menu keep their real actions instead of going blank for the whole busy window.
	expect(presentation.nextStep?.action).toMatchObject({ id: "apply-and-play", disabled: true });
	expect(presentation.menu.map((action) => action.id)).toEqual(["apply"]);
	expect(presentation.menu[0]?.disabled).toBe(true);
	expect(presentation.canPlay).toBe(true);
});

test("a busy apply leaves undo and discard available", () => {
	const applied = gamePresentation(status({ kind: "applied", at: 10, enabled: 4, total: 6 }, { deployed: true }), {
		gameRunning: false,
		busy: "applying",
	});
	expect(applied.canUndo).toBe(true);
	expect(applied.menu.find((action) => action.id === "apply")?.disabled).toBe(true);

	const draft = gamePresentation(status({ kind: "draft", changes: 2, enabled: 3, total: 5 }, { deployed: true, draft_changes: 2 }), {
		gameRunning: false,
		busy: "applying",
	});
	expect(draft.canDiscard).toBe(true);
	expect(draft.menu.map((action) => action.id)).toEqual(["apply", "discard"]);
	expect(draft.menu.find((action) => action.id === "discard")?.disabled).toBeUndefined();
});

test("an applied game keeps undo available while an unrelated update runs", () => {
	const presentation = gamePresentation(status({ kind: "applied", at: 10, enabled: 4, total: 6 }, { deployed: true }), {
		gameRunning: false,
		busy: "updating",
	});
	expect(presentation.stateKey).toBe("header.busyUpdating");
	expect(presentation.primary).toMatchObject({ id: "play", labelKey: "header.launch" });
	expect(presentation.primary.disabled).toBeUndefined();
	expect(presentation.canUndo).toBe(true);
	expect(presentation.menu.map((action) => action.id)).toEqual(["apply"]);
});
