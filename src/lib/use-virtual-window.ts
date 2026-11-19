import { useVirtualizer, type VirtualItem, type Virtualizer } from "@tanstack/react-virtual";

type VirtualizerOptions<TScrollElement extends Element> = Parameters<typeof useVirtualizer<TScrollElement, Element>>[0];

export interface VirtualWindow {
	/** The rows around the viewport. A new array whenever the window moves, so callers can depend on it safely. */
	items: VirtualItem[];
	totalSize: number;
	scrollToIndex: Virtualizer<Element, Element>["scrollToIndex"];
}

/**
 * The one place that calls TanStack Virtual. Its `useVirtualizer()` returns a stable instance with methods the React
 * Compiler cannot memoize: caching a view on that instance would keep a stale window after a scroll. This hook hands
 * callers the plain values they draw instead, so the compiler can compile the views themselves. The hook stays out of
 * the compiler (see `vite.config.ts`); callers must not reach for the instance behind it.
 */
export function useVirtualWindow<TScrollElement extends Element>(options: VirtualizerOptions<TScrollElement>): VirtualWindow {
	"use no memo";
	// oxlint-disable-next-line react/incompatible-library -- isolated here on purpose: a single opted-out hook keeps every caller compilable. The returned values are plain and change when the virtualizer updates.
	const virtualizer = useVirtualizer<TScrollElement, Element>(options);
	return {
		items: virtualizer.getVirtualItems(),
		totalSize: virtualizer.getTotalSize(),
		scrollToIndex: virtualizer.scrollToIndex,
	};
}
