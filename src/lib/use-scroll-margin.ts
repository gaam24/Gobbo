import { type RefObject, useLayoutEffect, useState } from "react";

/// The distance from the scroll container's content start to the list that a virtualizer positions. The table has
/// alerts and a sticky header above that list; the grid has its own top padding. TanStack Virtual compares the raw
/// `scrollTop` with item offsets that already include this margin, so without it the window would be shifted by
/// exactly the height of everything above the first row.
export function useScrollMargin(
	scrollRef: RefObject<HTMLElement | null> | undefined,
	listRef: RefObject<HTMLElement | null> | undefined,
): number {
	const [margin, setMargin] = useState(0);

	useLayoutEffect(() => {
		const update = () => {
			const scroll = scrollRef?.current;
			const list = listRef?.current;
			if (!(scroll && list)) return;
			const next = list.getBoundingClientRect().top - scroll.getBoundingClientRect().top + scroll.scrollTop;
			// The list's height changes on every resize; only its top matters here, so ignore those updates. Comparing
			// keeps a no-op from re-rendering the whole virtualized list.
			setMargin((previous) => (Math.abs(previous - next) < 0.5 ? previous : next));
		};
		const frame = requestAnimationFrame(update);
		const observer = new ResizeObserver(update);
		if (scrollRef?.current) observer.observe(scrollRef.current);
		if (listRef?.current) observer.observe(listRef.current);
		window.addEventListener("resize", update);
		return () => {
			cancelAnimationFrame(frame);
			observer.disconnect();
			window.removeEventListener("resize", update);
		};
	}, [scrollRef, listRef]);

	return margin;
}
