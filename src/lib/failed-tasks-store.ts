import { createStore, useStore } from "@/lib/external-store";
import type { TaskKind } from "@/lib/tasks";

/// A failed long-running operation that stays in the activity centre until it is retried or dismissed. The operation
/// that started it records the row with its own retry action, so the row keeps working after the failure toast is gone.
export interface FailedTask {
	id: string;
	kind: TaskKind;
	/// The game the failure belongs to, when it belongs to one. The store keeps rows for every game and never filters
	/// them: the activity centre shows all failures and uses this id to name the game a retry will affect.
	game?: string;
	title: string;
	detail?: string;
	onRetry?: () => void;
	/// The operation's identity, when it has one. The activity hook uses it to merge a local failure with the backend
	/// lifecycle row of the same work, so one failed import is not drawn twice.
	dedupeKey?: string;
}

const store = createStore<readonly FailedTask[]>([]);

/// Adds a failed task, replacing an earlier failure with the same id. The newest failure is first.
export function recordFailedTask(task: FailedTask) {
	store.set([task, ...store.get().filter((item) => item.id !== task.id)]);
}

/// Removes a failed task. A retry removes its row before it starts, so a second failure adds a fresh one.
export function clearFailedTask(id: string) {
	const current = store.get();
	const next = current.filter((task) => task.id !== id);

	if (next.length === current.length) return;
	store.set(next);
}

/// Drops every failed task. Tests use it; the window keeps the rows across a game switch.
export function resetFailedTasks() {
	store.set([]);
}

/// The pure list of all stored rows, for every game and for tasks with no game. The store never filters by game: a
/// game switch only changes which game the activity hook draws, and a failure of another game keeps its retry action.
export function failedTasks(): readonly FailedTask[] {
	return store.get();
}

export function useFailedTasks(): readonly FailedTask[] {
	return useStore(store);
}
