import { useState } from "react";
import { readPref, writePref } from "@/lib/ui-prefs";

/// How the mod list is drawn: a table of rows, or a grid of cards. The data, the separators and the actions are the same.
export type ModView = "list" | "grid";

const KEY = "mods-view";
const isModView = (value: unknown): value is ModView => value === "list" || value === "grid";

/// The list or grid choice of this window. It is kept in the webview's storage like the column widths; losing it only
/// brings the table back.
export function useModView(): [ModView, (view: ModView) => void] {
	const [view, setViewState] = useState<ModView>(() => readPref<ModView>(KEY, "list", isModView));
	const setView = (next: ModView) => {
		writePref(KEY, next);
		setViewState(next);
	};
	return [view, setView];
}
