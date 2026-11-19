import { expect, test } from "bun:test";

import { type PaletteItem, paletteResults } from "@/lib/palette";

const item = (id: string, group: PaletteItem["group"], label: string, keywords?: string): PaletteItem => ({
	id,
	group,
	label,
	keywords,
	run: () => undefined,
});

const items = [
	item("deploy", "actions", "Deploy and launch", "start play"),
	item("import", "actions", "Import archive"),
	item("downloads", "go", "Downloads"),
	item("tweaks", "mods", "Sample Engine Tweaks", "sampleauthor"),
	item("redscript", "mods", "redscript"),
	item("zrodlo", "profiles", "Źródło"),
];

test("without a query only the actions and places show", () => {
	expect(paletteResults(items, " ").map((found) => found.id)).toEqual(["deploy", "import", "downloads"]);
});

test("every word of the query must match, the label or the other words", () => {
	expect(paletteResults(items, "engine tw").map((found) => found.id)).toEqual(["tweaks"]);
	expect(paletteResults(items, "sampleauthor").map((found) => found.id)).toEqual(["tweaks"]);
	expect(paletteResults(items, "play").map((found) => found.id)).toEqual(["deploy"]);
});

test("accents do not matter, and a label that starts with the query comes first in its group", () => {
	expect(paletteResults(items, "zrodlo").map((found) => found.id)).toEqual(["zrodlo"]);
	const mods = [item("b", "mods", "Better Scripts"), item("a", "mods", "redscript")];
	expect(paletteResults(mods, "script").map((found) => found.id)).toEqual(["b", "a"]);
	expect(paletteResults(mods, "red").map((found) => found.id)).toEqual(["a"]);
});
