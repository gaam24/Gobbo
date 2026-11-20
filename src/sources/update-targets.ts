import type { Mod } from "@/lib/mods";
import type { Download } from "@/nexus/use-downloads";

/// One mod the last check found an update for, with the exact provider file a manual update has to download.
export interface UpdateTarget {
	id: string;
	name: string;
	/// The folder the profile selects now, for the undo.
	folder: string;
	provider: string;
	modId: string;
	current: string | null;
	latest: string | null;
	latestFileId: string | null;
	/// The newest version is already installed, so the run only selects it.
	installed: boolean;
}

/// Every mod with a saved update, in list order.
export function updateTargets(mods: readonly Mod[]): UpdateTarget[] {
	const targets: UpdateTarget[] = [];

	for (const mod of mods) {
		const update = mod.update;

		if (!update?.has_update) continue;
		targets.push({
			id: mod.id,
			name: mod.info.name,
			folder: mod.folder,
			provider: update.provider,
			modId: update.mod_id,
			current: update.current ?? null,
			latest: update.latest ?? null,
			latestFileId: update.latest_file_id ?? null,
			installed: update.latest_installed,
		});
	}

	return targets;
}

/// The targets the backend routes to a direct download, and the ones whose provider needs the files page. The route is
/// the backend's: `browserOnly` comes from `SourceProviderInfo.download_route`, and the report's `installed` flag does
/// not override it. An installed newest version on a browser-routed provider goes to the files page step too; that step
/// can select the installed folder, and the backend never downloads for a target it reports as installed.
export function splitUpdateTargets(
	targets: UpdateTarget[],
	browserOnly: ReadonlySet<string>,
): { direct: UpdateTarget[]; manual: UpdateTarget[] } {
	const direct: UpdateTarget[] = [];
	const manual: UpdateTarget[] = [];

	for (const target of targets) {
		if (browserOnly.has(target.provider)) manual.push(target);
		else direct.push(target);
	}

	return { direct, manual };
}

/// Whether a queued download is the update of this target: same game, provider and mod, and the newest provider file
/// when both names it. A provider that does not name files matches on the mod alone.
export function matchesUpdateDownload(target: UpdateTarget, download: Download, gameId: string): boolean {
	const source = download.source;

	if (!source || source.provider !== target.provider || source.mod_id !== target.modId) return false;

	if (download.game !== "" && download.game !== gameId) return false;
	return !(target.latestFileId && source.file_id) || source.file_id === target.latestFileId;
}
