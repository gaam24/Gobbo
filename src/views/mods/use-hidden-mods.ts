import { useCallback, useEffect, useMemo, useState } from "react";
import type { ProfileView, Status } from "@/bindings";
import { hiddenModIds } from "@/lib/mods";
import { readPref, writePref } from "@/lib/ui-prefs";

type Choices = ReadonlyMap<string, boolean>;

/// A saved list of `[mod id, hide]` pairs.
function isChoiceList(value: unknown): value is Array<[string, boolean]> {
	return (
		Array.isArray(value) &&
		value.every((entry) => Array.isArray(entry) && entry.length === 2 && typeof entry[0] === "string" && typeof entry[1] === "boolean")
	);
}

/// The mods the list hides, with the user's own choices for this game and profile. A hidden mod is off here, so the list
/// drops it; an enabled mod is never hidden. A hand choice (hide, or show against the collection rule) is kept per game
/// and profile, like the folds.
export function useHiddenMods({
	gameId,
	profileId,
	mods,
	profiles,
}: {
	gameId: string;
	profileId: string;
	mods: Status["mods"];
	profiles: ProfileView[];
}) {
	const key = `hidden-mods.${gameId}.${profileId}`;
	/// The choices saved for this key. A switch of game or profile reads the key's own list again.
	const saved = useMemo((): Choices => new Map(readPref<Array<[string, boolean]>>(key, [], isChoiceList)), [key]);
	/// The edits made since the page opened, tagged with their key so an edit of one profile never leaks into another.
	const [edits, setEdits] = useState<{ key: string; choices: Choices } | null>(null);
	const stored = edits?.key === key ? edits.choices : saved;
	/// Choices for mods that no longer exist are ignored and dropped on the next write; a reused id cannot hide a new mod.
	const choices = useMemo((): Choices => new Map([...stored].filter(([id]) => mods.some((mod) => mod.id === id))), [stored, mods]);
	useEffect(() => {
		writePref(key, choices.size > 0 ? [...choices] : null);
	}, [choices, key]);
	const hidden = useMemo(() => hiddenModIds(mods, profiles, choices), [mods, profiles, choices]);

	/// Hides or shows the given mods by hand. Stable, because the memoized rows receive it.
	const setHidden = useCallback(
		(ids: string[], hide: boolean) =>
			setEdits((previous) => {
				const next = new Map(previous?.key === key ? previous.choices : saved);
				for (const id of ids) next.set(id, hide);
				return { key, choices: next };
			}),
		[key, saved],
	);

	return { hidden, setHidden };
}
