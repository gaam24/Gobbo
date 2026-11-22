import { afterAll, expect, mock, test } from "bun:test";
import { renderToString } from "react-dom/server";

import { applyLanguage } from "@/i18n";
import type { Mod } from "@/lib/mods";
import { modFixture } from "@/test/fixtures/mod";

// The bar opens the group dialog through the UI context. A server render has no provider, so the one hook it uses is
// replaced here; restoring afterwards keeps later test files on the real module.
await mock.module("@/app/contexts/ui-context", () => ({
	useUiActions: () => ({ openDialog: () => undefined }),
}));
const { ModsSelectionBar } = await import("@/views/mods/mods-selection-bar");
afterAll(() => mock.restore());

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const bar = (picked: Mod[], hidden: ReadonlySet<string>) =>
	renderToString(
		<ModsSelectionBar
			picked={picked}
			selection={picked.map((item) => item.id)}
			hidden={hidden}
			onSetHidden={() => undefined}
			onSetEnabled={() => undefined}
			onSelect={() => undefined}
		/>,
	);

// The class list contains "disabled:" variants, so match the real attribute to tell an enabled button from a disabled
// one in the server-rendered markup.
const actionButton = (html: string, label: string) => {
	const at = html.indexOf(label);

	if (at < 0) return null;
	const start = html.lastIndexOf("<button", at);
	return html.slice(start, html.indexOf(">", start) + 1);
};

test("offers an enabled hide action for selected mods that are off", () => {
	const html = bar([modFixture({ id: "a", folder: "a", enabled: false }), modFixture({ id: "b", folder: "b", enabled: false })], new Set());
	const button = actionButton(html, "Hide from this list");
	expect(button).not.toBeNull();
	expect(button).not.toContain('disabled=""');
	expect(html).not.toContain("Show in the list");
});

test("offers an enabled show action when all selected mods are hidden", () => {
	const html = bar(
		[modFixture({ id: "a", folder: "a", enabled: false }), modFixture({ id: "b", folder: "b", enabled: false })],
		new Set(["a", "b"]),
	);
	const button = actionButton(html, "Show in the list");
	expect(button).not.toBeNull();
	expect(button).not.toContain('disabled=""');
	expect(html).not.toContain("Hide from this list");
});

test("keeps a disabled hide action when every selected mod is enabled", () => {
	const html = bar([modFixture({ id: "a", folder: "a" }), modFixture({ id: "b", folder: "b" })], new Set());
	const button = actionButton(html, "Hide from this list");
	expect(button).not.toBeNull();
	expect(button).toContain('disabled=""');
	expect(button).toContain('title="Turn a mod off before you hide it from this list"');
	expect(html).not.toContain("Show in the list");
});

test("offers an enabled hide action for a mixed enabled and disabled selection", () => {
	const html = bar([modFixture({ id: "a", folder: "a" }), modFixture({ id: "b", folder: "b", enabled: false })], new Set());
	const button = actionButton(html, "Hide from this list");
	expect(button).not.toBeNull();
	expect(button).not.toContain('disabled=""');
});

test("showing wins when the mixed selection also has a hidden mod", () => {
	const html = bar(
		[modFixture({ id: "a", folder: "a", enabled: false }), modFixture({ id: "b", folder: "b", enabled: false })],
		new Set(["a"]),
	);
	const button = actionButton(html, "Show in the list");
	expect(button).not.toBeNull();
	expect(button).not.toContain('disabled=""');
	expect(html).not.toContain("Hide from this list");
});

test("offers to group the selected mods in Polish", async () => {
	await applyLanguage("pl");
	const html = bar([modFixture({ id: "a", folder: "a" }), modFixture({ id: "b", folder: "b", enabled: false })], new Set());
	expect(html).toContain("Grupuj");
	await applyLanguage("en");
});
