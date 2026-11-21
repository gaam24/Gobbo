import type { ProtonBuild, Runtime } from "@/bindings";

/// The value of the automatic entry. A folder path never equals it, so it stands for the empty setting, which lets Gobbo
/// pick the build: Steam's choice for a Steam install, the newest installed build for any other install.
export const PROTON_STEAM = "steam";

/// The label of the automatic entry: Steam's choice only for a Steam install under Proton, the newest installed build
/// otherwise. The backend decides the runtime, so the window never re-derives the store.
export function protonAutomaticLabel(runtime: Runtime): "settings.proton.steamChoice" | "settings.proton.newest" {
	return runtime === "steam_proton" ? "settings.proton.steamChoice" : "settings.proton.newest";
}

/// The builds the game settings list. A folder the game already uses but that is not in the list (one chosen by hand)
/// comes first, so saving the form never drops the user's choice.
export function protonChoices(builds: readonly ProtonBuild[], chosen: string): ProtonBuild[] {
	if (chosen === "" || builds.some((build) => build.folder === chosen)) return [...builds];
	return [{ name: chosen, folder: chosen }, ...builds];
}
