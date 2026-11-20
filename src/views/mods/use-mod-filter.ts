import { useDeferredValue, useMemo, useState } from "react";
import type { Status, UpdateReport } from "@/bindings";
import { useTranslation } from "@/i18n";
import { foldText } from "@/lib/fold-text";
import { type Filter, matchesNeedle, passesFilter, tagsOf } from "@/lib/mods";

/// The search, the status filter and the tag filter of the mod list, with the counts the filter buttons show. The hidden
/// mods leave the list and its counts; the Hidden filter shows them alone.
export function useModFilter({
	mods,
	updates,
	hidden,
}: {
	mods: Status["mods"];
	updates: Record<string, UpdateReport>;
	hidden: ReadonlySet<string>;
}) {
	const { i18n } = useTranslation();
	const language = i18n.language;
	const [filter, setFilter] = useState<Filter>("all");
	const [query, setQuery] = useState("");
	const [tagFilter, setTagFilter] = useState<string | null>(null);

	/// Tag list, filter results, and filter counts come from the whole profile. Memoized, because they
	/// walk every mod and the search field re-renders this view on each keystroke.
	const allTags = useMemo(() => tagsOf(mods, language), [mods, language]);
	const filtering = filter !== "all" || query.trim() !== "" || tagFilter !== null;
	/// The field shows every keystroke at once; the list catches up in a lower-priority render, so typing stays smooth
	/// even when the filter has to walk 2 000 mods.
	const deferredQuery = useDeferredValue(query);
	const needle = foldText(deferredQuery.trim());
	const visible = useMemo(
		() => mods.filter((mod) => hidden.has(mod.id) === (filter === "hidden") && matchesNeedle(mod, filter, needle, tagFilter, updates)),
		[mods, filter, needle, tagFilter, updates, hidden],
	);

	const counts = useMemo(() => {
		const all: Record<Filter, number> = {
			all: mods.length - hidden.size,
			enabled: 0,
			disabled: 0,
			missing: 0,
			conflict: 0,
			updates: 0,
			hidden: hidden.size,
		};

		const counted = ["enabled", "disabled", "missing", "conflict", "updates"] as const;

		for (const mod of mods) {
			if (hidden.has(mod.id)) continue;

			for (const chip of counted) {
				if (passesFilter(mod, chip, updates)) all[chip] += 1;
			}
		}

		return all;
	}, [mods, updates, hidden]);

	/// An empty result offers the way back to the whole list.
	const clearFilters = () => {
		setQuery("");
		setFilter("all");
		setTagFilter(null);
	};

	return { filter, setFilter, query, setQuery, tagFilter, setTagFilter, filtering, allTags, visible, counts, clearFilters };
}
