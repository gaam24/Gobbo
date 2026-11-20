import { useMemo, useSyncExternalStore } from "react";
import en from "./locales/en";
import type { LanguageSetting, Locale } from "./locales/locale";
import { resolveLocale } from "./locales/locale";

/// The plural forms the locale files name (`title_one`, `title_few`, ...), the CLDR categories.
const PLURAL_FORMS = ["zero", "one", "two", "few", "many", "other"] as const;
type PluralForm = (typeof PLURAL_FORMS)[number];

/// Every dotted key of the English bundle, such as `mods.info.size`.
type Leaves<T, Prefix extends string = ""> = {
	[K in keyof T & string]: T[K] extends string ? `${Prefix}${K}` : T[K] extends object ? Leaves<T[K], `${Prefix}${K}.`> : never;
}[keyof T & string];

/// The base key of a plural message: `gameChangesSummary` for `gameChangesSummary_one`. Keys have one underscore at most
/// before the form, so the split is unambiguous.
type PluralBase<K extends string> = K extends `${infer Base}_${infer Form}` ? (Form extends PluralForm ? Base : never) : never;

/// The keys `translateKey` accepts. A typo is a type error, and a plural message is named by its base key.
export type TranslationKey = Leaves<typeof en> | PluralBase<Leaves<typeof en>>;

/// What a message can be filled with. `count` also picks the plural form.
export type TranslationValues = Readonly<Record<string, string | number | boolean | null | undefined>>;

interface Tree {
	readonly [key: string]: string | Tree;
}

type Messages = Partial<Record<string, string>>;

/// Turns a nested bundle (`mods.info.size`) into one table keyed by the dotted path. Built once per language.
function flatten(tree: Tree, prefix = "", into: Record<string, string> = {}): Record<string, string> {
	for (const [name, value] of Object.entries(tree)) {
		const path = `${prefix}${name}`;

		if (typeof value === "string") into[path] = value;
		else flatten(value, `${path}.`, into);
	}

	return into;
}

// English is inline, so the first render already has text. The Polish bundle is loaded on demand, so it stays out of
// the initial chunk unless it is the language the system asks for.
const initialLocale = resolveLocale("system");
const messages: Partial<Record<Locale, Messages>> = { en: flatten(en) };

if (initialLocale === "pl") messages.pl = flatten((await import("./locales/pl")).default);

let language: Locale = initialLocale;
const listeners = new Set<() => void>();

if (typeof document !== "undefined") document.documentElement.lang = language;

const pluralRules = new Map<Locale, Intl.PluralRules>();
function pluralFormOf(locale: Locale, count: number): string {
	let rules = pluralRules.get(locale);

	if (!rules) {
		rules = new Intl.PluralRules(locale);
		pluralRules.set(locale, rules);
	}

	return rules.select(count);
}

/// The text of a key in one language. A plural message picks its form for `count` and falls back to `_other`.
function messageIn(locale: Locale, key: string, count?: number): string | undefined {
	const table = messages[locale];

	if (!table) return undefined;

	if (count !== undefined) {
		const plural = table[`${key}_${pluralFormOf(locale, count)}`] ?? table[`${key}_other`];

		if (plural !== undefined) return plural;
	}

	return table[key];
}

const PLACEHOLDER = /\{\{\s*(\w+)\s*\}\}/g;

/// Fills `{{name}}` from the values. A placeholder without a value stays visible, so a missing value is easy to spot.
function interpolate(template: string, values: TranslationValues): string {
	return template.replace(PLACEHOLDER, (placeholder, name: string) => {
		const value = values[name];
		return value === undefined || value === null ? placeholder : String(value);
	});
}

/// The text for a key in one language, then in English. A key with no text shows the key itself.
function renderIn(locale: Locale, key: string, values: TranslationValues): string {
	const count = typeof values.count === "number" ? values.count : undefined;
	const template = messageIn(locale, key, count) ?? messageIn("en", key, count);
	return template === undefined ? key : interpolate(template, values);
}

/// The text for a key in the current language.
function render(key: string, values: TranslationValues): string {
	return renderIn(language, key, values);
}

/// Translates a key from the source bundle. The typed form: a key that does not exist is a type error.
export function translateKey(key: TranslationKey, values: TranslationValues = {}): string {
	return render(key, values);
}

/// Translates a key built at run time, such as a backend message code. Check `hasTranslation` first when a missing key
/// should not show the key itself.
export function translateDynamic(key: string, values: TranslationValues = {}): string {
	return render(key, values);
}

/// Whether the key, or one of its plural forms, has text in the current language or in English.
export function hasTranslation(key: string): boolean {
	return [key, `${key}_one`, `${key}_other`].some(
		(candidate) => messageIn(language, candidate) !== undefined || messageIn("en", candidate) !== undefined,
	);
}

function subscribe(listener: () => void): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

function currentLanguage(): Locale {
	return language;
}

/// The translation hook. `t` translates a key; `i18n.language` is the current locale, for formatting dates and numbers.
/// A component that reads it re-renders when the language changes.
export function useTranslation(): {
	t: typeof translateKey;
	i18n: { language: Locale };
} {
	const current = useSyncExternalStore(subscribe, currentLanguage, currentLanguage);
	// React Compiler memoizes `t("key")` by the identity of `t`. A stable `t` would keep the strings of the previous
	// language after a switch, so the hook returns a function bound to the current locale.
	const t = useMemo(
		() =>
			(key: TranslationKey, values: TranslationValues = {}): string =>
				renderIn(current, key, values),
		[current],
	);
	return { t, i18n: { language: current } };
}

/// Switches the interface language and the document language. Components re-render on their own.
export async function applyLanguage(setting: LanguageSetting): Promise<void> {
	const locale: Locale = resolveLocale(setting);

	if (locale === "pl" && !messages.pl) messages.pl = flatten((await import("./locales/pl")).default);

	if (typeof document !== "undefined") document.documentElement.lang = locale;
	language = locale;

	for (const listener of listeners) listener();
}
