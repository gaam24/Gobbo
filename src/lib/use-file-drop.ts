import { useState } from "react";

import { loadWebview } from "@/lib/tauri";
import { useLatestRef } from "@/lib/use-latest-ref";
import { useTauriListener } from "@/lib/use-tauri-listener";

/// Files dragged from the system onto the window. Returns whether something is hovering over the window.
/// Tauri receives the events, not the browser, so `onDrop` gets the paths on disk.
export function useFileDrop(onDrop: (paths: string[]) => void | Promise<void>): boolean {
	const [dragging, setDragging] = useState(false);
	const handlerRef = useLatestRef(onDrop);

	// A browser preview has no Tauri webview; dropping files is simply not available there.
	useTauriListener(async (scope) => {
		const { getCurrentWebview } = await loadWebview();
		const stop = await getCurrentWebview().onDragDropEvent((event) => {
			const { payload } = event;

			if (payload.type === "drop") {
				setDragging(false);
				void handlerRef.current(payload.paths);
			} else {
				setDragging(payload.type !== "leave");
			}
		});
		scope.add(stop);
	});

	return dragging;
}
