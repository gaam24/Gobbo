import { useCallback, useEffect, useRef, useState } from "react";

export interface DeferredOpen<T> {
	/// Remember a payload to open once the new game is current.
	defer: (payload: T) => void;
	/// Forget a remembered payload without opening it.
	clear: () => void;
}

/// A game switch closes every shell dialog, so an Open action that needs another game must wait until the switch's
/// reset has landed. The zero-delay timer lets that close run first. `isReady` and `open` are read from refs, so only
/// `trigger` (the active game) decides when a pending payload may go ahead. No `finally`: the React Compiler skips
/// functions that contain one.
export function useDeferredOpen<T>(trigger: unknown, isReady: (payload: T) => boolean, open: (payload: T) => void): DeferredOpen<T> {
	const [pending, setPending] = useState<T | null>(null);
	const readyRef = useRef(isReady);
	const openRef = useRef(open);
	useEffect(() => {
		readyRef.current = isReady;
		openRef.current = open;
	});
	const defer = useCallback((payload: T) => setPending(payload), []);
	const clear = useCallback(() => setPending(null), []);
	useEffect(() => {
		if (pending === null || !readyRef.current(pending)) return;
		const id = setTimeout(() => {
			openRef.current(pending);
			setPending(null);
		}, 0);
		return () => clearTimeout(id);
		// oxlint-disable-next-line react/exhaustive-effect-dependencies -- trigger is the switch signal; isReady and open are read through refs.
	}, [trigger, pending]);
	return { defer, clear };
}
