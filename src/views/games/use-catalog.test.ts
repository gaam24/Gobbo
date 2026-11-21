import { expect, test } from "bun:test";

import type { AppError, CatalogEntryView, CatalogInstallOutcome, CatalogStatus } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { hasCatalogUpdates, installFailures, progressOf } from "@/views/games/use-catalog";

await applyLanguage("en");

function entry(id: string): CatalogEntryView {
	return { id, name: `Game ${id}`, loader: null, verified: false, min_gobbo: null };
}

function status(changed: CatalogEntryView[], available: CatalogEntryView[], blocked: CatalogEntryView[] = []): CatalogStatus {
	return { changed, available, blocked };
}

const failure: AppError = { code: "catalog.unknown", params: {}, kind: "user_action" };

test("only changed and available entries make the banner appear", () => {
	expect(hasCatalogUpdates(null)).toBe(false);
	expect(hasCatalogUpdates(status([], [], [entry("blocked")]))).toBe(false);
	expect(hasCatalogUpdates(status([entry("changed")], []))).toBe(true);
	expect(hasCatalogUpdates(status([], [entry("new")]))).toBe(true);
});

test("a channel step becomes the progress the dialog draws", () => {
	expect(progressOf({ step: "downloading", percent: 42 })).toEqual({ kind: "downloading", percent: 42 });
	expect(progressOf({ step: "downloading", percent: null })).toEqual({ kind: "downloading", percent: undefined });
	expect(progressOf({ step: "installing" })).toEqual({ kind: "installing" });
});

test("each refused id becomes one translated failure and installed ids are kept out", () => {
	const outcomes: CatalogInstallOutcome[] = [
		{ id: "good", result: { status: "installed" } },
		{ id: "missing", result: { status: "failed", error: failure } },
	];
	expect(installFailures(outcomes)).toEqual([{ id: "missing", message: "The catalog doesn't list this game." }]);
	expect(installFailures(outcomes.filter((outcome) => outcome.result.status === "installed"))).toEqual([]);
});
