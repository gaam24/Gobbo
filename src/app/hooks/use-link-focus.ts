import { useEffect, useRef } from "react";
import { loadWindow } from "@/lib/tauri";

/// How long a check waits after the previous one. A burst of focus changes, such as a dialog opening and closing, asks
/// the system once.
const CHECK_EVERY_MS = 5_000;

/// Calls `check` when the main window gains focus, at most once every `everyMs`. The callback is kept in a ref, so a
/// changing callback does not subscribe again. A browser preview has no Tauri window and keeps the listener off.
export function useLinkFocus(check: () => void, everyMs = CHECK_EVERY_MS) {
	const latest = useRef(check);
	/// The startup check runs as the app opens, so a focus in the first moments does not ask the system again.
	const last = useRef(0);
	useEffect(() => {
		latest.current = check;
	}, [check]);
	useEffect(() => {
		let unlisten: (() => void) | undefined;
		const state = { stopped: false };
		last.current = Date.now();
		void (async () => {
			try {
				const { getCurrentWindow } = await loadWindow();
				const stop = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
					if (!focused) return;
					const now = Date.now();
					if (now - last.current < everyMs) return;
					last.current = now;
					latest.current();
				});
				if (state.stopped) stop();
				else unlisten = stop;
			} catch {
				// a browser preview or a closing window has no focus event to listen to
			}
		})();
		return () => {
			state.stopped = true;
			unlisten?.();
		};
	}, [everyMs]);
}
