import { expect, test } from "bun:test";

import type { ModDetails, ModGroup } from "@/bindings";
import { detectLanguage, sharedConflictPaths, suggestTranslationPairs } from "@/lib/translation-hints";
import { modFixture } from "@/test/fixtures/mod";

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
		[
			modFixture({ id: "base-mod", folder: "base-mod", page_id: basePage }),
			modFixture({ id: "base-mod-pl", folder: "base-mod-pl", page_id: translationPage }),
		],
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

test("a text naming several languages returns the first in table order", () => {
	expect(detectLanguage("base-mod-de-pl")).toBe("pl");
	expect(detectLanguage("base-mod-zh-ja")).toBe("zh");
});

test("a translation is paired with the base whose name it repeats", () => {
	const hints = suggestTranslationPairs(
		[modFixture({ id: "base-mod", folder: "base-mod" }), modFixture({ id: "base-mod-pl", folder: "base-mod-pl" })],
		[],
	);
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
	const byWord = suggestTranslationPairs(
		[modFixture({ id: "sky-overhaul", folder: "Sky UI Overhaul" }), modFixture({ id: "sky-pl", folder: "Sky Polish" })],
		[],
	);
	expect(byWord).toHaveLength(1);
	expect(byWord[0]?.language).toBe("pl");
	expect(byWord[0]?.confidence).toBeCloseTo(0.79);

	const byFullName = suggestTranslationPairs(
		[modFixture({ id: "sky", folder: "Sky" }), modFixture({ id: "sky-pl", folder: "Sky Polish" })],
		[],
	);
	expect(byFullName[0]?.confidence).toBeGreaterThan(byWord[0]?.confidence ?? 0);
});

test("a language word in tags is a signal too", () => {
	const hints = suggestTranslationPairs(
		[modFixture({ id: "base-mod", folder: "base-mod" }), modFixture({ id: "base-mod-tr", folder: "base-mod-x", tags: ["Türkçe"] })],
		[],
	);
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
	const mods = [modFixture({ id: "base-mod", folder: "base-mod" }), modFixture({ id: "base-mod-pl", folder: "base-mod-pl" })];
	expect(suggestTranslationPairs(mods, [group("base-mod-pl")])).toEqual([]);
	expect(suggestTranslationPairs(mods, [group("base-mod")])).toEqual([]);
	expect(suggestTranslationPairs(mods, [group("unrelated-mod")])).toHaveLength(1);
});

test("disabled mods are still suggested", () => {
	const mods = [
		modFixture({ id: "base-mod", folder: "base-mod", enabled: false }),
		modFixture({ id: "base-mod-pl", folder: "base-mod-pl", enabled: false }),
	];
	expect(suggestTranslationPairs(mods, [])).toHaveLength(1);
});

test("hints sort by confidence, then by family id", () => {
	const mods = [
		modFixture({ id: "z-base", folder: "zebra" }),
		modFixture({ id: "z-base-pl", folder: "zebra-pl" }),
		modFixture({ id: "a-base", folder: "alpha" }),
		modFixture({ id: "a-base-pl", folder: "alpha-pl" }),
		modFixture({ id: "m-base", folder: "mango", page_id: "nexus:1" }),
		modFixture({ id: "m-base-pl", folder: "mango-pl", page_id: "nexus:2" }),
	];
	expect(suggestTranslationPairs(mods, []).map((hint) => hint.baseId)).toEqual(["m-base", "a-base", "z-base"]);
});

test("no pair is suggested without a language word or without a base reference", () => {
	expect(suggestTranslationPairs([], [])).toEqual([]);
	expect(suggestTranslationPairs([modFixture({ id: "base-mod", folder: "base-mod" })], [])).toEqual([]);
	expect(
		suggestTranslationPairs(
			[modFixture({ id: "base-mod", folder: "base-mod" }), modFixture({ id: "unrelated-mod", folder: "unrelated-mod" })],
			[],
		),
	).toEqual([]);
	expect(
		suggestTranslationPairs(
			[modFixture({ id: "base-mod", folder: "base-mod" }), modFixture({ id: "explosion-pl", folder: "explosion-pl" })],
			[],
		),
	).toEqual([]);
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
