import type { ProtonBuild } from "@/bindings";

/// The value of the "Steam's choice" entry. A folder path never equals it, so it stands for the empty setting, which
/// lets Gobbo pick the build Steam assigns to the game.
export const PROTON_STEAM = "steam";

/// The builds the game settings list. A folder the game already uses but that is not in the list (one chosen by hand)
/// comes first, so saving the form never drops the user's choice.
export function protonChoices(builds: readonly ProtonBuild[], chosen: string): ProtonBuild[] {
	if (chosen === "" || builds.some((build) => build.folder === chosen)) return [...builds];
	return [{ name: chosen, folder: chosen }, ...builds];
}
