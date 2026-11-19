import type { RefObject } from "react";
import type { ModGroup, UpdateReport } from "@/bindings";
import type { ListItem } from "@/lib/mods";
import type { ModRowActions } from "@/views/mod-table/mod-row";
import type { SeparatorActions } from "@/views/mod-table/separator-row";

/// The group operations a mod's context menu offers. One stable object is passed to every memoized row and card; the
/// lookup reads the newest status when the menu opens, so a status answer never changes a row's props.
export interface GroupMenuActions {
	/// The group this menu describes, or `null` for a mod outside a group.
	groupForMenu: (modId: string) => ModGroup | null;
	/// Opens the group editor with the group's members.
	onEditGroup: (groupId: string, members: string[]) => void;
	/// Removes the whole group, leaving its mods on the list.
	onUngroupMods: (groupId: string) => void;
	/// Adds the mod to the group, or takes it out when `groupId` is `null`.
	onSetModGroup: (modId: string, groupId: string | null) => void;
	/// Opens the translation winners dialog for a translation group.
	onSetTranslationWinner: (groupId: string) => void;
	/// The current selection, for a menu opened on one of several selected mods.
	groupSelection: () => readonly string[];
	/// Groups the selected mods when none of them belongs to a group yet.
	onGroupSelected: (ids: readonly string[]) => void;
}

/// Resolves the group a mod's menu describes. The group of the clicked mod always wins — even when the rest of the
/// selection belongs elsewhere. The selection only supplies a group when the clicked mod is not grouped itself.
export function menuGroupFor(modId: string, groups: readonly ModGroup[], selection: readonly string[]): ModGroup | null {
	const clicked = groups.find((group) => group.members.includes(modId)) ?? null;
	if (clicked) return clicked;
	if (selection.length <= 1 || !selection.includes(modId)) return null;
	return groups.find((group) => selection.every((id) => group.members.includes(id))) ?? null;
}

/// What the list views need, whether they draw a table or a grid of cards: the rows, the selection, and the actions of a
/// mod and of a separator. The views take the same props, so the list can switch between them.
export interface ModListProps extends ModRowActions, SeparatorActions {
	gameId: string;
	/// The element that scrolls the list; the table uses it to mount only the rows near the viewport.
	scrollRef?: RefObject<HTMLDivElement | null>;
	/// The rows to show: the mods after filters and search, with the separators while no filter is on.
	rows: ListItem[];
	/// User groups stored in the status. Passed to the group helpers so their headers render like page headers.
	groups: readonly ModGroup[];
	/// Renames a user group from its header menu.
	onRenameGroup: (id: string) => void;
	/// Removes the mods from a user group, leaving them in the list.
	onUngroup: (id: string) => void;
	/// Makes the translation of a translation group the winner of its shared files.
	onSetTranslationWinner: (id: string) => void;
	/// Separators whose groups are folded.
	collapsed: ReadonlySet<string>;
	/// The mods the user hid from this list, by id; their menus offer to show them again.
	hidden: ReadonlySet<string>;
	/// Page headers whose parts are folded.
	foldedPages: ReadonlySet<string>;
	/// The active profile, for preferences keyed to it.
	profileId: string;
	/// How many mods each separator heads, by separator id.
	groupSizes: ReadonlyMap<string, number>;
	/// Folds or unfolds one page header.
	onTogglePage: (id: string) => void;
	/// Opens the confirmation that deletes every installed part of a source page.
	onDeletePage: (pageId: string) => void;
	/// Switches one row on or off; a page or separator header is expanded by the backend.
	onSetEnabled: (id: string, enabled: boolean) => void | Promise<void>;
	/// Selected mod families. The last one is the focused row.
	selection: string[];
	/// A deployment is active: the list stays editable, but removing a mod from the library waits for the undo.
	/// Moving mods is only possible on the full list, without a filter or search.
	filtering: boolean;
	/// The update state of each mod, by id: its last saved check, or the failure of this session. A mod without one is
	/// "Not checked".
	updates: Record<string, UpdateReport>;
	libraryDir: string;
	archivesDir: string;
	locale: string;
	/// A row was dropped on another row: the dragged row moves above or below the target. The backend resolves groups.
	onReorder: (from: string, to: string, side: "above" | "below") => void;
	/// The separator made from a row's menu, whose name field opens when it appears.
	freshSeparator: string | null;
	/// The first and last mod in the full list, so a move can cross a separator at either end.
	firstModId: string | null;
	lastModId: string | null;
	/// Clears the search, the filter and the tag, for an empty result.
	onClearFilters: () => void;
}
