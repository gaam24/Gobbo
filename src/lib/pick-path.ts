import { e2eDialogs } from "@/lib/e2e-bridge";

/// A folder or a single file for a dialog that is opened from a lazy screen. The module stays out of the first paint: the
/// shell only needs the `pickFiles`/`pickSaveFile` of `lib/tauri`. The E2E build answers from a queue set by the suite;
/// a production build removes the branch.
const loadDialog = () => import("@tauri-apps/plugin-dialog");

/// Asks for one folder, for a game, a Proton build or a per-profile folder.
export async function pickFolder(): Promise<string | null> {
	const test = e2eDialogs();
	if (test?.folder !== undefined) return test.folder;
	const { open } = await loadDialog();
	const picked = await open({ directory: true, multiple: false });
	return typeof picked === "string" ? picked : null;
}

/// Asks for one file, for a per-profile single-file path.
export async function pickFile(): Promise<string | null> {
	const test = e2eDialogs();
	if (test?.file !== undefined) return test.file;
	const { open } = await loadDialog();
	const picked = await open({ multiple: false });
	return typeof picked === "string" ? picked : null;
}
