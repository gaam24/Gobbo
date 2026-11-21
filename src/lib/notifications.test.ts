import { expect, test } from "bun:test";
import { taskNotification, withMuted } from "@/lib/notifications";

test("a task notification code joins the kind and the outcome", () => {
	expect(taskNotification("pack-link", "failed")).toBe("pack-link.failed");
});

test("muting adds a code once and unmuting removes it", () => {
	expect(withMuted([], "import.done", false)).toEqual(["import.done"]);
	expect(withMuted(["import.done"], "import.done", false)).toEqual(["import.done"]);
	expect(withMuted(["import.done", "update.failed"], "import.done", true)).toEqual(["update.failed"]);
});

test("a code from a newer version is kept", () => {
	expect(withMuted(["from_a_newer_version"], "import.done", false)).toEqual(["from_a_newer_version", "import.done"]);
});
