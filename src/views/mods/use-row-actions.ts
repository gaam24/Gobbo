import { type MouseEvent as ReactMouseEvent, useCallback, useEffect, useRef, useState } from "react";
import type { DetailsTab } from "@/app/contexts/ui-context";
import type { Status } from "@/bindings";
import { useTranslation } from "@/i18n";

/// What the user does to the rows and separators of the list: toggling, moving, naming and folding them, and picking mods.
/// The list edits are commands now; the backend owns the group and order rules and answers with the fresh status.
export function useRowActions({
	separators,
	selection,
	focused,
	shown,
	onSetEnabled,
	onMoveMod,
	onSetSeparator,
	onSortList,
	onSelect,
	onShowDetails,
}: {
	separators: Status["separators"];
	selection: string[];
	focused: string | null;
	shown: Status["mods"];
	/// Enables or disables one row. A group or separator row id is expanded by the backend.
	onSetEnabled: (id: string, enabled: boolean) => void | Promise<void>;
	/// Moves a row above or below another; the backend resolves page and user-group blocks.
	onMoveMod: (id: string, target: string, side: "above" | "below") => void | Promise<void>;
	/// Renames a separator, creates one, or removes it with an empty name.
	onSetSeparator: (id: string, name: string) => void | Promise<void>;
	onSortList: () => void | Promise<void>;
	onSelect: (folders: string[]) => void;
	onShowDetails: (tab?: DetailsTab) => void;
}) {
	const { t } = useTranslation();
	/// The latest selection and visible rows, read by handlers without putting them in their dependencies. A click changes
	/// `selection`, and a search changes `shown`; a new callback each time made every memoized row render again.
	const latest = useRef({ selection, focused, shown });
	useEffect(() => {
		latest.current = { selection, focused, shown };
	}, [selection, focused, shown]);

	const toggle = useCallback((modId: string, enabled: boolean) => void onSetEnabled(modId, enabled), [onSetEnabled]);

	/// One step up or down in the visible list: the backend checks whether the move is allowed inside a group.
	const move = useCallback(
		(modId: string, delta: number) => {
			const list = latest.current.shown;
			const at = list.findIndex((mod) => mod.id === modId);
			const target = list[at + delta];

			if (at < 0 || !target) return;
			void onMoveMod(modId, target.id, delta < 0 ? "above" : "below");
		},
		[onMoveMod],
	);

	/// Drag and drop: the view reports the dragged row, the row under the pointer and the side of the drop; the backend
	/// moves a whole page or user-group header as one block and keeps a part inside its group.
	const reorder = useCallback((from: string, to: string, side: "above" | "below") => void onMoveMod(from, to, side), [onMoveMod]);

	/// Separators: renaming, removing and switching a group on or off.
	const renameSeparator = useCallback((id: string, name: string) => onSetSeparator(id, name), [onSetSeparator]);
	const removeSeparator = useCallback((id: string) => onSetSeparator(id, ""), [onSetSeparator]);
	const groupEnabled = useCallback((id: string, enabled: boolean) => onSetEnabled(id, enabled), [onSetEnabled]);

	/// A new separator goes above the focused mod, or at the top of the list when the id is empty.
	const addSeparator = (name: string) => onSetSeparator(focused ?? "", name);
	/// From a row's menu: a separator with a plain name goes above the mod, and its name field opens once it is there.
	/// The separators that were there before tell the new one apart.
	/// `id` is the mod the separator is placed above; the backend gives the new separator its own id.
	const [naming, setNaming] = useState<{ before: string | null; known: ReadonlySet<string> } | null>(null);
	const freshSeparator =
		naming === null
			? null
			: (separators.find((separator) => !naming.known.has(separator.id) && (separator.before ?? null) === naming.before)?.id ?? null);
	const addSeparatorAbove = useCallback(
		(modId: string) => {
			setNaming({ before: modId, known: new Set(separators.map((separator) => separator.id)) });
			void onSetSeparator(modId, t("separators.newName"));
		},
		[onSetSeparator, separators, t],
	);
	const addSeparatorAtEnd = () => {
		setNaming({ before: null, known: new Set(separators.map((separator) => separator.id)) });
		void onSetSeparator("", t("separators.newName"));
	};
	/// The list's own menu, on its empty part: the rows and the header have menus of their own. The action waits until
	/// the menu has closed, so a new separator's name field can take the focus.
	const afterListMenu = useRef<(() => void) | null>(null);
	const deferAfterListMenu = (action: () => void) => {
		afterListMenu.current = action;
	};
	const takeAfterListMenu = () => {
		const action = afterListMenu.current;
		afterListMenu.current = null;
		return action;
	};
	const renameDone = useCallback(() => setNaming(null), []);

	const showDetailsOf = useCallback(
		(modId: string) => {
			onSelect([modId]);
			onShowDetails();
		},
		[onSelect, onShowDetails],
	);

	/// Opens the conflicts tab of the same mod, from the counter in the row.
	const showConflictsOf = useCallback(
		(modId: string) => {
			onSelect([modId]);
			onShowDetails("conflicts");
		},
		[onSelect, onShowDetails],
	);

	/// Clicking selects one mod family. Ctrl adds or removes one, and Shift selects the range from the focused mod.
	const pick = useCallback(
		(modId: string, event: ReactMouseEvent) => {
			const current = latest.current;

			if (event.ctrlKey || event.metaKey) {
				onSelect(current.selection.includes(modId) ? current.selection.filter((item) => item !== modId) : [...current.selection, modId]);
			} else if (event.shiftKey && current.focused) {
				const from = current.shown.findIndex((mod) => mod.id === current.focused);
				const to = current.shown.findIndex((mod) => mod.id === modId);

				if (from >= 0 && to >= 0) {
					const [lo, hi] = from < to ? [from, to] : [to, from];
					onSelect(current.shown.slice(lo, hi + 1).map((mod) => mod.id));
				} else {
					// The focused row is filtered or folded away; the click still selects the row under the pointer.
					onSelect([modId]);
				}
			} else {
				onSelect([modId]);
			}
		},
		[onSelect],
	);

	/// The backend sorts each separator group on its own, as the old list rule did; it keeps page and user groups whole.
	const sortByName = useCallback(() => onSortList(), [onSortList]);
	const sortGroup = useCallback(() => onSortList(), [onSortList]);
	return {
		toggle,
		move,
		reorder,
		renameSeparator,
		removeSeparator,
		groupEnabled,
		addSeparator,
		freshSeparator,
		addSeparatorAbove,
		addSeparatorAtEnd,
		deferAfterListMenu,
		takeAfterListMenu,
		renameDone,
		showDetailsOf,
		showConflictsOf,
		pick,
		sortByName,
		sortGroup,
	};
}
