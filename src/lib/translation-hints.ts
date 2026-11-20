import type { ModDetails, ModGroup, ModView } from "@/bindings";

/** A canonical language code and the words that name it in a mod title. Universal data, not game logic. */
export interface LanguageTokens {
	code: string;
	tokens: readonly string[];
}

/** Lowercase codes and aliases, including the native spelling of each language. */
export const languageTokens: readonly LanguageTokens[] = [
	{ code: "pl", tokens: ["pl", "pl-pl", "polski", "polish"] },
	{ code: "de", tokens: ["de", "de-de", "deutsch", "german"] },
	{ code: "fr", tokens: ["fr", "fr-fr", "français", "french"] },
	{ code: "es", tokens: ["es", "es-es", "español", "spanish"] },
	{ code: "it", tokens: ["it", "it-it", "italiano", "italian"] },
	{ code: "ru", tokens: ["ru", "ru-ru", "русский", "russian"] },
	{ code: "zh", tokens: ["zh", "zh-cn", "中文", "chinese"] },
	{ code: "ja", tokens: ["ja", "ja-jp", "日本語", "japanese"] },
	{ code: "ko", tokens: ["ko", "ko-kr", "한국어", "korean"] },
	{ code: "tr", tokens: ["tr", "tr-tr", "türkçe", "turkish"] },
];

export interface TranslationHint {
	baseId: string;
	translationId: string;
	language: string | null;
	confidence: number;
}

// Filler words carry no mod identity, so they cannot stand for the base mod by themselves.
const stopWords = new Set(["the", "and", "for", "with", "mod", "pack", "version", "translation", "translate"]);

// A handful is enough for a hint list the user still has to confirm.
const MAX_HINTS = 8;
const LANGUAGE_WEIGHT = 0.55;
const NAME_WEIGHT = 0.4;

const combiningMarks = /[\u0300-\u036f]/g;
const nonWord = /[^\p{L}\p{N}]+/u;

/** Lowercase, strip diacritics and split on anything that is not a Unicode letter or digit. */
function wordsOf(value: string): string[] {
	return value.normalize("NFD").replace(combiningMarks, "").toLowerCase().split(nonWord).filter(Boolean);
}

/** Whole-token match: `needle` must appear as a consecutive run of words in `haystack`. */
function containsSequence(haystack: readonly string[], needle: readonly string[]): boolean {
	if (needle.length === 0 || needle.length > haystack.length) return false;

	for (let start = 0; start <= haystack.length - needle.length; start += 1) {
		let matched = true;

		for (let offset = 0; offset < needle.length; offset += 1) {
			if (haystack[start + offset] !== needle[offset]) {
				matched = false;
				break;
			}
		}

		if (matched) return true;
	}

	return false;
}

/** The first matching language in table order, so a text naming several is deterministic. */
export function detectLanguage(text: string): string | null {
	const words = wordsOf(text);

	for (const entry of languageTokens) {
		for (const token of entry.tokens) {
			if (containsSequence(words, wordsOf(token))) return entry.code;
		}
	}

	return null;
}

/** The name and the small bit of source metadata a list row actually carries. */
function searchableText(mod: ModView): string {
	return [mod.info.name, ...mod.tags, mod.part_name ?? "", mod.page_name ?? "", mod.source?.name ?? ""].join(" ");
}

/** 1 for the full base name, 0.6 for one meaningful word of it, 0 for no reference. */
function nameReference(base: ModView, translation: ModView): 0 | 0.6 | 1 {
	const baseWords = wordsOf(base.info.name);
	const candidateWords = wordsOf(searchableText(translation));

	if (baseWords.length === 0) return 0;

	if (containsSequence(candidateWords, baseWords)) return 1;
	const meaningful = baseWords.filter((word) => word.length >= 3 && !stopWords.has(word));
	return meaningful.some((word) => candidateWords.includes(word)) ? 0.6 : 0;
}

/** Translations usually live on their own provider page; the same page only lowers the score. */
function pageScore(base: ModView, translation: ModView): -0.2 | 0 | 0.15 {
	const basePage = base.page_id ?? null;
	const translationPage = translation.page_id ?? null;

	if (basePage === null || translationPage === null) return 0;
	return basePage === translationPage ? -0.2 : 0.15;
}

/**
 * Hints for base + translation pairs. A mod with a language word in its name is a candidate translation of a
 * language-neutral mod whose name it repeats. Nothing here mutates the list or talks to the backend.
 */
export function suggestTranslationPairs(mods: ModView[], existingGroups: ModGroup[]): TranslationHint[] {
	const grouped = new Set<string>();

	for (const group of existingGroups) {
		for (const member of group.members) grouped.add(member);
	}

	const pool = mods.filter((mod) => !grouped.has(mod.id));
	const hints: TranslationHint[] = [];

	for (const base of pool) {
		if (detectLanguage(base.info.name) !== null) continue;

		for (const translation of pool) {
			if (base.id === translation.id) continue;
			const language = detectLanguage(searchableText(translation));

			if (language === null) continue;
			const reference = nameReference(base, translation);

			if (reference === 0) continue;
			const raw = LANGUAGE_WEIGHT + NAME_WEIGHT * reference + pageScore(base, translation);
			const confidence = Math.round(Math.min(1, Math.max(0, raw)) * 100) / 100;
			hints.push({
				baseId: base.id,
				translationId: translation.id,
				language,
				confidence,
			});
		}
	}

	hints.sort((a, b) => {
		if (a.confidence !== b.confidence) return b.confidence - a.confidence;

		if (a.baseId !== b.baseId) return a.baseId < b.baseId ? -1 : 1;

		if (a.translationId !== b.translationId) return a.translationId < b.translationId ? -1 : 1;
		return 0;
	});
	return hints.slice(0, MAX_HINTS);
}

/** Conflict path keys present in both mods, in base order, without duplicates. */
export function sharedConflictPaths(base: ModDetails, translation: ModDetails): string[] {
	const otherPaths = new Set(translation.conflicts.map((conflict) => conflict.path));
	const seen = new Set<string>();
	const shared: string[] = [];

	for (const conflict of base.conflicts) {
		const path = conflict.path;

		if (otherPaths.has(path) && !seen.has(path)) {
			seen.add(path);
			shared.push(path);
		}
	}

	return shared;
}
