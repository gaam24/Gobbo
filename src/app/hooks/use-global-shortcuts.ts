import { useEffect, useRef } from "react";
import { useGameEditor } from "@/app/contexts/game-editor-context";
import { useGames } from "@/app/contexts/games-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { shortcutKey, shortcutKeys } from "@/lib/shortcuts";

const PALETTE_KEY = shortcutKey("palette");
const HELP_KEYS = shortcutKeys("help");
const DETAILS_KEY = shortcutKey("details");
const CONSOLE_KEY = shortcutKey("console");
const BACK_KEY = shortcutKey("back");
const FORWARD_KEY = shortcutKey("forward");
const SETTINGS_KEY = shortcutKey("settings");
const IMPORT_KEY = shortcutKey("import");
/// Esc goes back only from a page opened on top of the main screens; on the mod list and the games page it keeps
/// clearing the selection and the search.
const ESC_BACK_VIEWS: ReadonlySet<string> = new Set(["app", "settings", "downloads", "notifications", "storage"]);

/// `MouseEvent.button` of the side buttons most mice have: 3 is back, 4 is forward.
const MOUSE_BACK = 3;
const MOUSE_FORWARD = 4;
const DIGIT = /^[1-9]$/;

/// The shortcuts that work from anywhere: the command palette, the shortcut list, the details column, the console, the
/// way back and forward (Alt+arrows and the mouse's side buttons), Gobbo settings, the game in a rail spot and import.
/// A field keeps its keys; a dialog or a menu handles its own. While the game editor is open it owns back and save,
/// because only it knows whether its draft has unsaved changes.
export function useGlobalShortcuts({ openPalette }: { openPalette: () => void }) {
	const { dialog } = useUiState();
	const { toggleInspector, toggleConsole, openDialog } = useUiActions();
	const navigation = useNavigation();
	const { target: editorTarget } = useGameEditor();
	const { managedGames } = useGames();
	const { gameId, selectGame, pickArchives } = useWorkspace();
	const latest = useRef({
		openPalette,
		toggleInspector,
		toggleConsole,
		openDialog,
		dialog,
		navigation,
		editorOpen: editorTarget !== null,
		managedGames,
		gameId,
		selectGame,
		pickArchives,
	});
	useEffect(() => {
		latest.current = {
			openPalette,
			toggleInspector,
			toggleConsole,
			openDialog,
			dialog,
			navigation,
			editorOpen: editorTarget !== null,
			managedGames,
			gameId,
			selectGame,
			pickArchives,
		};
	});

	useEffect(() => {
		/// Back and forward move through the screens, unless a dialog is open or the editor owns the way back.
		const move = (direction: "back" | "forward") => {
			const current = latest.current;

			if (current.dialog || current.editorOpen) return false;

			if (direction === "back" && current.navigation.canGoBack) current.navigation.back();
			else if (direction === "forward" && current.navigation.canGoForward) current.navigation.forward();
			else return false;
			return true;
		};

		/// Ctrl+1 is the first tile of the rail, Ctrl+9 the ninth, the same order the rail draws.
		const openGameAt = (digit: string) => {
			const game = latest.current.managedGames[Number(digit) - 1];

			if (!game || latest.current.editorOpen) return false;
			latest.current.selectGame(game.id);
			return true;
		};

		/// The Ctrl shortcuts outside a field. Each returns whether it acted, so an unhandled key keeps its default.
		const onCtrlKey = (event: KeyboardEvent, key: string): boolean => {
			const current = latest.current;

			if (key === DETAILS_KEY) void current.toggleInspector();
			else if (key === CONSOLE_KEY) current.toggleConsole();
			else if (key === SETTINGS_KEY) current.navigation.openAppSettings();
			else if (key === IMPORT_KEY && current.gameId !== undefined) void current.pickArchives();
			else if (!event.altKey && DIGIT.test(event.key)) return openGameAt(event.key);
			else return false;
			return true;
		};

		/// The keys without Ctrl outside a field: Esc and Alt+arrows go back and forward, ? and F1 open the list.
		const onPlainKey = (event: KeyboardEvent, key: string): boolean => {
			if (event.key === "Escape" && ESC_BACK_VIEWS.has(latest.current.navigation.view)) return move("back");
			if (event.altKey && event.key === BACK_KEY) return move("back");
			if (event.altKey && event.key === FORWARD_KEY) return move("forward");

			if (HELP_KEYS.includes(event.key) || HELP_KEYS.includes(key)) {
				latest.current.openDialog({ kind: "shortcuts" });
				return true;
			}

			return false;
		};

		const onKey = (event: KeyboardEvent) => {
			const target = event.target instanceof HTMLElement ? event.target : null;
			const ctrl = event.ctrlKey || event.metaKey;
			const key = event.key.toLowerCase();
			// A dialog owns the keyboard. The palette does not open above it: the user closes the dialog first.
			if (latest.current.dialog) return;

			if (ctrl && key === PALETTE_KEY) {
				event.preventDefault();
				latest.current.openPalette();
				return;
			}

			const typing = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target?.isContentEditable;

			if (typing || target?.closest("[role='dialog'], [role='alertdialog'], [role='menu']")) return;

			if (ctrl ? onCtrlKey(event, key) : onPlainKey(event, key)) event.preventDefault();
		};

		// The side buttons are read on mouseup, like a browser. Their mousedown is stopped too, so the webview never
		// treats them as its own history navigation.
		const onMouseDown = (event: MouseEvent) => {
			if (event.button === MOUSE_BACK || event.button === MOUSE_FORWARD) event.preventDefault();
		};
		const onMouseUp = (event: MouseEvent) => {
			if (event.button === MOUSE_BACK && move("back")) event.preventDefault();
			else if (event.button === MOUSE_FORWARD && move("forward")) event.preventDefault();
		};

		window.addEventListener("keydown", onKey);
		window.addEventListener("mousedown", onMouseDown);
		window.addEventListener("mouseup", onMouseUp);
		return () => {
			window.removeEventListener("keydown", onKey);
			window.removeEventListener("mousedown", onMouseDown);
			window.removeEventListener("mouseup", onMouseUp);
		};
	}, []);
}
