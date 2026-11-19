/// How the settings show the Proton switches. Every id the backend offers (`domain::proton_options`) appears in exactly
/// one group; an id the backend offers that no group lists is not shown, so the interface never guesses a label. The
/// backend owns the rules: which options conflict, and the order of the saved list.
export const PROTON_OPTION_GROUPS = [
	{ id: "general", options: ["wined3d", "proton_log", "no_esync", "no_fsync", "dxr", "shader_cache", "mangohud"] },
	{ id: "amd", options: ["fsr", "fsr4", "fsr_indicator"] },
	{ id: "nvidia", options: ["nvapi", "dlss", "dlss_indicator"] },
	{ id: "latency", options: ["antilag", "low_latency", "low_latency_reflex", "dxvk_lowlatency"] },
] as const satisfies ReadonlyArray<{ id: string; options: readonly string[] }>;

export type ProtonOption = (typeof PROTON_OPTION_GROUPS)[number]["options"][number];

export function isProtonOption(id: string): id is ProtonOption {
	return PROTON_OPTION_GROUPS.some((group) => (group.options as readonly string[]).includes(id));
}

/// The frame rates offered for the Proton cap. `null` means unlimited, the same as an unset backend value.
export const PROTON_FPS_LIMITS = [30, 40, 45, 60, 75, 90, 120, 144, 165, 240] as const;
