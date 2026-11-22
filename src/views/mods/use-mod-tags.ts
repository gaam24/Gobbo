import { useMemo } from "react";
import type { Status } from "@/bindings";
import { useTranslation } from "@/i18n";
import { tagsOf } from "@/lib/mods";

/// The distinct tags of the whole profile, for the filter chips and the inspector tag editor. Walking every mod on
/// each keystroke is expensive, so the list is memoized.
export function useModTags(mods: Status["mods"]): string[] {
	const { i18n } = useTranslation();
	return useMemo(() => tagsOf(mods, i18n.language), [mods, i18n.language]);
}
