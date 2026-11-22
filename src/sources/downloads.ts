import type { DownloadView_Serialize, ImportView, Status } from "@/bindings";

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
