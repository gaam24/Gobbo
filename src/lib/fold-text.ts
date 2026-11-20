const DIACRITICS = /\p{Diacritic}/gu;

/// Lower case without accents, so "zrodlo" finds "Źródło". "ł" is a letter of its own, not an accented "l". Every
/// search in the window folds both the query and the text with this rule.
export function foldText(text: string): string {
	return text.normalize("NFD").replace(DIACRITICS, "").toLowerCase().replaceAll("ł", "l");
}
