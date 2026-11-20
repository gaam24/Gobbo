import type { listen as tauriListen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";

import { loadEvents } from "@/lib/tauri";

/// The lazily loaded `listen` and the function Tauri returns to stop one subscription.
export type Listen = typeof tauriListen;
export type Unlisten = () => void;

export interface ListenerScope {
	/// False once the owning effect has cleaned up. A late answer must not write state or keep a listener.
	readonly active: boolean;
	/// The `listen` from the lazily loaded event module, ready for an event name and a payload handler.
	readonly listen: Listen;
	/// Registers unlisten functions as soon as `listen` resolves. One that arrives after cleanup is stopped at once.
	add(...unlistens: Unlisten[]): void;
}

/// The mount-safe core, free of React so the async unlisten race can be tested on its own. `listen` resolves after the
/// component may already have unmounted (React mounts effects twice in development), so registration and cleanup share
/// one list of stop functions.
export function createListenerScope(listen: Listen): {
	scope: ListenerScope;
	close: () => void;
} {
	let active = true;
	const stops: Unlisten[] = [];
	const scope: ListenerScope = {
		get active() {
			return active;
		},
		listen,
		add(...unlistens: Unlisten[]) {
			if (active) stops.push(...unlistens);
			else for (const stop of unlistens) stop();
		},
	};
	return {
		scope,
		close() {
			active = false;

			for (const stop of stops.splice(0)) stop();
		},
	};
}

/// Subscribes to Tauri events for the lifetime of `key`. The subscribe callback is kept in a ref, so a handler closing
/// over changing values does not resubscribe on every render; callers that never want to resubscribe pass no key and
/// read changing values through a ref. A subscription that resolves after unmount is closed immediately, and every
/// registered unlisten runs on cleanup.
export function useTauriListener(subscribe: (scope: ListenerScope) => Promise<void>, key?: unknown): void {
	const subscribeRef = useRef(subscribe);
	useEffect(() => {
		subscribeRef.current = subscribe;
	});
	useEffect(() => {
		// An object, not a local boolean: the cleanup writes it from another closure, which flow analysis cannot follow.
		const state = { closed: false };
		let close: (() => void) | undefined;
		void (async () => {
			try {
				const { listen } = await loadEvents();

				if (state.closed) return;
				const opened = createListenerScope(listen);
				close = opened.close;
				await subscribeRef.current(opened.scope);
			} catch {
				// A browser preview has no events. A failed listen leaves nothing to clean up; the caller's fallback (a
				// poll, an empty list) still runs on its own.
			}
		})();
		return () => {
			state.closed = true;
			close?.();
		};
		// oxlint-disable-next-line react/exhaustive-effect-dependencies -- key is the resubscribe trigger; the callback is read through a ref.
	}, [key]);
}
