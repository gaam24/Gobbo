/// Languages the interface has translations for. English is the fallback.
export type Locale = "en" | "pl";

/// The stored setting: a fixed locale, or `system` to follow the browser or OS language list.
export type LanguageSetting = "system" | Locale;

/// Picks the locale for a setting. `system` takes the first language in the list that has a translation,
/// and English when none does.
export function resolveLocale(setting: LanguageSetting, languages: readonly string[] = browserLanguages()): Locale {
	if (setting !== "system") return setting;

	for (const tag of languages) {
		const base = (tag.split("-")[0] ?? tag).toLowerCase();

		if (base === "en" || base === "pl") return base;
	}

	return "en";
}

function browserLanguages(): readonly string[] {
	// Bun's test environment exposes a `navigator` without `languages`, while the DOM types say the list is always
	// present. Read both fields as optional so the test and a browser use the same path.
	const current = globalThis.navigator as { languages?: readonly string[]; language?: string } | undefined;

	if (!current) return [];

	if (current.languages?.length) return current.languages;
	return current.language ? [current.language] : [];
}
