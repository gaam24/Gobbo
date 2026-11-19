import { useCallback, useEffect, useMemo, useState } from "react";
import type { ModGroup, Status } from "@/bindings";
import { foldedPageParts, type ListItem, listItems } from "@/lib/mods";
import { isTextList, readPref, writePref } from "@/lib/ui-prefs";

const NO_PAGES: ReadonlySet<string> = new Set();

/// The list without the hidden mods. A separator whose mods are all hidden leaves with them; a group that was empty in
/// the full list keeps its separator.
function withoutHidden(items: ListItem[], hidden: ReadonlySet<string>): ListItem[] {
	const result: ListItem[] = [];
	let header: ListItem | null = null;
	let members: ListItem[] = [];
	const flush = () => {
		const shown = members.filter((item) => !hidden.has(item.id));
		if (header && (members.length === 0 || shown.length > 0)) result.push(header);
		result.push(...shown);
	};
	for (const item of items) {
		if (item.kind === "separator") {
			flush();
			header = item;
			members = [];
		} else {
			members.push(item);
		}
	}
	flush();
	return result;
}

/// The list's rows: its separators and mods, the groups folded for this game, and the mods the user can walk with the arrows.
export function useListRows({
	gameId,
	profileId,
	mods,
	separators,
	groups,
	filtering,
	visible,
	hidden,
}: {
	gameId: string;
	/// Folds belong to one profile: separator ids repeat across profiles, so a shared key would fold unrelated groups.
	profileId: string;
	mods: Status["mods"];
	separators: Status["separators"];
	/// Stored user groups, so a folded header's row id (`group:<id>`) hides its parts as a page id does.
	groups: readonly ModGroup[];
	filtering: boolean;
	visible: Status["mods"];
	/// The mods the list hides. A filter shows them only through `visible`.
	hidden: ReadonlySet<string>;
}) {
	/// The list with its separators. Folded groups hide their mods.
	const items = useMemo(() => listItems(mods, separators), [mods, separators]);
	const foldedKey = `folded-groups.${gameId}.${profileId}`;
	const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set(readPref<string[]>(foldedKey, [], isTextList)));
	const pagesKey = `folded-pages.${gameId}.${profileId}`;
	const [foldedPages, setFoldedPages] = useState<ReadonlySet<string>>(() => new Set(readPref<string[]>(pagesKey, [], isTextList)));
	/// Separator ids that no longer exist are ignored and dropped on the next write; a reused id cannot fold a new group.
	const activeCollapsed = useMemo(
		() => new Set([...collapsed].filter((id) => separators.some((separator) => separator.id === id))),
		[collapsed, separators],
	);
	/// Folded groups are kept per game and profile, so the list opens the way it was left, also after a restart.
	useEffect(() => {
		writePref(foldedKey, activeCollapsed.size > 0 ? [...activeCollapsed] : null);
	}, [activeCollapsed, foldedKey]);
	useEffect(() => {
		writePref(pagesKey, foldedPages.size > 0 ? [...foldedPages] : null);
	}, [foldedPages, pagesKey]);
	/// A filter keeps the list's groups: only the matching mods stay, and a separator stays while its group has a match.
	/// Without a filter the hidden mods leave the list too.
	const matching = useMemo((): ListItem[] => {
		if (!filtering) return hidden.size > 0 ? withoutHidden(items, hidden) : items;
		const ids = new Set(visible.map((mod) => mod.id));
		const kept = items.filter((item) => item.kind === "separator" || ids.has(item.id));
		return kept.filter((item, index) => item.kind === "mod" || kept[index + 1]?.kind === "mod");
	}, [filtering, visible, items, hidden]);
	/// The mods under each separator; while a filter is on, the matches under it.
	const groupSizes = useMemo(() => {
		const sizes = new Map<string, number>();
		let current: string | null = null;
		for (const item of matching) {
			if (item.kind === "separator") current = item.id;
			else if (current !== null) sizes.set(current, (sizes.get(current) ?? 0) + 1);
		}
		return sizes;
	}, [matching]);
	const rows = useMemo((): ListItem[] => {
		const result: ListItem[] = [];
		let folded = false;
		for (const item of matching) {
			if (item.kind === "separator") folded = activeCollapsed.has(item.id);
			if (item.kind === "separator" || !folded) result.push(item);
		}
		return result;
	}, [matching, activeCollapsed]);
	/// A search shows every match, also a part of a folded page: the fold belongs to the unfiltered list, and hiding a
	/// match would leave the keyboard unable to reach a card the user can see.
	const hiddenPageParts = useMemo(
		() => (filtering ? NO_PAGES : foldedPageParts(mods, foldedPages, groups)),
		[filtering, groups, mods, foldedPages],
	);
	/// The mods the user can see and walk with the arrows, in order. A folded page hides its parts but keeps its header.
	const shown = useMemo(
		() => rows.flatMap((row) => (row.kind === "mod" && !hiddenPageParts.has(row.mod.id) ? [row.mod] : [])),
		[hiddenPageParts, rows],
	);

	const toggleGroup = useCallback(
		(id: string) =>
			setCollapsed((previous) => {
				const next = new Set(previous);
				if (!next.delete(id)) next.add(id);
				return next;
			}),
		[],
	);

	const foldAll = (fold: boolean) => setCollapsed(fold ? new Set(separators.map((separator) => separator.id)) : new Set());
	const togglePage = useCallback(
		(id: string) =>
			setFoldedPages((previous) => {
				const next = new Set(previous);
				if (!next.delete(id)) next.add(id);
				return next;
			}),
		[],
	);
	return {
		items,
		collapsed: activeCollapsed,
		groupSizes,
		rows,
		shown,
		toggleGroup,
		foldAll,
		foldedPages: filtering ? NO_PAGES : foldedPages,
		togglePage,
	};
}
