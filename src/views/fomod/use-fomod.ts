import { commands, type FomodState } from "@/bindings";
import type { FomodChoice } from "@/lib/fomod";
import { unwrap } from "@/lib/ipc";

/// Where a FOMOD wizard reads its installer. An import reads a loose archive; a reinstall reads the archive copy of an
/// installed version. Only the identity differs: the backend owns the group rules and the pick semantics in both cases.
export type FomodTarget = { kind: "archive"; path: string } | { kind: "reinstall"; folder: string };

/// The backend's identity for a target, as the command arguments expect it.
function source(target: FomodTarget) {
	return target.kind === "archive" ? ({ kind: "path", path: target.path } as const) : ({ kind: "folder", folder: target.folder } as const);
}

/// The installer's answer for a selection; `null` is the default selection.
export function readFomodState(game: string, target: FomodTarget, selection: FomodChoice[] | null): Promise<FomodState> {
	const from = source(target);
	return from.kind === "path"
		? unwrap(commands.fomodState(game, from.path, selection))
		: unwrap(commands.reinstallFomodState(game, from.folder, selection));
}

/// Asks the backend to apply one click to the current selection and answer with the selection it produces. The window
/// renders that answer; it does not work out what a radio or a required option means.
export function toggleFomodPick(
	game: string,
	target: FomodTarget,
	selection: FomodChoice[],
	step: number,
	group: number,
	option: string,
): Promise<FomodState> {
	return unwrap(commands.fomodTogglePick(game, source(target), selection, step, group, option));
}

/// Starts a reinstall from saved answers: the backend lays them over its defaults with the same rules a click applies.
export function fomodFromChoices(game: string, target: FomodTarget, choices: FomodChoice[]): Promise<FomodState> {
	return unwrap(commands.fomodSelectionFromChoices(game, source(target), choices));
}
