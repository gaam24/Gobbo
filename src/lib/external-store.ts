import { useSyncExternalStore } from "react";

/// A small external store for state that only a few components subscribe to, such as progress and failed tasks. The
/// screens around them keep rendering from the workspace, while a percent event re-renders only the activity card.
export interface Store<T> {
	get: () => T;
	set: (value: T) => void;
	subscribe: (listener: () => void) => () => void;
}

// A count of store notifications, for a development or test reader. A progress event notifies the activity card, not
// the screens, so the number of notifications is an upper bound for the renders a burst of progress can cause. The
// counter is one integer increment per change, so it stays in every build and needs no environment switch.
let notices = 0;

/// The notifications since the last reset. Only useful in a development or E2E build.
export function storeNotices() {
	return notices;
}

/// Clears the notification counter before a measured burst.
export function resetStoreNotices(): void {
	notices = 0;
}

export function createStore<T>(initial: T): Store<T> {
	let value = initial;
	const listeners = new Set<() => void>();
	return {
		get: () => value,
		set(next) {
			if (Object.is(next, value)) return;
			value = next;
			notices += 1;
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
