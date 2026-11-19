import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { UpdateReport } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { UpdateLine } from "@/sources/update-line";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const report = (overrides: Partial<UpdateReport>): UpdateReport => ({
	id: "nexus:1",
	provider: "nexus",
	mod_id: "1",
	current: "1.1.3",
	latest: "1.1.3a",
	latest_file_id: null,
	state: "different",
	latest_installed: false,
	installed_folder: null,
	has_update: true,
	error: null,
	...overrides,
});

test("an unordered difference is still shown as a possible update", () => {
	const html = renderToString(<UpdateLine update={report({})} onIgnore={() => undefined} />);
	expect(html).toContain("Available 1.1.3a");
	expect(html).toContain("Ignore this version");
	expect(html).not.toContain("Different version");
});

test("an ignored version offers the way back", () => {
	const html = renderToString(<UpdateLine update={report({ state: "ignored", has_update: false })} onUnignore={() => undefined} />);
	expect(html).toContain("Ignored 1.1.3a");
	expect(html).toContain("Show this version again");
});
