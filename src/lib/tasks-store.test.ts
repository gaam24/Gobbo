import { beforeEach, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToString } from "react-dom/server";

import type { TaskView_Serialize } from "@/bindings";
import {
	backendTaskFromPayload,
	backendTaskKind,
	normalizeBackendTask,
	type RegisteredTask,
	registerTask,
	removeTask,
	updateTask,
	useRegisteredTasks,
} from "@/lib/tasks-store";

function TaskProbe({ observe }: { observe: (value: RegisteredTask[]) => void }) {
	observe(useRegisteredTasks());
	return null;
}

/// Reads the registry through the production subscription, the same one the activity card uses.
function registeredTasks(): RegisteredTask[] {
	const seen: RegisteredTask[][] = [];
	renderToString(createElement(TaskProbe, { observe: (value) => seen.push(value) }));
	const tasks = seen[0];
	if (tasks === undefined) throw new Error("the task hook did not render");
	return tasks;
}

function local(id: string, overrides: Partial<RegisteredTask> = {}): RegisteredTask {
	return {
		id,
		game: null,
		kind: "import",
		title: id,
		state: "running",
		source: "local",
		...overrides,
	};
}

beforeEach(() => {
	// Clear through the public removal command; the store has no test-only reset.
	for (const task of registeredTasks()) removeTask(task.id);
});

test("registering an id twice replaces its row in place", () => {
	registerTask(local("a", { title: "First" }));
	registerTask(local("b", { title: "Second" }));
	registerTask(local("a", { title: "Updated", percent: 50 }));

	expect(registeredTasks()).toEqual([local("a", { title: "Updated", percent: 50 }), local("b", { title: "Second" })]);
});

test("tasks of other games and global tasks are all kept", () => {
	registerTask(local("a", { game: "game-a" }));
	registerTask(local("b", { game: "game-b" }));
	registerTask(local("global", { game: null }));

	expect(registeredTasks().map((task) => task.game)).toEqual(["game-a", "game-b", null]);
});

test("a patch changes one row and an unknown id is ignored", () => {
	const cancel = { label: "Stop collection", run: () => undefined };
	registerTask(local("a", { cancel }));

	updateTask("a", { percent: 42, state: "failed" });
	updateTask("missing", { percent: 1 });

	expect(registeredTasks()).toEqual([local("a", { cancel, percent: 42, state: "failed" })]);
});

test("removing a task leaves the others and an unknown id changes nothing", () => {
	registerTask(local("a"));
	registerTask(local("b"));

	removeTask("a");
	removeTask("missing");

	expect(registeredTasks()).toEqual([local("b")]);
});

test("backend kind codes map to task kinds, with an update fallback for unknown codes", () => {
	expect(backendTaskKind("collection")).toBe("collection");
	expect(backendTaskKind("deploy")).toBe("apply");
	expect(backendTaskKind("download")).toBe("download");
	// Pack work is an emitted backend code, so it must not silently take the generic update fallback.
	expect(backendTaskKind("pack")).toBe("import");
	expect(backendTaskKind("pack-link")).toBe("download");
	expect(backendTaskKind("a-new-backend-code")).toBe("update");
});

test("a backend payload becomes a backend-sourced registry row", () => {
	expect(backendTaskFromPayload({ id: "t1", game: "game-b", kind: "collection", state: "waiting" }, "Installing collection…")).toEqual({
		id: "t1",
		game: "game-b",
		kind: "collection",
		title: "Installing collection…",
		state: "waiting",
		source: "backend",
		sourceKind: "collection",
	});

	expect(backendTaskFromPayload({ id: "t2", game: null, kind: "unlisted", state: "failed" }, "Working…")).toEqual({
		id: "t2",
		game: null,
		kind: "update",
		title: "Working…",
		state: "failed",
		source: "backend",
		sourceKind: "unlisted",
	});
});

test("a backend row with an existing local id replaces it instead of duplicating it", () => {
	registerTask(local("same", { title: "Local" }));
	registerTask(backendTaskFromPayload({ id: "same", game: null, kind: "deploy", state: "running" }, "Backend"));

	expect(registeredTasks()).toHaveLength(1);
	expect(registeredTasks()[0]?.title).toBe("Backend");
	expect(registeredTasks()[0]?.source).toBe("backend");
});

test("a backend payload carries its progress into the registry row", () => {
	expect(backendTaskFromPayload({ id: "t3", game: null, kind: "import", state: "running", percent: 75 }, "Importing…")).toEqual({
		id: "t3",
		game: null,
		kind: "import",
		title: "Importing…",
		state: "running",
		percent: 75,
		source: "backend",
		sourceKind: "import",
	});
});

test("a snapshot row normalizes a null percent into an absent one", () => {
	const row: TaskView_Serialize = {
		id: "t4",
		game: "game-b",
		kind: "apply",
		state: "running",
		percent: null,
	};

	expect(normalizeBackendTask(row)).toEqual({
		id: "t4",
		game: "game-b",
		kind: "apply",
		state: "running",
		percent: undefined,
	});
	expect(normalizeBackendTask({ ...row, percent: 25 }).percent).toBe(25);
});

test("a restored failure carries the retry and open actions of its kind", () => {
	const retry = { label: "Retry", run: () => undefined };
	const open = { label: "Open", run: () => undefined };
	const payload = {
		id: "apply:game-b",
		game: "game-b",
		kind: "apply",
		state: "failed",
	};

	expect(
		backendTaskFromPayload(payload, "Apply failed", {
			retry: () => retry,
			open: () => open,
		}),
	).toMatchObject({
		state: "failed",
		retry,
		open,
	});
});

test("a restored waiting row keeps its open action and a running row gets no stale retry", () => {
	const open = { label: "Open", run: () => undefined };
	const retry = { label: "Retry", run: () => undefined };
	const actions = { retry: () => retry, open: () => open };

	const waiting = backendTaskFromPayload({ id: "waiting:game", game: "game", kind: "collection", state: "waiting" }, "Waiting", actions);
	expect(waiting.state).toBe("waiting");
	expect(waiting.open).toBe(open);
	expect(waiting.retry).toBe(retry);

	const running = backendTaskFromPayload({ id: "apply:game", game: "game", kind: "apply", state: "running" }, "Applying", actions);
	expect(running.retry).toBeUndefined();
	expect(running.open).toBeUndefined();
});
