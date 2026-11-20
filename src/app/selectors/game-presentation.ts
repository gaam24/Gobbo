import type { BlockerKey, NextAction, Status } from "@/bindings";
import type { TranslationKey } from "@/i18n";
import { assertNever } from "@/lib/assert";

/// How a state is shown: the same tones as the shared status badge.
export type GameTone = "neutral" | "success" | "warning" | "danger";

/// An action the header, the palette and the inspector can offer. The id says what to do; the label is a translation key,
/// so no component builds its own sentence for the same state.
export type GameActionId =
	| "play"
	| "apply"
	| "apply-and-play"
	| "discard"
	| "stop"
	| "fix-setup"
	| "fix-problems"
	| "import"
	| "enable"
	| "retry";

export interface GameAction {
	id: GameActionId;
	labelKey: TranslationKey;
	params?: Record<string, string | number>;
	disabled?: boolean;
	/// Why the action is disabled, as a translation key. The palette and the header show it in the same way.
	reasonKey?: TranslationKey;
}

/// The next step for a game that is not ready yet. It is the one place that decides what to show first.
export interface GameNextStep {
	titleKey: TranslationKey;
	bodyKey: TranslationKey;
	params?: Record<string, string | number>;
	action: GameAction;
}

/// The presentation of one game state. Every surface reads this; no surface derives its own chip or primary action.
export interface GamePresentation {
	tone: GameTone;
	stateKey: TranslationKey;
	stateParams?: Record<string, string | number>;
	stateHintKey?: TranslationKey;
	primary: GameAction;
	menu: GameAction[];
	canApply: boolean;
	canPlay: boolean;
	canPlayWithoutMods: boolean;
	canDiscard: boolean;
	canUndo: boolean;
	/// The translation key of the first reason an apply is impossible, when there is one.
	blockedReason?: TranslationKey;
	nextStep?: GameNextStep;
}

/// The runtime part of the state. It is separate from `Status`, because the game can start and stop without a new status.
export interface GameRuntime {
	gameRunning: boolean;
	busy: "applying" | "launching" | "stopping" | "updating" | null;
}

const IDLE: GameRuntime = { gameRunning: false, busy: null };

/// The words of each busy state and the action whose own work is running. `updating` has no game action of its own,
/// so it disables none: the game menu stays usable while a mod update runs in the background.
const BUSY = {
	applying: { stateKey: "header.busyApplying", action: "apply" },
	launching: { stateKey: "header.busyLaunching", action: "play" },
	stopping: { stateKey: "header.busyStopping", action: "stop" },
	updating: { stateKey: "header.busyUpdating", action: null },
} as const;

/// The one presentation of a game state. The status carries the stage and the next action; the next action decides the
/// primary button and the next step, and the stage decides the words and the menu around them. A transient busy state
/// only disables the action whose work is running — it does not blank the other actions for the whole window.
export function gamePresentation(status: Status, runtime: GameRuntime = IDLE): GamePresentation {
	if (runtime.gameRunning) {
		return {
			tone: "danger",
			stateKey: "header.running",
			primary: { id: "stop", labelKey: "header.forceClose" },
			menu: [],
			canApply: false,
			canPlay: false,
			canPlayWithoutMods: false,
			canDiscard: false,
			canUndo: false,
		};
	}

	const presentation = statusPresentation(status);
	return runtime.busy === null ? presentation : duringBusy(presentation, runtime.busy);
}

/// Keeps the status-derived presentation and disables only the action whose work is running. Undo, Discard and every
/// other menu entry stay available, as do the Working-panel actions the activity card owns.
function duringBusy(presentation: GamePresentation, busy: Exclude<GameRuntime["busy"], null>): GamePresentation {
	const { stateKey, action } = BUSY[busy];
	const disable = (item: GameAction): GameAction => {
		const isBusyAction = action !== null && (item.id === action || (action === "apply" && item.id === "apply-and-play"));
		return isBusyAction ? { ...item, disabled: true, reasonKey: item.reasonKey ?? stateKey } : item;
	};
	return {
		...presentation,
		tone: "warning",
		stateKey,
		primary: disable(presentation.primary),
		menu: presentation.menu.map(disable),
		canApply: action === "apply" ? false : presentation.canApply,
		canPlay: action === "play" ? false : presentation.canPlay,
		canPlayWithoutMods: action === "play" ? false : presentation.canPlayWithoutMods,
		nextStep: presentation.nextStep ? { ...presentation.nextStep, action: disable(presentation.nextStep.action) } : undefined,
	};
}

/// The presentation the status alone decides, before the transient runtime state is applied.
function statusPresentation(status: Status): GamePresentation {
	const blockedReason = blockerOf(status);
	const deployedElsewhere = status.profiles.find((profile) => profile.deployed && !profile.active)?.name;
	const primary = actionOf(status.next_action, status, blockedReason);
	const nextStep = nextStepOf(status, primary);

	switch (status.stage.kind) {
		case "setup":
			return {
				tone: "warning",
				stateKey: "header.setupRequired",
				stateHintKey: blockedReason,
				primary,
				menu: [],
				canApply: false,
				canPlay: false,
				canPlayWithoutMods: false,
				canDiscard: false,
				canUndo: false,
				blockedReason,
				nextStep,
			};
		case "blocked": {
			const play: GameAction = {
				id: "play",
				labelKey: status.deployed ? "header.launch" : "header.playWithoutMods",
			};
			return {
				tone: "danger",
				stateKey: "header.needsAttention",
				stateHintKey: "header.needsAttentionHint",
				primary,
				menu: [play],
				canApply: false,
				canPlay: true,
				canPlayWithoutMods: !status.deployed,
				canDiscard: false,
				canUndo: false,
				blockedReason,
				nextStep,
			};
		}
		case "empty": {
			const play: GameAction = { id: "play", labelKey: "header.playWithoutMods" };
			return {
				tone: "neutral",
				stateKey: "header.empty",
				primary,
				menu: [play],
				canApply: false,
				canPlay: true,
				canPlayWithoutMods: true,
				canDiscard: false,
				canUndo: false,
				nextStep,
			};
		}
		case "no_enabled":
			return {
				tone: "warning",
				stateKey: "header.noEnabled",
				stateParams: { total: status.stage.total },
				primary,
				menu: [{ id: "enable", labelKey: "start.enable.title" }],
				canApply: false,
				canPlay: true,
				canPlayWithoutMods: true,
				canDiscard: false,
				canUndo: false,
				blockedReason,
				nextStep,
			};
		case "ready":
			return {
				tone: "warning",
				stateKey: deployedElsewhere ? "header.deployedElsewhere" : "header.notDeployed",
				stateParams: deployedElsewhere ? { name: deployedElsewhere } : undefined,
				primary,
				menu: [{ id: "apply", labelKey: "header.deployWithoutLaunch", disabled: blockedReason !== undefined, reasonKey: blockedReason }],
				canApply: true,
				canPlay: true,
				canPlayWithoutMods: true,
				canDiscard: false,
				canUndo: false,
				blockedReason,
				nextStep,
			};
		case "draft":
			return {
				tone: "warning",
				stateKey: "header.draftChanges",
				stateParams: { count: status.stage.changes },
				stateHintKey: "header.draftHint",
				primary,
				menu: [
					{ id: "apply", labelKey: "header.deployWithoutLaunch", disabled: blockedReason !== undefined, reasonKey: blockedReason },
					{ id: "discard", labelKey: "header.discard" },
				],
				canApply: true,
				canPlay: true,
				canPlayWithoutMods: true,
				canDiscard: true,
				canUndo: false,
				blockedReason,
				nextStep,
			};
		case "applied":
			return {
				tone: "success",
				stateKey: "header.deployed",
				primary,
				menu: [{ id: "apply", labelKey: "header.applyAgain" }],
				canApply: true,
				canPlay: true,
				canPlayWithoutMods: false,
				canDiscard: false,
				canUndo: true,
				blockedReason,
			};
		default:
			return assertNever(status.stage);
	}
}

/// The primary action for the backend's next action. Each action keeps the label and the handler it had before the
/// backend decided it; `apply` depends on whether the game was deployed and whether an apply is blocked.
function actionOf(next: NextAction, status: Status, blockedReason: TranslationKey | undefined): GameAction {
	switch (next) {
		case "set_game_folder":
			return { id: "fix-setup", labelKey: "header.setGameFolder" };
		case "set_settings_folder":
			return { id: "fix-setup", labelKey: "header.setSettingsFolder" };
		case "set_mods_folder":
			return { id: "fix-setup", labelKey: "header.setModsFolder" };
		case "fix_problems":
			return { id: "fix-problems", labelKey: "header.fixProblems" };
		case "import_mods":
			return { id: "import", labelKey: "mods.import" };
		case "enable_mods":
			// The list has no enabled mod, so the primary button stays a plain play; the menu points at the switches.
			return { id: "play", labelKey: "header.playWithoutMods" };
		case "apply":
			return {
				id: "apply-and-play",
				labelKey: status.deployed ? "header.applyAndLaunch" : "header.deployAndLaunch",
				disabled: blockedReason !== undefined,
				reasonKey: blockedReason,
			};
		case "play":
			return { id: "play", labelKey: "header.launch" };
		default:
			return assertNever(next);
	}
}

/// The next step for the backend's next action. A deployed list has no next step, because play is already the action.
/// Only the apply step reads the stage, since a draft and a first deployment have different words.
function nextStepOf(status: Status, action: GameAction): GameNextStep | undefined {
	switch (status.next_action) {
		case "set_game_folder":
		case "set_settings_folder":
		case "set_mods_folder":
			return { titleKey: "start.setup.title", bodyKey: "start.setup.hint", action };
		case "fix_problems":
			return { titleKey: "start.fix.title", bodyKey: "start.fix.hint", action };
		case "import_mods":
			return { titleKey: "start.add.title", bodyKey: "start.add.hint", action };
		case "enable_mods":
			return { titleKey: "start.enable.title", bodyKey: "start.enable.hint", action };
		case "apply": {
			const stage = status.stage;

			if (stage.kind === "draft") {
				return { titleKey: "start.changes.title", bodyKey: "start.changes.hint", params: { count: stage.changes }, action };
			}

			return { titleKey: "start.deploy.title", bodyKey: "start.deploy.hint", action };
		}
		case "play":
			return undefined;
		default:
			return assertNever(status.next_action);
	}
}

/// The backend derives why an apply is impossible. The window only translates that value — it does not re-check the
/// folders or the list to decide which reason applies.
function blockerOf(status: Status): TranslationKey | undefined {
	if (!status.blocker) return undefined;
	const blocker: BlockerKey = status.blocker;

	switch (blocker) {
		case "game_and_settings":
			return "deploy.blocker.gameAndSettings";
		case "game_folder":
			return "deploy.blocker.gameFolder";
		case "mods_folder":
			return "deploy.blocker.modsFolder";
		case "problems":
			return "deploy.blocker.problems";
		case "no_mods":
			return "deploy.blocker.noMods";
		default:
			return assertNever(blocker);
	}
}
