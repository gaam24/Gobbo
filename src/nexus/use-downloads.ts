import { useState } from "react";

import { commands, type DownloadView_Serialize, type ImportView, type Status } from "@/bindings";
import { useLatestRef } from "@/lib/use-latest-ref";
import { useTauriListener } from "@/lib/use-tauri-listener";

/// One download of the queue, as the backend sends it.
export type Download = DownloadView_Serialize;

/// A link's download the backend imported on its own (`download-imported`): the same import a collection member gets.
export interface DownloadImported {
	game: string;
	name: string;
	/// What the import added; `null` when the library already had the file.
	view: ImportView | null;
	/// The library folder that already had the exact file, when the download was a duplicate.
	existing: string | null;
	/// The game's status after the profiles took the new mods.
	status: Status;
}

interface UseDownloadsOptions {
	/// A link started a download while the window was open.
	onStarted: (download: Download) => void;
	/// The backend imported a link's download.
	onImported: (imported: DownloadImported) => void;
	/// Each download that fails while the window is open, once.
	onFailed: (download: Download) => void;
}

/// What the rail button shows: the downloads still coming in or being installed, the progress of the transfer, and the
/// ones that wait for the user (downloaded, not installed) or failed.
export function downloadSummary(downloads: Download[]) {
	const downloading = downloads.filter((download) => download.state === "downloading");
	const measured = downloading.filter((download) => download.total_kb > 0);
	const received = measured.reduce((sum, download) => sum + download.received_kb, 0);
	const total = measured.reduce((sum, download) => sum + download.total_kb, 0);
	const installing = downloads.filter((download) => download.state === "installing").length;
	return {
		running: downloading.length + installing,
		percent: total > 0 ? Math.min(100, Math.floor((received * 100) / total)) : null,
		waiting: downloads.filter((download) => download.state === "ready").length,
		failed: downloads.filter((download) => download.state === "failed").length,
	};
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
		/// counts as seen.
		let known: { ids: Set<number>; failed: Set<number> } | null = null;
		/// The downloads already seen, and the failures already reported. What was there before the window opened
		/// counts as seen. The first full queue fills them; a progress event before it only merges.
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
