import { useEffect, useRef, useState } from "react";

interface LoadOptions<T> {
	/// Load again every this many milliseconds while the window is visible. A hidden window does not ask the backend.
	pollMs?: number;
	/// Called with a failure. Without it a failed load keeps the last answer.
	onError?: (failure: unknown) => void;
	/// Called with a successful answer, so a caller can clear a stale banner.
	onSuccess?: (value: T) => void;
	/// Any change asks again at once, before the next poll. An event uses it to refresh without waiting.
	reloadKey?: unknown;
}

/// The state after an answer arrives. An answer equal to the one on screen keeps the same state object, so a poll that
/// finds nothing new (the game still runs, the value did not change) does not render the screen again. Objects are
/// compared by identity; a backend answer is a new object each time and always replaces the state.
export function answerState<T>(previous: { key: string; value: T } | null, key: string, value: T): { key: string; value: T } {
	return previous?.key === key && Object.is(previous.value, value) ? previous : { key, value };
}

/// Whether an answer still belongs to the live request. Cleanup flips `active`, and a new poll or key raises `request`,
/// so a slow reply from an earlier call is dropped instead of overwriting the newer one.
export function isCurrentReply(active: boolean, request: number, current: number): boolean {
	return active && current === request;
}

/// The answer of `load` for the current `key`, or `undefined` until it arrives. A new key starts a new load, and the
/// answer of an older key is dropped, so a slow reply never shows the wrong item. `null` loads nothing.
export function useLoad<T>(
	key: string | null,
	load: () => Promise<T>,
	{ pollMs, onError, onSuccess, reloadKey }: LoadOptions<T> = {},
): T | undefined {
	const [answer, setAnswer] = useState<{ key: string; value: T } | null>(null);
	// The latest callbacks, so a new closure on every render does not restart the load.
	const latest = useRef({ load, onError, onSuccess });
	useEffect(() => {
		latest.current = { load, onError, onSuccess };
	});

	useEffect(() => {
		if (key === null) return;
		let active = true;
		let request = 0;
		const run = async () => {
			if (pollMs !== undefined && document.hidden) return;
			const current = request + 1;
			request = current;

			try {
				const value = await latest.current.load();

				if (isCurrentReply(active, request, current)) {
					setAnswer((previous) => answerState(previous, key, value));
					latest.current.onSuccess?.(value);
				}
			} catch (failure) {
				if (isCurrentReply(active, request, current)) latest.current.onError?.(failure);
			}
		};
		void run();

		if (pollMs === undefined) {
			return () => {
				active = false;
			};
		}

		const timer = window.setInterval(() => void run(), pollMs);
		// A window that comes back asks at once instead of waiting for the next tick.
		const onVisible = () => {
			if (!document.hidden) void run();
		};
		document.addEventListener("visibilitychange", onVisible);
		return () => {
			active = false;
			window.clearInterval(timer);
			document.removeEventListener("visibilitychange", onVisible);
		};
		// oxlint-disable-next-line react/exhaustive-effect-dependencies -- reloadKey is a trigger; the effect reads it only through the dependency.
	}, [key, pollMs, reloadKey]);

	return answer?.key === key ? answer.value : undefined;
}
