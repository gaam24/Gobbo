import { afterAll, expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import { renderToString } from "react-dom/server";

import type { CatalogEntryView, CatalogStatus } from "@/bindings";
import { applyLanguage } from "@/i18n";

// The dialog shell renders through a portal, which has no container without a browser. The rows and the footer are
// what these checks read, so the shell draws its children inline. Restore the module for the other test files.
const inline = ({ children }: { children?: ReactNode }) => <div>{children}</div>;

await mock.module("@/components/modal-dialog", () => ({ ModalDialog: inline }));
await applyLanguage("en");

const { CatalogDialog } = await import("@/views/games/catalog-dialog");

const BLOCKED_CHECKBOX = /<button[^>]*aria-label="Select Blocked Game"[^>]*>/;
const DISABLED = /\sdisabled=""/;

const nothing = () => {
	// The dialog actions are not part of these structural checks.
};

function entry(id: string, name: string, extra: Partial<CatalogEntryView> = {}): CatalogEntryView {
	return { id, name, loader: null, verified: false, min_gobbo: null, ...extra };
}

function dialog(status: CatalogStatus | null) {
	return renderToString(
		<CatalogDialog
			status={status}
			progress={{ kind: "idle" }}
			onInstall={() => Promise.resolve({ installed: [], failures: [] })}
			onClose={nothing}
		/>,
	);
}

function buttonTag(html: string, text: string): string {
	const at = html.indexOf(text);
	const start = html.lastIndexOf("<button", at);
	const end = html.indexOf("</button>", at) + "</button>".length;
	return html.slice(start, end);
}

test("both tabs draw their entries and the blocked rows stay disabled with a written reason", () => {
	const html = dialog({
		changed: [entry("changed", "Changed Game", { verified: true, loader: "sample-loader" })],
		available: [entry("new", "New Game")],
		blocked: [entry("old", "Blocked Game", { min_gobbo: "0.9.0" }), entry("future", "Future Game")],
	});
	expect(html).toContain("Updates");
	expect(html).toContain("Available games");
	expect(html).toContain("Verified");
	expect(html).toContain("Loader: sample-loader");
	const blocked = html.match(BLOCKED_CHECKBOX)?.[0] ?? "";
	expect(blocked).toMatch(DISABLED);
	expect(html).toContain("Requires Gobbo 0.9.0");
	expect(html).toContain("Requires a newer Gobbo");
	// Updates start checked, so the primary action is available.
	expect(buttonTag(html, "Download selected")).not.toMatch(DISABLED);
});

test("the primary action stays disabled until something is selected", () => {
	const html = dialog({ changed: [], available: [entry("new", "New Game")], blocked: [] });
	expect(buttonTag(html, "Download selected")).toMatch(DISABLED);
});

test("an empty catalog shows a short empty line", () => {
	expect(dialog(null)).toContain("No updated definitions.");
});

afterAll(() => mock.restore());
