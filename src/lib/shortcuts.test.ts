import { expect, test } from "bun:test";

import {
	comboEventKey,
	comboTokens,
	type ShortcutId,
	shortcut,
	shortcutHint,
	shortcutIds,
	shortcutKey,
	shortcutKeys,
} from "@/lib/shortcuts";

/// The display tokens and palette hint of every shortcut. Pinned so a binding cannot silently disappear or change.
const EXPECTED: Record<ShortcutId, { keys: string[][]; hint: string }> = {
	palette: { keys: [["Ctrl", "K"]], hint: "Ctrl+K" },
	help: { keys: [["?"], ["F1"]], hint: "?" },
	search: { keys: [["Ctrl", "F"]], hint: "Ctrl+F" },
	details: { keys: [["Ctrl", "I"]], hint: "Ctrl+I" },
	console: { keys: [["Ctrl", "J"]], hint: "Ctrl+J" },
	rescan: { keys: [["F5"]], hint: "F5" },
	move: { keys: [["↑"], ["↓"]], hint: "↑" },
	edge: { keys: [["Home"], ["End"]], hint: "Home" },
	select: { keys: [["Ctrl", "Click"]], hint: "Ctrl+Click" },
	range: { keys: [["Shift", "Click"]], hint: "Shift+Click" },
	all: { keys: [["Ctrl", "A"]], hint: "Ctrl+A" },
	toggle: { keys: [["Space"]], hint: "Space" },
	reorder: {
		keys: [
			["Alt", "↑"],
			["Alt", "↓"],
		],
		hint: "Alt+↑",
	},
	clear: { keys: [["Esc"]], hint: "Esc" },
	undo: { keys: [["Ctrl", "Z"]], hint: "Ctrl+Z" },
	redo: {
		keys: [
			["Ctrl", "Shift", "Z"],
			["Ctrl", "Y"],
		],
		hint: "Ctrl+Y",
	},
};

test("every shortcut keeps its keys and its palette hint", () => {
	expect(shortcutIds.length).toBe(Object.keys(EXPECTED).length);
	for (const id of shortcutIds) {
		expect(Object.hasOwn(EXPECTED, id), id).toBe(true);
		const expected = EXPECTED[id];
		expect(shortcut(id).keys.map(comboTokens), id).toEqual(expected.keys);
		expect(shortcutHint(id), id).toBe(expected.hint);
	}
});

test("named keys keep the casing KeyboardEvent uses", () => {
	expect(shortcutKeys("help")).toEqual(["?", "F1"]);
	expect(shortcutKey("rescan")).toBe("F5");
	expect(shortcutKeys("edge")).toEqual(["Home", "End"]);
	expect(shortcutKey("clear")).toBe("Escape");
	expect(shortcutKey("toggle")).toBe(" ");
	expect(shortcutKeys("move")).toEqual(["ArrowUp", "ArrowDown"]);
});

test("no two shortcuts claim the same key in the same context", () => {
	const claims = new Map<string, string>();
	for (const id of shortcutIds) {
		const { context } = shortcut(id);
		for (const combo of shortcut(id).keys) {
			const signature = `${context}|${[...combo.modifiers].toSorted().join("+")}+${comboEventKey(combo)}`;
			const owner = claims.get(signature);
			expect(owner, `${id} and ${owner ?? "?"} both claim ${signature}`).toBeUndefined();
			claims.set(signature, id);
		}
	}
});
