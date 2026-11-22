import { useState } from "react";

import { commands } from "@/bindings";
import { useLatestRef } from "@/lib/use-latest-ref";
import { useTauriListener } from "@/lib/use-tauri-listener";
import type { Download, DownloadImported } from "@/sources/downloads";

interface UseDownloadsOptions {
	/// A link started a download while the window was open.
	onStarted: (download: Download) => void;
	/// The backend imported a link's download.
	onImported: (imported: DownloadImported) => void;
	/// Each download that fails while the window is open, once.
	onFailed: (download: Download) => void;
}

/// The download queue of the backend, kept current by its events. The backend owns the install decision and the wait
/// for a FOMOD wizard: this hook only observes the queue and reports starts and failures. A wizard is opened from the
/// Waiting panel's explicit install action, never from an effect reading a download flag.
export function useDownloads({ onStarted, onImported, onFailed }: UseDownloadsOptions) {
	const [downloads, setDownloads] = useState<Download[]>([]);
	const handlers = useLatestRef({ onStarted, onImported, onFailed });

	useTauriListener(async (scope) => {
		/// Events can arrive while the first queue is being read. Buffering them preserves their order and prevents the
		/// older snapshot from overwriting a newer event that arrived just before its answer.
		let initialized = false;
		const buffered: Array<{ kind: "queue"; queue: Download[] } | { kind: "progress"; download: Download }> = [];
		/// The downloads already seen, and the failures already reported. What was there before the window opened
		/// counts as seen. The first full queue fills them; a progress event before it only merges.
		let known: { ids: Set<number>; failed: Set<number> } | null = null;
		const observe = (queue: Download[]) => {
			const seen = known ?? {
				ids: new Set(queue.map((download) => download.id)),
				failed: new Set(queue.filter((download) => download.state === "failed").map((download) => download.id)),
			};
			known = seen;

			for (const download of queue) {
				if (!seen.ids.has(download.id)) {
					seen.ids.add(download.id);

					if (download.state === "downloading") handlers.current.onStarted(download);
				}

				if (download.state === "failed" && !seen.failed.has(download.id)) {
					seen.failed.add(download.id);
					handlers.current.onFailed(download);
				}
			}
		};
		const show = (queue: Download[]) => {
			if (!scope.active) return;

			if (!initialized) {
				buffered.push({ kind: "queue", queue });
				return;
			}

			setDownloads(queue);
			observe(queue);
		};
		/// One download whose progress changed: replace it in place, without cloning the whole queue on every tick. Once
		/// the baseline queue is known, a progress event for an unknown id is a late event for a removed download.
		const merge = (download: Download) => {
			if (!scope.active) return;

			if (!initialized) {
				buffered.push({ kind: "progress", download });
				return;
			}

			setDownloads((queue) => {
				const at = queue.findIndex((entry) => entry.id === download.id);

				if (at < 0) return known ? queue : [...queue, download];
				const next = [...queue];
				next[at] = download;
				return next;
			});

			if (known) observe([download]);
		};
		const flush = () => {
			initialized = true;

			for (const event of buffered.splice(0)) {
				if (event.kind === "queue") {
					setDownloads(event.queue);
					observe(event.queue);
				} else {
					merge(event.download);
				}
			}
		};
		const unlisten = await Promise.all([
			scope.listen<Download[]>("downloads", (event) => show(event.payload)),
			scope.listen<Download>("downloads-progress", (event) => merge(event.payload)),
			scope.listen<DownloadImported>("download-imported", (event) => {
				if (scope.active) handlers.current.onImported(event.payload);
			}),
		]);
		scope.add(...unlisten);

		try {
			const initial = await commands.listDownloads();

			if (!scope.active) return;
			initialized = true;
			setDownloads(initial);
			observe(initial);
			flush();
		} catch {
			if (!scope.active) return;
			// Without the backend (a browser preview) there are no downloads. A queued event may still be the only
			// thing the window has seen, so let it through instead of dropping it.
			flush();
		}
	});

	return { downloads, setDownloads };
}
