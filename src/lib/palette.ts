import type { LucideIcon } from "lucide-react";

/// The command palette's items and how a query finds them.

type PaletteGroup = "actions" | "go" | "mods" | "profiles" | "games";

/// One thing the palette can do: an action, a place, a mod, a profile or a game.
export interface PaletteItem {
	id: string;
	group: PaletteGroup;
	label: string;
	/// More words that find the item, such as the mod's author or the action's other name.
	keywords?: string;
	/// Shown on the right: the item's shortcut or its state.
	hint?: string;
	icon?: LucideIcon;
	disabled?: boolean;
	/// Why a disabled item cannot run. The palette shows it next to the item, so the reason does not live in a title.
	reason?: string;
	run: () => void | Promise<void>;
}

export const PALETTE_GROUPS: PaletteGroup[] = ["actions", "go", "mods", "profiles", "games"];
/// Without a query only these groups show: the mods appear once the user types.
const IDLE_GROUPS = new Set<PaletteGroup>(["actions", "go"]);
const MAX_RESULTS = 60;
const DIACRITICS = /\p{Diacritic}/gu;
const WORD_BREAK = /[\s\-_/]+/;
const SPACES = /\s+/;

/// Lower case without accents, so "zrodlo" finds "Źródło". "ł" is a letter of its own, not an accented "l".
function plain(text: string): string {
	return text.normalize("NFD").replace(DIACRITICS, "").toLowerCase().replaceAll("ł", "l");
}

/// The normalized label and search text of an item, built once. The palette keeps the same item objects while the
/// status does not change, so typing does not run `normalize("NFD")` over every mod again.
const SEARCH_TEXT = new WeakMap<PaletteItem, { label: string; text: string }>();

function searchText(item: PaletteItem): { label: string; text: string } {
	let found = SEARCH_TEXT.get(item);

	if (!found) {
		const label = plain(item.label);
		found = { label, text: `${label} ${plain(item.keywords ?? "")}` };
		SEARCH_TEXT.set(item, found);
	}

	return found;
}

/// How well an item matches: 0 when its label starts with the query, 1 when a word does, 2 when every word of the
/// query is somewhere in it, `null` when not.
function rank(item: PaletteItem, words: string[]): 0 | 1 | 2 | null {
	const { label, text } = searchText(item);

	if (!words.every((word) => text.includes(word))) return null;
	const query = words.join(" ");

	if (label.startsWith(query)) return 0;

	if (label.split(WORD_BREAK).some((word) => word.startsWith(words[0] ?? ""))) return 1;
	return 2;
}

/// The items to show for a query, best first within each group, and the groups in a fixed order.
export function paletteResults(items: PaletteItem[], query: string): PaletteItem[] {
	const words = plain(query).split(SPACES).filter(Boolean);

	if (words.length === 0) return items.filter((item) => IDLE_GROUPS.has(item.group));
	const ranked = items.flatMap((item) => {
		const score = rank(item, words);
		return score === null ? [] : [{ item, score }];
	});
	return PALETTE_GROUPS.flatMap((group) =>
		ranked
			.filter((entry) => entry.item.group === group)
			.toSorted((a, b) => a.score - b.score)
			.map((entry) => entry.item),
	).slice(0, MAX_RESULTS);
}
