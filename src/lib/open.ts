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

/// Copies text to the clipboard, e.g. the path to a game folder. Says whether it worked.
export const copyText = (text: string, report: Report) =>
	reporting(() => navigator.clipboard.writeText(text), report, translateKey("clipboard.copyFailed"));
