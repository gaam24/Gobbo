import { useEffect, useRef } from "react";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { shortcutKey, shortcutKeys } from "@/lib/shortcuts";

const PALETTE_KEY = shortcutKey("palette");
const HELP_KEYS = shortcutKeys("help");
const DETAILS_KEY = shortcutKey("details");
const CONSOLE_KEY = shortcutKey("console");

/// The shortcuts that work from anywhere: the command palette, the shortcut list, the details column and the console.
/// A field keeps its keys; a dialog or a menu handles its own.
export function useGlobalShortcuts({ openPalette }: { openPalette: () => void }) {
	const { dialog } = useUiState();
	const { toggleInspector, toggleConsole, openDialog } = useUiActions();
	const latest = useRef({
		openPalette,
		toggleInspector,
		toggleConsole,
		openDialog,
		dialog,
	});
	useEffect(() => {
		latest.current = {
			openPalette,
			toggleInspector,
			toggleConsole,
			openDialog,
			dialog,
		};
	});

	useEffect(() => {
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

			if (HELP_KEYS.includes(event.key) || HELP_KEYS.includes(key)) latest.current.openDialog({ kind: "shortcuts" });
			else if (ctrl && key === DETAILS_KEY) void latest.current.toggleInspector();
			else if (ctrl && key === CONSOLE_KEY) latest.current.toggleConsole();
			else return;
			event.preventDefault();
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, []);
}
