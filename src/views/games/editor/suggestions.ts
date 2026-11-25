import type { FolderInspection_Serialize } from "@/bindings";
import { type GameDraft, loaderId, setLoader, slugify } from "@/views/games/editor/draft";

const EXE_SUFFIX = /\.exe$/i;

/// The values read from the folder that the draft does not have yet. A field the user already filled is never
/// overwritten, so a scan is always safe to repeat.
export function applySuggestions(draft: GameDraft, inspection: FolderInspection_Serialize): GameDraft {
	let next = draft;

	if (next.name.trim() === "") next = { ...next, name: inspection.name };

	if (next.id.trim() === "") next = { ...next, id: slugify(inspection.name) };

	if (next.executables.length === 0 && inspection.executables[0]) {
		const executable = inspection.executables[0];
		next = {
			...next,
			executables: [
				{
					name: executable.name.replace(EXE_SUFFIX, ""),
					path: executable.path,
				},
			],
		};
	}

	const detect = { ...next.detect };

	if (!detect.markers?.length && inspection.markers.length > 0) detect.markers = inspection.markers;

	if (!detect.steam_app_ids?.length && inspection.steam) detect.steam_app_ids = [inspection.steam.app_id];

	if (!detect.steam_install_dirs?.length && inspection.steam?.install_dir) {
		detect.steam_install_dirs = [inspection.steam.install_dir];
	}

	if (!detect.gog_ids?.length && inspection.stores.gog.length > 0) detect.gog_ids = inspection.stores.gog;

	if (!detect.epic_ids?.length && inspection.stores.epic.length > 0) detect.epic_ids = inspection.stores.epic;

	if (!detect.lutris_slugs?.length && inspection.stores.lutris.length > 0) detect.lutris_slugs = inspection.stores.lutris;
	next = { ...next, detect };
	const hasLayout = next.loader !== undefined || (next.deploy.slots?.length ?? 0) > 0 || (next.deploy.mirror?.length ?? 0) > 0;
	const installed = inspection.loaders[0];
	const engine = inspection.engine;

	// A loader that is already in the folder is the stronger signal; the engine of the folder is the fallback for a fresh one.
	if (!hasLayout && installed) {
		next = setLoader(next, installed.id, installed.variant ?? "");
	} else if (!hasLayout && engine) {
		next = setLoader(next, engine.loader_id, engine.variant ?? "");
	}

	// The project folder is the deployment root of the engine's own loader. A root the user set is kept.
	if (engine?.root && !next.deploy.root?.trim() && loaderId(next) === engine.loader_id) {
		next = { ...next, deploy: { ...next.deploy, root: `{game}/${engine.root}` } };
	}

	return next;
}
