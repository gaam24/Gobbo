import { beforeEach, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToString } from "react-dom/server";

import { localFailedTasks } from "@/app/hooks/use-activity";
import { clearFailedTask, type FailedTask, recordFailedTask, useFailedTasks } from "@/lib/failed-tasks-store";
import { groupedTasks } from "@/lib/tasks";

function FailedTaskProbe({ observe }: { observe: (value: readonly FailedTask[]) => void }) {
	observe(useFailedTasks());
	return null;
}

/// Reads the failures through the production subscription, the same one the activity card uses.
function failedTasks(): readonly FailedTask[] {
	const seen: Array<readonly FailedTask[]> = [];
	renderToString(createElement(FailedTaskProbe, { observe: (value) => seen.push(value) }));
	const tasks = seen[0];
	if (tasks === undefined) throw new Error("the failure hook did not render");
	return tasks;
}

beforeEach(() => {
	// Clear through the public removal command; the store has no test-only reset.
	for (const task of failedTasks()) clearFailedTask(task.id);
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

test("a background game's failed row still appears in the activity view with its Retry action", () => {
	let retried = false;
	recordFailedTask({
		id: "apply:game-b",
		kind: "apply",
		game: "game-b",
		title: "Apply failed",
		onRetry: () => {
			retried = true;
		},
	});

	// The window may be on another game: the activity view groups every failure and keeps each one's retry action.
	const groups = groupedTasks(
		localFailedTasks(failedTasks(), [
			{ id: "game-a", name: "Game A" },
			{ id: "game-b", name: "Game B" },
		]),
	);
	const [background] = groups.flatMap((group) => group.tasks);

	expect(groups.map((group) => group.game)).toEqual(["game-b"]);
	expect(background?.state).toBe("failed");
	expect(background?.gameName).toBe("Game B");
	expect(background?.onRetry).toBeTypeOf("function");
	background?.onRetry?.();
	expect(retried).toBe(true);
});
