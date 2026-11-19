import { Channel } from "@tauri-apps/api/core";
import { e2eDialogs } from "@/lib/e2e-bridge";

/// Tauri modules that are only needed after the first paint or after a user action. The imports stay
/// outside components so React Compiler can still optimize the callers.
export const loadWindow = () => import("@tauri-apps/api/window");
export const loadWebview = () => import("@tauri-apps/api/webview");
export const loadEvents = () => import("@tauri-apps/api/event");
const loadDialog = () => import("@tauri-apps/plugin-dialog");

/// A progress channel whose steps go to `onStep`. Tauri's Channel class calls `onmessage`, which is not a DOM event handler.
export function newProgressChannel<T>(onStep: (step: T) => void): Channel<T> {
	const channel = new Channel<T>();
	// oxlint-disable-next-line unicorn/prefer-add-event-listener
	channel.onmessage = onStep;
	return channel;
}

/// Asks where to save a file of one kind. `null` when the user cancels.
export async function pickSaveFile(defaultPath: string, name: string, extension: string): Promise<string | null> {
	const test = e2eDialogs();
	if (test?.saveFile !== undefined) return test.saveFile;
	const { save } = await loadDialog();
	return save({ defaultPath, filters: [{ name, extensions: [extension] }] });
}

/// Asks for files of the given kinds. Empty when the user cancels.
export async function pickFiles(name: string, extensions: string[], multiple: boolean): Promise<string[]> {
	const test = e2eDialogs();
	// A defined array is a test answer, even when it is empty: that is a cancelled dialog. `splice` consumes the queue, so
	// one click cannot reuse a path that a later dialog should see.
	if (test?.files !== undefined) return multiple ? test.files.splice(0) : test.files.splice(0, 1);
	const { open } = await loadDialog();
	const picked = await open({ multiple, filters: [{ name, extensions }] });
	if (picked === null) return [];
	return typeof picked === "string" ? [picked] : picked;
}
