import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { HistoryEntry_Serialize } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { HistoryTimeline } from "@/views/history/history-timeline";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const nothing = () => {
	// Selecting an entry belongs to the dialog, not to this check.
};

const entry: HistoryEntry_Serialize = {
	id: 1,
	at: 0,
	total: 2,
	enabled: 1,
	summary: {
		added: 2,
		removed: 0,
		updated: 0,
		moved: 0,
		enabled_on: 0,
		enabled_off: 1,
		priority: 0,
		groups: 0,
		winners: 0,
		waiting: 0,
	},
};

test("the timeline says what each change did and shows the current state", () => {
	const html = renderToString(<HistoryTimeline entries={[entry]} selectedId={1} onSelect={nothing} total={2} enabled={2} locale="en" />);
	expect(html).toContain("Now");
	expect(html).toContain("2 of 2 on");
	expect(html).toContain("Added: 2");
	expect(html).toContain("Switched off: 1");
});

test("an entry without a summary has no row label, and one note under the list explains older entries", () => {
	const older = { ...entry, id: 2, summary: null };
	const html = renderToString(
		<HistoryTimeline entries={[entry, older, { ...older, id: 3 }]} selectedId={1} onSelect={nothing} total={2} enabled={2} locale="en" />,
	);
	expect(html.split("Older entries were saved without a change summary.")).toHaveLength(2);
});

test("the note is absent when every entry has a summary", () => {
	const html = renderToString(<HistoryTimeline entries={[entry]} selectedId={1} onSelect={nothing} total={2} enabled={2} locale="en" />);
	expect(html).not.toContain("Older entries were saved");
});
