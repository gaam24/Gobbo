import { type RefObject, useEffect, useRef, useState } from "react";

/// Whether the element has been on screen at least once. It turns `true` the first time the element scrolls near the
/// view and stays `true`, so a picture read once is not read again when it leaves the screen. Without
/// IntersectionObserver (an old webview, a test) the element counts as seen at once. `active` avoids an observer for a
/// card that has no picture to load.
export function useSeen<T extends Element>(active = true): [RefObject<T | null>, boolean] {
	const ref = useRef<T>(null);
	const [seen, setSeen] = useState(() => !active || typeof IntersectionObserver === "undefined");
	useEffect(() => {
		const element = ref.current;
		if (!active || seen || !element) return;
		const observer = new IntersectionObserver(
			([entry]) => {
				if (!entry?.isIntersecting) return;
				setSeen(true);
				observer.disconnect();
			},
			// A little before the edge, so the picture is ready when the card comes into view.
			{ rootMargin: "240px" },
		);
		observer.observe(element);
		return () => observer.disconnect();
	}, [active, seen]);
	return [ref, seen];
}
