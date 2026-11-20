import type { TranslationKey } from "@/i18n";

/// Every keyboard shortcut of the app, keyed by a stable id. One entry is the source of truth for the keys a handler
/// matches, the hint the palette shows and the row the shortcut list renders.
export type ShortcutId =
	| "palette"
	| "help"
	| "search"
	| "details"
	| "console"
	| "rescan"
	| "move"
	| "edge"
	| "select"
	| "range"
	| "all"
	| "toggle"
	| "reorder"
	| "clear"
	| "undo"
	| "redo";

export type ShortcutContext = "general" | "list";
export type ShortcutModifier = "Ctrl" | "Shift" | "Alt";

export interface ShortcutCombo {
	readonly modifiers: readonly ShortcutModifier[];
	/// The display form of the main key: a letter, an arrow, a function key or a named key such as `Space`.
	readonly key: string;
}

export interface ShortcutDefinition {
	/// Chords in the order the shortcut list shows them. The first one also supplies the palette hint by default.
	readonly keys: readonly [ShortcutCombo, ...ShortcutCombo[]];
	readonly labelKey: TranslationKey;
	readonly context: ShortcutContext;
	/// The palette shows the second redo chord (`Ctrl+Y`), not the first one listed in the dialog (`Ctrl+Shift+Z`).
	readonly hintCombo?: number;
}

const definitions: Record<ShortcutId, ShortcutDefinition> = {
	palette: {
		context: "general",
		labelKey: "shortcuts.items.palette",
		keys: [{ modifiers: ["Ctrl"], key: "K" }],
	},
	help: {
		context: "general",
		labelKey: "shortcuts.items.help",
		keys: [
			{ modifiers: [], key: "?" },
			{ modifiers: [], key: "F1" },
		],
	},
	search: {
		context: "general",
		labelKey: "shortcuts.items.search",
		keys: [{ modifiers: ["Ctrl"], key: "F" }],
	},
	details: {
		context: "general",
		labelKey: "shortcuts.items.details",
		keys: [{ modifiers: ["Ctrl"], key: "I" }],
	},
	console: {
		context: "general",
		labelKey: "shortcuts.items.console",
		keys: [{ modifiers: ["Ctrl"], key: "J" }],
	},
	rescan: {
		context: "general",
		labelKey: "shortcuts.items.rescan",
		keys: [{ modifiers: [], key: "F5" }],
	},
	move: {
		context: "list",
		labelKey: "shortcuts.items.move",
		keys: [
			{ modifiers: [], key: "↑" },
			{ modifiers: [], key: "↓" },
		],
	},
	edge: {
		context: "list",
		labelKey: "shortcuts.items.edge",
		keys: [
			{ modifiers: [], key: "Home" },
			{ modifiers: [], key: "End" },
		],
	},
	select: {
		context: "list",
		labelKey: "shortcuts.items.select",
		keys: [{ modifiers: ["Ctrl"], key: "Click" }],
	},
	range: {
		context: "list",
		labelKey: "shortcuts.items.range",
		keys: [{ modifiers: ["Shift"], key: "Click" }],
	},
	all: {
		context: "list",
		labelKey: "shortcuts.items.all",
		keys: [{ modifiers: ["Ctrl"], key: "A" }],
	},
	toggle: {
		context: "list",
		labelKey: "shortcuts.items.toggle",
		keys: [{ modifiers: [], key: "Space" }],
	},
	reorder: {
		context: "list",
		labelKey: "shortcuts.items.reorder",
		keys: [
			{ modifiers: ["Alt"], key: "↑" },
			{ modifiers: ["Alt"], key: "↓" },
		],
	},
	clear: {
		context: "list",
		labelKey: "shortcuts.items.clear",
		keys: [{ modifiers: [], key: "Esc" }],
	},
	undo: {
		context: "list",
		labelKey: "shortcuts.items.undo",
		keys: [{ modifiers: ["Ctrl"], key: "Z" }],
	},
	redo: {
		context: "list",
		labelKey: "shortcuts.items.redo",
		hintCombo: 1,
		keys: [
			{ modifiers: ["Ctrl", "Shift"], key: "Z" },
			{ modifiers: ["Ctrl"], key: "Y" },
		],
	},
};

export const shortcutIds: readonly ShortcutId[] = [
	"palette",
	"help",
	"search",
	"details",
	"console",
	"rescan",
	"move",
	"edge",
	"select",
	"range",
	"all",
	"toggle",
	"reorder",
	"clear",
	"undo",
	"redo",
];

export function shortcut(id: ShortcutId): ShortcutDefinition {
	return definitions[id];
}

/// Display tokens in press order, e.g. `["Ctrl", "Shift", "Z"]`.
export function comboTokens(combo: ShortcutCombo): string[] {
	return [...combo.modifiers, combo.key];
}

export function comboHasModifier(combo: ShortcutCombo, modifier: ShortcutModifier): boolean {
	return combo.modifiers.includes(modifier);
}

const EVENT_KEYS: Record<string, string> = {
	Space: " ",
	Esc: "Escape",
	F1: "F1",
	F5: "F5",
	Home: "Home",
	End: "End",
	"↑": "ArrowUp",
	"↓": "ArrowDown",
	"←": "ArrowLeft",
	"→": "ArrowRight",
};

/// The `KeyboardEvent.key` of a combo's main key, in the exact form the handlers compare it. Letters are lowercased
/// because the handlers compare against a lowercased `event.key`; named keys keep their display casing.
export function comboEventKey(combo: ShortcutCombo): string {
	return EVENT_KEYS[combo.key] ?? combo.key.toLowerCase();
}

/// The key of one chord, defaulting to the first. Used where a handler compares against a single key.
export function shortcutKey(id: ShortcutId, index = 0): string {
	const combo = definitions[id].keys[index] ?? definitions[id].keys[0];
	return comboEventKey(combo);
}

/// Every key a shortcut listens for.
export function shortcutKeys(id: ShortcutId): string[] {
	return definitions[id].keys.map(comboEventKey);
}

/// The single flat hint shown next to a palette item.
export function shortcutHint(id: ShortcutId): string {
	const def = definitions[id];
	const combo = def.hintCombo === undefined ? def.keys[0] : (def.keys[def.hintCombo] ?? def.keys[0]);
	return comboTokens(combo).join("+");
}
