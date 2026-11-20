import { beforeEach, expect, test } from "bun:test";

import { clearFailedTask, failedTasks, recordFailedTask, resetFailedTasks } from "@/lib/failed-tasks-store";

beforeEach(() => {
	resetFailedTasks();
});

test("a failed task replaces its earlier failure and stays first", () => {
	recordFailedTask({
		id: "apply",
		kind: "apply",
		title: "Apply failed",
		detail: "first",
	});
	recordFailedTask({
		id: "import:/a.zip",
		kind: "import",
		title: "Import failed",
	});
	recordFailedTask({
		id: "apply",
		kind: "apply",
		title: "Apply failed",
		detail: "second",
	});

	expect(failedTasks()).toEqual([
		{ id: "apply", kind: "apply", title: "Apply failed", detail: "second" },
		{ id: "import:/a.zip", kind: "import", title: "Import failed" },
	]);
});

test("clearing a task leaves the others and an unknown id changes nothing", () => {
	recordFailedTask({ id: "apply", kind: "apply", title: "Apply failed" });
	recordFailedTask({
		id: "import:/a.zip",
		kind: "import",
		title: "Import failed",
	});

	clearFailedTask("apply");
	clearFailedTask("missing");

	expect(failedTasks()).toEqual([{ id: "import:/a.zip", kind: "import", title: "Import failed" }]);
});

test("the store keeps every game's rows and tasks without a game; filtering is the activity hook's job", () => {
	recordFailedTask({
		id: "apply:a",
		kind: "apply",
		game: "game-a",
		title: "Apply A failed",
	});
	recordFailedTask({
		id: "import:b",
		kind: "import",
		game: "game-b",
		title: "Import B failed",
	});
	recordFailedTask({ id: "global", kind: "update", title: "Global failure" });

	// Listing all rows is pure and unfiltered, so switching the shown game never drops another game's retry action.
	expect(failedTasks()).toHaveLength(3);
	expect(failedTasks().map((task) => task.game)).toEqual([undefined, "game-b", "game-a"]);
});
