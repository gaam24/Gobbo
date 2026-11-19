import { expect, test } from "bun:test";
import { missingFolders, trimCache } from "@/lib/use-mod-extras";

test("missing folders are the ones not cached, in the order asked", () => {
	const cache = new Map([["a", { folder: "a" }]]);
	expect(missingFolders(cache, ["b", "a", "c", "b"])).toEqual(["b", "c", "b"]);
});

test("trimming drops entries outside the window before entries still in view", () => {
	const cache = new Map([
		["old", { folder: "old" }],
		["seen", { folder: "seen" }],
		["other", { folder: "other" }],
	]);
	trimCache(cache, new Set(["seen"]), 2);
	expect([...cache.keys()]).toEqual(["seen", "other"]);
});

test("entries in view are kept even above the limit", () => {
	const cache = new Map([
		["a", { folder: "a" }],
		["b", { folder: "b" }],
		["c", { folder: "c" }],
	]);
	trimCache(cache, new Set(["a", "b", "c"]), 1);
	expect([...cache.keys()]).toEqual(["a", "b", "c"]);
});

test("a cache at or below the limit is left alone", () => {
	const cache = new Map([["kept", { folder: "kept" }]]);
	trimCache(cache, new Set(), 1);
	expect([...cache.keys()]).toEqual(["kept"]);
});
