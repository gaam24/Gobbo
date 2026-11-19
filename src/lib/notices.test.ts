import { expect, test } from "bun:test";

import {
	addNotice,
	continueToastRun,
	groupNotices,
	isNoticeList,
	MAX_NOTICES,
	type Notice,
	newNotice,
	readAll,
	unreadSummary,
} from "@/lib/notices";

function notice(overrides: Partial<Notice>): Notice {
	return { id: "1", kind: "info", text: "Saved.", at: 0, read: false, ...overrides };
}

test("a new notice is unread, and its detail is kept only when it adds to the text", () => {
	expect(newNotice("info", "Saved.")).toMatchObject({ kind: "info", text: "Saved.", read: false });
	expect(newNotice("error", "Failed.", "install.unknown_layout")).toMatchObject({ detail: "install.unknown_layout" });
	expect(newNotice("error", "Failed.", "Failed.")).not.toHaveProperty("detail");
});

test("the newest notice comes first, and the list keeps only the last ones", () => {
	const full = Array.from({ length: MAX_NOTICES }, (_, index) => notice({ id: String(index) }));
	const list = addNotice(full, notice({ id: "new" }));
	expect(list).toHaveLength(MAX_NOTICES);
	expect(list[0]?.id).toBe("new");
	expect(list.at(-1)?.id).toBe(String(MAX_NOTICES - 2));
});

test("identical notices in a row merge into one group, and a later repeat stays a group of its own", () => {
	const list = [
		notice({ id: "c", text: "Started." }),
		notice({ id: "b", text: "Started." }),
		notice({ id: "a", text: "Saved." }),
		notice({ id: "z", text: "Started." }),
	];
	const groups = groupNotices(list);
	expect(groups.map((group) => [group.notice.id, group.count])).toEqual([
		["c", 2],
		["a", 1],
		["z", 1],
	]);
	expect(groups[0]?.ids).toEqual(["c", "b"]);
});

test("reading the list marks every notice read, and an all-read list comes back as it is", () => {
	const list = [notice({ id: "a" }), notice({ id: "b", read: true })];
	const read = readAll(list);
	expect(read.every((item) => item.read)).toBe(true);
	expect(readAll(read)).toBe(read);
});

test("the unread count names an unread error, and a read one does not count", () => {
	expect(unreadSummary([])).toEqual({ count: 0, error: false });
	expect(unreadSummary([notice({ id: "a" }), notice({ id: "b", kind: "error", read: true })])).toEqual({ count: 1, error: false });
	expect(unreadSummary([notice({ id: "a" }), notice({ id: "b", kind: "error" })])).toEqual({ count: 2, error: true });
});

test("a notice list saved by an earlier start is kept only when every notice in it is well formed", () => {
	const list = [notice({ id: "a" }), notice({ id: "b", kind: "error", detail: "E_CODE" })];
	const text = JSON.stringify(list);
	expect(isNoticeList(JSON.parse(text))).toBe(true);
	expect(isNoticeList([{ id: "a" }])).toBe(false);
	expect(isNoticeList([{ ...notice({}), kind: "warning" }])).toBe(false);
	expect(isNoticeList([{ ...notice({}), detail: 42 }])).toBe(false);
	expect(isNoticeList("notices")).toBe(false);
});

test("notices made in one start never share an id", () => {
	expect(newNotice("info", "Saved.").id).not.toBe(newNotice("info", "Saved.").id);
});

test("consecutive identical results fold into one toast run, and a different result starts a new one", () => {
	const first = continueToastRun(null, "success|Saved.");
	expect(first).toEqual({ run: { signature: "success|Saved.", count: 1 }, repeated: false });
	const second = continueToastRun(first.run, "success|Saved.");
	expect(second).toEqual({ run: { signature: "success|Saved.", count: 2 }, repeated: true });
	const third = continueToastRun(second.run, "info|Downloading.");
	expect(third).toEqual({ run: { signature: "info|Downloading.", count: 1 }, repeated: false });
});

test("a durable notice keeps its action, and a stored action keeps its label when the callback is dropped", () => {
	const action = {
		label: "Undo",
		onClick: () => {
			/* fixture: no work to do */
		},
	};
	const durable = newNotice("info", "Updated.", undefined, action);
	expect(durable.action).toBe(action);
	expect(newNotice("info", "Saved.")).not.toHaveProperty("action");

	// JSON serialization drops the function, leaving only the label; the notice still loads and its button is hidden.
	// oxlint-disable-next-line unicorn/prefer-structured-clone -- JSON is the real storage format; it drops the function on purpose.
	const stored = JSON.parse(JSON.stringify([durable]));
	expect(isNoticeList(stored)).toBe(true);
	expect(stored[0].action).toEqual({ label: "Undo" });
	expect(isNoticeList([{ ...notice({}), action: { label: 42 } }])).toBe(false);
});
