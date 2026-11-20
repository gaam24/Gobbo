import { useSyncExternalStore } from "react";

/// A small external store for state that only a few components subscribe to, such as progress and failed tasks. The
/// screens around them keep rendering from the workspace, while a percent event re-renders only the activity card.
export interface Store<T> {
	get: () => T;
	set: (value: T) => void;
	subscribe: (listener: () => void) => () => void;
}

export function createStore<T>(initial: T): Store<T> {
	let value = initial;
	const listeners = new Set<() => void>();
	return {
		get: () => value,
		set(next) {
			if (Object.is(next, value)) return;
			value = next;

			for (const listener of listeners) listener();
		},
		subscribe(listener) {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
	};
}

/// Subscribes a component to a store. `get` is the same function on every render, as `useSyncExternalStore` requires.
export function useStore<T>(store: Store<T>): T {
	return useSyncExternalStore(store.subscribe, store.get, store.get);
}
