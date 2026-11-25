import { expect, test } from "bun:test";

import { tagSuggestions, withTag } from "@/lib/mod-tags";

test("a tag is added after the tags the mod has, trimmed", () => {
	expect(withTag(["fantasy"], "  sci-fi ")).toEqual(["fantasy", "sci-fi"]);
});

test("a blank name adds nothing", () => {
	expect(withTag(["fantasy"], "   ")).toBeNull();
	expect(withTag([], "")).toBeNull();
});

test("a tag the mod has already is not added again, whatever the spacing", () => {
	expect(withTag(["fantasy"], " fantasy ")).toBeNull();
});

test("adding a tag leaves the list it was given as it was", () => {
	const tags = ["fantasy"];
	withTag(tags, "sci-fi");
	expect(tags).toEqual(["fantasy"]);
});

test("the suggestions are the game's tags the mod does not have, in their order", () => {
	expect(tagSuggestions(["horror", "fantasy", "sci-fi"], ["fantasy"])).toEqual(["horror", "sci-fi"]);
});

test("every game tag is suggested when the mod has none", () => {
	expect(tagSuggestions(["horror", "fantasy"], [])).toEqual(["horror", "fantasy"]);
});

test("no suggestion is left when the mod has every game tag", () => {
	expect(tagSuggestions(["horror", "fantasy"], ["fantasy", "horror"])).toEqual([]);
});
