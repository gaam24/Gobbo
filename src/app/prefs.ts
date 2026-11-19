import { isNumber, isText } from "@/lib/ui-prefs";

/// The seven screens of the launcher. `mods` and `settings` belong to the active game; the other five are shared by all
/// games. The two the launcher opens on are remembered across starts.
export type AppView = "games" | "mods" | "settings" | "app" | "downloads" | "notifications" | "storage";

/// The two views the launcher opens on. The others (downloads, notifications, settings) are not remembered across starts.
export type MainView = "mods" | "games";

/// The details panel's width the user dragged it to; without one it grows with the window.
export const INSPECTOR_PREF = "inspector-width";

export const isWidth = (value: unknown): value is number | null => value === null || (isNumber(value) && value >= 280);

/// The game and the main view the launcher was on, so the window opens where it stopped. A game that was removed falls
/// back to the first one, and an unknown view to the mod list.
export const GAME_PREF = "game";

export const VIEW_PREF = "view";

export const isGameId = (value: unknown): value is string | null => value === null || isText(value);

export const isMainView = (value: unknown): value is MainView => value === "mods" || value === "games";
