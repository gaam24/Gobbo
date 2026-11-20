/// Small choices of this window, kept in the webview's storage: column widths, panel sizes, the game and view that were
/// open, and the notice list. They are not app settings: losing them only brings the defaults back, so every read checks
/// the shape and falls back.

const PREFIX = "gobbo.";

/// The value saved under `key`, when `valid` accepts it; `fallback` otherwise.
export function readPref<T>(key: string, fallback: T, valid: (value: unknown) => value is T): T {
	try {
		const text = localStorage.getItem(PREFIX + key);

		if (text === null) return fallback;
		const value: unknown = JSON.parse(text);
		return valid(value) ? value : fallback;
	} catch {
		return fallback;
	}
}

/// Saves `value` under `key`; `null` forgets it.
export function writePref(key: string, value: unknown) {
	try {
		if (value === null) localStorage.removeItem(PREFIX + key);
		else localStorage.setItem(PREFIX + key, JSON.stringify(value));
	} catch {
		// A full or blocked storage only means the layout is not remembered.
	}
}

export const isNumber = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);
export const isText = (value: unknown): value is string => typeof value === "string";
export const isTextList = (value: unknown): value is string[] => Array.isArray(value) && value.every((item) => isText(item));
