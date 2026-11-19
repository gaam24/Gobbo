import { useCallback, useSyncExternalStore } from "react";

/// Whether a media query matches now, kept current as the window changes.
export function useMediaQuery(query: string): boolean {
	const subscribe = useCallback(
		(changed: () => void) => {
			const list = window.matchMedia(query);
			list.addEventListener("change", changed);
			return () => list.removeEventListener("change", changed);
		},
		[query],
	);
	const getSnapshot = useCallback(() => window.matchMedia(query).matches, [query]);
	return useSyncExternalStore(subscribe, getSnapshot, () => false);
}
