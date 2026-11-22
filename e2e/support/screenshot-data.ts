/// Intentionally realistic data for the README screenshots only: a real game id, icon path and mod names make the
/// captured images look like the shipped app. No other spec uses this data; the suite's fixtures stay generic.
export const screenshotGame = {
	id: "the-witcher-3-demo",
	name: "The Witcher 3: Wild Hunt",
	icon: "games/the-witcher-3-wild-hunt/icon.webp",
} as const;

export interface ScreenshotMod {
	archive: string;
	folder: string;
	name: string;
	version: string;
	files?: Record<string, string>;
}

/// Two mods write `content/shared.ws` so the conflict screenshot has something to show.
export const screenshotMods: ScreenshotMod[] = [
	{
		archive: "readme-community-patch.zip",
		folder: "modCommunityPatch",
		name: "Community Patch",
		version: "4.2.1",
		files: { "content/shared.ws": "community" },
	},
	{
		archive: "readme-hd-reworked.zip",
		folder: "modHDReworkedProject",
		name: "HD Reworked Project",
		version: "12.0",
	},
	{
		archive: "readme-friendly-hud.zip",
		folder: "modFriendlyHUD",
		name: "Friendly HUD",
		version: "3.1",
	},
	{
		archive: "readme-no-time-for-gwent.zip",
		folder: "modNoTimeForGwent",
		name: "No Time for Gwent",
		version: "2.0",
	},
	{
		archive: "readme-shared-utils.zip",
		folder: "modSharedUtils",
		name: "Shared Utils",
		version: "1.4",
	},
	{
		archive: "readme-better-roach.zip",
		folder: "modBetterRoachControls",
		name: "Better Roach Controls",
		version: "1.2",
		files: { "content/shared.ws": "roach" },
	},
	{
		archive: "readme-photo-mode.zip",
		folder: "modPhotoModeUnlocked",
		name: "Photo Mode Unlocked",
		version: "1.0",
	},
	{
		archive: "readme-no-intro.zip",
		folder: "modNoIntroVideos",
		name: "No Intro Videos",
		version: "2.2",
	},
	{
		archive: "readme-extra-tooltips.zip",
		folder: "modExtraTooltips",
		name: "Extra Tooltips",
		version: "1.1",
	},
	{
		archive: "readme-debug-console.zip",
		folder: "modDebugConsole",
		name: "Debug Console",
		version: "1.0",
	},
];
