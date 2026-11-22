import { commands } from "@/bindings";
import { translateKey } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";

type Report = (message: string) => void;

/// Runs a system action and sends its failure to `report` (the app's notices) instead of throwing. Says whether it worked.
async function reporting(action: () => Promise<unknown>, report: Report, failure?: string): Promise<boolean> {
	try {
		await action();
		return true;
	} catch (error) {
		report(failure ?? messageOf(error));
		return false;
	}
}

/// Opens a folder in the system file manager, or with `reveal` shows a file selected in its folder. The backend accepts
/// only Gobbo's data folder and the configured game folders, so a compromised webview cannot open an arbitrary path.
export async function openPath(path: string, gameId: string | null, reveal: boolean, report: Report) {
	await reporting(() => unwrap(commands.openPath(gameId, path, reveal)), report);
}

/// Writes text to the clipboard and hands the failure message to `report`. Says whether it worked. A caller chooses what
/// a failure means: a translated notice for a user action, or nothing for a view that keeps its text on screen.
export async function writeClipboard(text: string, report: Report): Promise<boolean> {
	try {
		await navigator.clipboard.writeText(text);
		return true;
	} catch (error) {
		report(messageOf(error));
		return false;
	}
}

/// Copies text to the clipboard, e.g. the path to a game folder. Says whether it worked.
export const copyText = (text: string, report: Report) => writeClipboard(text, () => report(translateKey("clipboard.copyFailed")));
