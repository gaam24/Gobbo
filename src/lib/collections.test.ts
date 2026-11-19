import { describe, expect, test } from "bun:test";

import type { CollectionBadge, CollectionItem_Serialize, CollectionItemStatus } from "@/bindings";
import { collectionItemLabel, downloadSizeKb, filterCounts, hasCollectionUpdate, matchesSearch, revisionLabel } from "@/lib/collections";

function item(key: string, optional = false, size = 10): CollectionItem_Serialize {
	return {
		key,
		name: `Example ${key}`,
		version: "1.0.0",
		author: "Author",
		category: null,
		summary: "",
		picture_url: null,
		optional,
		phase: 0,
		source: { kind: "provider", provider: "example", mod_id: key, file_id: "1" },
		file_name: null,
		size_kb: size,
		md5: null,
		update_policy: "exact",
		choices: [],
		patches: {},
		instructions: "",
		adult: false,
	};
}

function badge(update: boolean): CollectionBadge {
	return {
		provider: "example",
		game_key: "example",
		slug: "example",
		revision: 2,
		revision_label: "r2",
		name: "Example",
		url: "",
		families: [],
		update,
	};
}

describe("collections", () => {
	test("the revision label falls back to the number", () => {
		expect(revisionLabel({ revision: 4, revision_label: "" })).toBe("r4");
		expect(revisionLabel({ revision: 1_002_003, revision_label: "v1.2.3" })).toBe("v1.2.3");
	});

	test("the header badge notices an update in any collection of the profile", () => {
		expect(hasCollectionUpdate([])).toBe(false);
		expect(hasCollectionUpdate([badge(false)])).toBe(false);
		expect(hasCollectionUpdate([badge(false), badge(true)])).toBe(true);
	});

	test("filter counts follow the statuses", () => {
		const items = [item("a"), item("b"), item("c", true), item("d")];
		const statuses: Record<string, CollectionItemStatus> = {
			a: { kind: "installed" },
			b: { kind: "missing" },
			c: { kind: "skipped" },
			d: { kind: "unsupported" },
		};
		expect(filterCounts(items, statuses)).toEqual({ all: 4, todo: 1, done: 1, optional: 1, issues: 1 });
	});

	test("only selected members that need a download count toward the size", () => {
		const items = [item("a", false, 100), item("b", false, 50), item("c", false, 25)];
		const statuses: Record<string, CollectionItemStatus> = { a: { kind: "installed" }, b: { kind: "missing" }, c: { kind: "missing" } };
		expect(downloadSizeKb(items, statuses, new Set(["a", "b"]))).toBe(50);
	});

	test("a waiting member is named with its version so two files of one page differ", () => {
		const first = { ...item("first"), name: "Placeholder addon", version: "1.0" };
		const second = { ...item("second"), name: "Placeholder main", version: "2.0" };
		expect(collectionItemLabel([first, second], "second")).toBe("Placeholder main 2.0");
		expect(collectionItemLabel([first], "missing")).toBe("missing");
	});

	test("search matches the name, the author and the version", () => {
		expect(matchesSearch(item("a"), "example a")).toBe(true);
		expect(matchesSearch(item("a"), "author")).toBe(true);
		expect(matchesSearch(item("a"), "nothing")).toBe(false);
	});
});
