import { expect, test } from "bun:test";

import type { HistoryPreview_Serialize, HistorySummary } from "@/bindings";
import { hasPreviewChanges, summaryParts } from "@/lib/history";

const summary = (overrides: Partial<HistorySummary> = {}): HistorySummary => ({
	added: 0,
	removed: 0,
	updated: 0,
	moved: 0,
	enabled_on: 0,
	enabled_off: 0,
	priority: 0,
	groups: 0,
	winners: 0,
	waiting: 0,
	...overrides,
});

const preview = (overrides: Partial<HistoryPreview_Serialize> = {}): HistoryPreview_Serialize => ({
	id: 1,
	at: 0,
	total: 2,
	enabled: 1,
	adds: [],
	removes: [],
	versions: [],
	switches_on: [],
	switches_off: [],
	moved: 0,
	priority: 0,
	groups: 0,
	winners: 0,
	waiting: 0,
	...overrides,
});

test("the summary leaves out zero parts and keeps the timeline order", () => {
	expect(summaryParts(summary({ enabled_off: 3, added: 2, moved: 1 }))).toEqual([
		{ kind: "added", count: 2 },
		{ kind: "moved", count: 1 },
		{ kind: "enabled_off", count: 3 },
	]);
	expect(summaryParts(summary())).toEqual([]);
	expect(summaryParts(null)).toEqual([]);
});

test("a preview with no visible change cannot be restored", () => {
	expect(hasPreviewChanges(preview())).toBe(false);
	expect(hasPreviewChanges(preview({ moved: 1 }))).toBe(true);
	expect(hasPreviewChanges(preview({ adds: [{ id: "a", name: "A", folder: "a", version: null, enabled: true, missing: false }] }))).toBe(
		true,
	);
	expect(hasPreviewChanges(preview({ winners: 1 }))).toBe(true);
});
