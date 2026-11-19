import { type RefObject, useEffect } from "react";
import type { Status } from "@/bindings";
import { type GridCell, gridNeighbour } from "@/lib/grid-neighbour";
import { comboEventKey, comboHasModifier, shortcut, shortcutKey, shortcutKeys } from "@/lib/shortcuts";
import { inOverlay } from "@/views/mods/mods-split";

/// The arrows the list uses: up and down in the table; in the grid, all four.
const TABLE_ARROWS: readonly string[] = shortcutKeys("move");
const GRID_ARROWS: readonly string[] = [...TABLE_ARROWS, "ArrowLeft", "ArrowRight"];

const SEARCH_KEY = shortcutKey("search");
const RESCAN_KEY = shortcutKey("rescan");
const SELECT_ALL_KEY = shortcutKey("all");
const UNDO_KEYS = shortcutKeys("undo");
const REDO_SHIFT_KEYS = new Set(
	shortcut("redo")
		.keys.filter((combo) => comboHasModifier(combo, "Shift"))
		.map((combo) => comboEventKey(combo)),
);
const REDO_PLAIN_KEYS = new Set(
	shortcut("redo")
		.keys.filter((combo) => !comboHasModifier(combo, "Shift"))
		.map((combo) => comboEventKey(combo)),
);
const EDGE_KEYS = shortcutKeys("edge");
const LAST_EDGE_KEY = shortcutKey("edge", 1);
const CLEAR_KEY = shortcutKey("clear");
const TOGGLE_KEY = shortcutKey("toggle");

/// The keyboard of the mod list: search, history, selection, moving and toggling mods. It listens to the window.
export function useListKeys({
	searchRef,
	tableRef,
	grid,
	filtering,
	focused,
	mods,
	selection,
	shown,
	history,
	onSetEnabled,
	onMoveMod,
	onSelect,
	onUndoList,
	onRedoList,
	onRescan,
}: {
	searchRef: RefObject<HTMLInputElement | null>;
	tableRef: RefObject<HTMLDivElement | null>;
	/// The grid of cards is on screen: the arrows then follow where the cards are, not only the order of the list.
	grid: boolean;
	filtering: boolean;
	focused: string | null;
	mods: Status["mods"];
	selection: string[];
	shown: Status["mods"];
	history: Status["history"];
	onSetEnabled: (id: string, enabled: boolean) => void | Promise<void>;
	onMoveMod: (id: string, target: string, side: "above" | "below") => void | Promise<void>;
	onSelect: (folders: string[]) => void;
	onUndoList: () => void | Promise<void>;
	onRedoList: () => void | Promise<void>;
	onRescan: () => void | Promise<void>;
}) {
	/// Shortcuts: Ctrl+F searches, F5 rescans, arrows move the selection, Alt+arrows move a mod, Space toggles.
	/// Typing in a field keeps its keys, and the arrows only act when the focus is in the list.
	useEffect(() => {
		const typingTarget = (target: EventTarget | null): boolean =>
			target instanceof HTMLInputElement ||
			target instanceof HTMLTextAreaElement ||
			(target instanceof HTMLElement && target.isContentEditable);

		const focusRow = (modId: string) => {
			tableRef.current?.querySelector<HTMLElement>(`[data-mod="${CSS.escape(modId)}"]`)?.focus();
		};

		/// The mods on the screen with their boxes, for the grid. Only the mod names carry a mod id.
		const cellsOnScreen = (): GridCell[] =>
			[...(tableRef.current?.querySelectorAll<HTMLElement>("[data-mod]") ?? [])].flatMap((element) => {
				const id = element.dataset.mod;
				if (!id) return [];
				const box = element.getBoundingClientRect();
				return [{ id, top: box.top, left: box.left, right: box.right }];
			});

		/// Alt+arrows move the focused mod one place and keep the focus on it. The backend decides whether the move is
		/// allowed inside a group.
		const moveFocused = (delta: number) => {
			if (focused === null || filtering) return;
			const at = shown.findIndex((mod) => mod.id === focused);
			const target = shown[at + delta];
			if (at < 0 || !target) return;
			void onMoveMod(focused, target.id, delta < 0 ? "above" : "below");
			requestAnimationFrame(() => focusRow(focused));
		};

		/// Plain arrows move the selection through the visible rows, clamped at the ends. A focused row that a filter or a
		/// fold hid starts again at the first or last visible row instead of jumping to the first one every time.
		const selectNeighbour = (delta: number) => {
			const index = shown.findIndex((mod) => mod.id === focused);
			let next: (typeof shown)[number] | undefined;
			if (index < 0) next = delta > 0 ? shown[0] : shown.at(-1);
			else next = shown[Math.min(Math.max(index + delta, 0), shown.length - 1)];
			if (!next) return;
			onSelect([next.id]);
			focusRow(next.id);
		};

		/// Home and End: the first or the last mod the list shows.
		const selectEdge = (last: boolean) => {
			const next = last ? shown.at(-1) : shown[0];
			if (!next) return;
			onSelect([next.id]);
			focusRow(next.id);
		};

		/// Grid, up or down: the card above or below the focused one. A card that is not on the screen falls back to the
		/// order of the list, and the edge of the grid stays where it is.
		const selectAbove = (direction: "up" | "down") => {
			const cells = cellsOnScreen();
			if (focused === null || !cells.some((cell) => cell.id === focused)) {
				selectNeighbour(direction === "down" ? 1 : -1);
				return;
			}
			const next = gridNeighbour(cells, focused, direction);
			if (next === null) return;
			onSelect([next]);
			focusRow(next);
		};

		/// Grid, Alt with up or down: the focused mod takes the place of the card above or below, as a drop on that card does.
		const dropAbove = (direction: "up" | "down") => {
			if (focused === null || filtering || !shown.some((mod) => mod.id === focused)) return;
			const target = gridNeighbour(cellsOnScreen(), focused, direction);
			if (target === null) return;
			void onMoveMod(focused, target, direction === "down" ? "below" : "above");
			requestAnimationFrame(() => focusRow(focused));
		};

		/// Space switches the selected mods together: on, unless all of them are on already. Each edit joins the queue.
		const toggleFocused = () => {
			const chosen = new Set(selection);
			const picked = mods.filter((mod) => chosen.has(mod.id));
			if (picked.length === 0) return;
			const on = !picked.every((mod) => mod.enabled);
			for (const mod of picked) void onSetEnabled(mod.id, on);
		};

		/// Ctrl+Z takes the last change of the list back; Ctrl+Shift+Z and Ctrl+Y bring it again. Fields keep their own.
		const onHistoryKey = (event: KeyboardEvent): boolean => {
			const target = event.target instanceof HTMLElement ? event.target : null;
			if (typingTarget(event.target) || inOverlay(target) || !(event.ctrlKey || event.metaKey)) return false;
			const key = event.key.toLowerCase();
			if (UNDO_KEYS.includes(key) && !event.shiftKey && history.can_undo) void onUndoList();
			else if ((REDO_PLAIN_KEYS.has(key) || (REDO_SHIFT_KEYS.has(key) && event.shiftKey)) && history.can_redo) void onRedoList();
			else return false;
			event.preventDefault();
			return true;
		};

		/// Ctrl+F and F5 work everywhere, including while typing in a field, but not inside a dialog or a menu.
		const onGlobalKey = (event: KeyboardEvent): boolean => {
			const target = event.target instanceof HTMLElement ? event.target : null;
			if (inOverlay(target)) return false;
			if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === SEARCH_KEY) {
				event.preventDefault();
				searchRef.current?.focus();
				return true;
			}
			if (event.key === RESCAN_KEY) {
				event.preventDefault();
				void onRescan();
				return true;
			}
			return false;
		};

		const interactiveControl = (target: HTMLElement | null): boolean =>
			Boolean(
				target?.closest(
					"button, a, input, select, textarea, [role='combobox'], [role='switch'], [role='menuitem'], [role='option'], [role='listbox'], [role='separator']",
				),
			);

		/// The arrows of the list: in the table, up and down walk the rows and Alt moves the mod one place. In the grid, left
		/// and right walk the order, up and down go to the card above or below, and Alt moves the mod there.
		const moveWithArrow = (event: KeyboardEvent) => {
			event.preventDefault();
			const delta = event.key === "ArrowDown" || event.key === "ArrowRight" ? 1 : -1;
			const vertical = event.key === "ArrowUp" || event.key === "ArrowDown";
			if (!(grid && vertical)) {
				if (event.altKey) moveFocused(delta);
				else selectNeighbour(delta);
				return;
			}
			const direction = delta > 0 ? "down" : "up";
			if (event.altKey) dropAbove(direction);
			else selectAbove(direction);
		};

		/// The keys that act on the list once the focus is in it: Home and End, the arrows, and Space toggling the focused mod.
		const onListAction = (event: KeyboardEvent) => {
			if (EDGE_KEYS.includes(event.key)) {
				event.preventDefault();
				selectEdge(event.key === LAST_EDGE_KEY);
				return;
			}
			if ((grid ? GRID_ARROWS : TABLE_ARROWS).includes(event.key)) {
				moveWithArrow(event);
				return;
			}
			if (event.key === TOGGLE_KEY && focused) {
				event.preventDefault();
				toggleFocused();
			}
		};

		/// List keys: Ctrl+A selects every shown mod, Escape clears the selection, arrows move it, Alt+arrows move a mod,
		/// Space toggles.
		/// Typing in a field keeps its keys, and the arrows only act when the focus is in the list.
		const onListKey = (event: KeyboardEvent) => {
			// `instanceof` narrows without a cast; anything but an element (the document) behaves like no target.
			const target = event.target instanceof HTMLElement ? event.target : null;
			if (event.defaultPrevented || typingTarget(event.target) || inOverlay(target)) return;
			// Space and arrows belong to the window only on the list itself or on the focused row's name button. Other
			// buttons, selects and separators keep their own keyboard behaviour.
			const rowButton = target?.dataset.mod ?? null;
			if (interactiveControl(target) && rowButton !== focused) return;
			if (event.key === CLEAR_KEY && selection.length > 0) {
				onSelect([]);
				return;
			}
			const inList = target === document.body || Boolean(target && tableRef.current?.contains(target));
			if (inList) onListAction(event);
		};

		/// Ctrl+A selects every mod the list shows, outside fields, dialogs and menus.
		const onSelectAllKey = (event: KeyboardEvent): boolean => {
			const target = event.target instanceof HTMLElement ? event.target : null;
			if (
				typingTarget(event.target) ||
				inOverlay(target) ||
				!(event.ctrlKey || event.metaKey) ||
				event.key.toLowerCase() !== SELECT_ALL_KEY
			)
				return false;
			event.preventDefault();
			onSelect(shown.map((mod) => mod.id));
			return true;
		};

		const onKey = (event: KeyboardEvent) => {
			if (onGlobalKey(event) || onHistoryKey(event) || onSelectAllKey(event)) return;
			onListKey(event);
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [
		filtering,
		focused,
		grid,
		mods,
		onMoveMod,
		onRedoList,
		onRescan,
		onSetEnabled,
		onSelect,
		onUndoList,
		selection,
		shown,
		history,
		searchRef,
		tableRef,
	]);
}
