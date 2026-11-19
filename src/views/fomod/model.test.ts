import { expect, test } from "bun:test";

import { GROUP_HINT, groupKey, isLocked, isRadio, selectionKey } from "@/views/fomod/model";

/// The pick decisions (radio, at-most-one, SelectAll, required and unusable options, saved answers) are covered by the
/// backend's `import::fomod::state` tests. These tests only cover the presentation helpers the window still owns.

test("group keys use the group position, not its name", () => {
	expect(groupKey(0, 1)).not.toBe(groupKey(0, 0));
	expect(groupKey(1, 0)).not.toBe(groupKey(0, 1));
	expect(groupKey(0, 1)).toBe("0:1");
});

test("every group kind has a wizard hint", () => {
	expect(GROUP_HINT).toEqual({
		select_any: "fomod.group.selectAny",
		select_at_least_one: "fomod.group.selectAtLeastOne",
		select_exactly_one: "fomod.group.selectExactlyOne",
		select_at_most_one: "fomod.group.selectAtMostOne",
		select_all: "fomod.group.selectAll",
	});
});

test("SelectAll is not a radio group", () => {
	expect(isRadio("select_all")).toBe(false);
	expect(isRadio("select_any")).toBe(false);
	expect(isRadio("select_at_least_one")).toBe(false);
	expect(isRadio("select_exactly_one")).toBe(true);
	expect(isRadio("select_at_most_one")).toBe(true);
});

test("a group is locked only when every listed option is required", () => {
	expect(isLocked([])).toBe(false);
	expect(isLocked(["required", "required"])).toBe(true);
	expect(isLocked(["required", "optional"])).toBe(false);
});

test("a selection key does not depend on the order of its choices", () => {
	const a = { step: 0, group_index: 0, group: "mode", option: "on" };
	const b = { step: 0, group_index: 1, group: "extras", option: "extra" };
	expect(selectionKey([a, b])).toBe(selectionKey([b, a]));
	expect(selectionKey([a])).not.toBe(selectionKey([b]));
});
