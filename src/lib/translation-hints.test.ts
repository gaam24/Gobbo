import { expect, test } from "bun:test";

import type { ModDetails, ModGroup, ModView_Serialize } from "@/bindings";
import { detectLanguage, languageTokens, sharedConflictPaths, suggestTranslationPairs } from "@/lib/translation-hints";

interface ModSeed {
	tags?: string[];
	pageId?: string | null;
	enabled?: boolean;
}

// A view fixture with safe defaults; a test names only the fields it changes.
const mod = (id: string, name: string, seed: ModSeed = {}): ModView_Serialize => ({
	id,
	folder: id,
	fomod_installer: false,
	version_count: 1,
	installed_at: 0,
	slot: "",
	enabled: seed.enabled ?? true,
	priority: 1,
	load_priority: null,
	files: 1,
	size_kb: 1,
	wins: 0,
	loses: 0,
	archive: null,
	archive_file: null,
	skipped: 0,
	info: { name, version: null, author: null, description: null },
	source: null,
	sources: [],
	page_id: seed.pageId ?? null,
	page_name: null,
	part_id: null,
	part_name: null,
	part_category: null,
	update: null,
	tags: seed.tags ?? [],
	instructions: "",
	unmet_requirements: 0,
	requires_names: [],
	requirement_groups: [],
	profiles: [],
	collections: [],
});

const group = (...members: string[]): ModGroup => ({
	id: `group-${members.join("-")}`,
	name: "Group",
	members,
	kind: "bundle",
	language: null,
});

const details = (paths: string[]): ModDetails => ({
	folder: "base-mod",
	info: { name: "base-mod", version: null, author: null, description: null },
	sources: [],
	versions: [],
	files: [],
	conflicts: paths.map((path) => ({ path, winner: "base-mod", losers: [] })),
	steps: { defined: [], before_start: [], after_end: [] },
	file_winners: false,
	extras: {
		folder: "base-mod",
		info_description: null,
		source: null,
		notes: "",
		requirements: [],
		source_dlcs: [],
		source_sync: [],
		fomod: [],
		fomod_installer: false,
		requires: [],
		incompatible: [],
		incompatible_names: [],
		overrides: [],
		overrides_names: [],
		steps_before: [],
		steps_after: [],
	},
});

const pair = (basePage: string | null, translationPage: string | null) =>
	suggestTranslationPairs(
		[mod("base-mod", "base-mod", { pageId: basePage }), mod("base-mod-pl", "base-mod-pl", { pageId: translationPage })],
		[],
	)[0];

test("a language word matches whole tokens, never a substring", () => {
	expect(detectLanguage("base-mod-pl")).toBe("pl");
	expect(detectLanguage("explosion")).toBeNull();
	expect(detectLanguage("an exploding planet")).toBeNull();
	expect(detectLanguage("a debris field")).toBeNull();
});

test("case and diacritics do not change detection", () => {
	expect(detectLanguage("FRANÇAIS")).toBe("fr");
	expect(detectLanguage("TÜRKÇE")).toBe("tr");
	expect(detectLanguage("Español")).toBe("es");
	expect(detectLanguage("Polski")).toBe("pl");
	expect(detectLanguage("日本語")).toBe("ja");
});

test("every token in the table detects its own language family", () => {
	for (const entry of languageTokens) {
		for (const token of entry.tokens) {
			expect(detectLanguage(token)).toBe(entry.code);
		}
	}
});

test("a text naming several languages returns the first in table order", () => {
	expect(detectLanguage("base-mod-de-pl")).toBe("pl");
	expect(detectLanguage("base-mod-zh-ja")).toBe("zh");
});

test("a translation is paired with the base whose name it repeats", () => {
	const hints = suggestTranslationPairs([mod("base-mod", "base-mod"), mod("base-mod-pl", "base-mod-pl")], []);
	expect(hints).toEqual([
		{
			baseId: "base-mod",
			translationId: "base-mod-pl",
			language: "pl",
			confidence: 0.95,
		},
	]);
});

test("a meaningful word of the base name is enough, but scores below the full name", () => {
	const byWord = suggestTranslationPairs([mod("sky-overhaul", "Sky UI Overhaul"), mod("sky-pl", "Sky Polish")], []);
	expect(byWord).toHaveLength(1);
	expect(byWord[0]?.language).toBe("pl");
	expect(byWord[0]?.confidence).toBeCloseTo(0.79);

	const byFullName = suggestTranslationPairs([mod("sky", "Sky"), mod("sky-pl", "Sky Polish")], []);
	expect(byFullName[0]?.confidence).toBeGreaterThan(byWord[0]?.confidence ?? 0);
});

test("a language word in tags is a signal too", () => {
	const hints = suggestTranslationPairs([mod("base-mod", "base-mod"), mod("base-mod-tr", "base-mod-x", { tags: ["Türkçe"] })], []);
	expect(hints).toEqual([
		{
			baseId: "base-mod",
			translationId: "base-mod-tr",
			language: "tr",
			confidence: 0.95,
		},
	]);
});

test("a different provider page raises the score and the same page lowers it", () => {
	const apart = pair("nexus:11", "nexus:12");
	const samePage = pair("nexus:11", "nexus:11");

	expect(apart?.confidence).toBe(1);
	expect(samePage?.confidence).toBeCloseTo(0.75);
	expect((apart?.confidence ?? 0) > (samePage?.confidence ?? 0)).toBe(true);
});

test("mods already in a group are skipped as base and as translation", () => {
	const mods = [mod("base-mod", "base-mod"), mod("base-mod-pl", "base-mod-pl")];
	expect(suggestTranslationPairs(mods, [group("base-mod-pl")])).toEqual([]);
	expect(suggestTranslationPairs(mods, [group("base-mod")])).toEqual([]);
	expect(suggestTranslationPairs(mods, [group("unrelated-mod")])).toHaveLength(1);
});

test("disabled mods are still suggested", () => {
	const mods = [mod("base-mod", "base-mod", { enabled: false }), mod("base-mod-pl", "base-mod-pl", { enabled: false })];
	expect(suggestTranslationPairs(mods, [])).toHaveLength(1);
});

test("hints sort by confidence, then by family id", () => {
	const mods = [
		mod("z-base", "zebra"),
		mod("z-base-pl", "zebra-pl"),
		mod("a-base", "alpha"),
		mod("a-base-pl", "alpha-pl"),
		mod("m-base", "mango", { pageId: "nexus:1" }),
		mod("m-base-pl", "mango-pl", { pageId: "nexus:2" }),
	];
	expect(suggestTranslationPairs(mods, []).map((hint) => hint.baseId)).toEqual(["m-base", "a-base", "z-base"]);
});

test("no pair is suggested without a language word or without a base reference", () => {
	expect(suggestTranslationPairs([], [])).toEqual([]);
	expect(suggestTranslationPairs([mod("base-mod", "base-mod")], [])).toEqual([]);
	expect(suggestTranslationPairs([mod("base-mod", "base-mod"), mod("unrelated-mod", "unrelated-mod")], [])).toEqual([]);
	expect(suggestTranslationPairs([mod("base-mod", "base-mod"), mod("explosion-pl", "explosion-pl")], [])).toEqual([]);
});

test("sharedConflictPaths returns only path keys present in both mods", () => {
	const base = details(["interface/fonts.esp", "textures/ui.dds", "scripts/main.pex"]);
	const translation = details(["textures/ui.dds", "audio/voice.ogg"]);
	expect(sharedConflictPaths(base, translation)).toEqual(["textures/ui.dds"]);
});

test("sharedConflictPaths drops non-overlapping and duplicate paths", () => {
	const base = details(["interface/fonts.esp", "interface/fonts.esp"]);
	expect(sharedConflictPaths(base, details(["audio/voice.ogg"]))).toEqual([]);
	expect(sharedConflictPaths(details([]), details([]))).toEqual([]);
});
