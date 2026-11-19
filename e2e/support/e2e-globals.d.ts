/// The globals that `tauri.e2e.conf.json` exposes through `withGlobalTauri`. The specs run in the webview, so the bridge
/// is read from the window, never imported from a Node module.
import type { E2eDialogOverrides, E2eTaskEvent as TaskEvent, E2eTaskPayload as TaskPayload } from "../../src/lib/e2e-contract";

declare global {
	interface TauriChannel {
		onmessage: (message: unknown) => void;
	}

	interface TauriBridge {
		core: {
			invoke: <T = unknown>(command: string, args?: Record<string, unknown>) => Promise<T>;
			Channel: new <T = unknown>() => TauriChannel;
		};
		event: {
			emit: (event: string, payload?: unknown) => Promise<void>;
		};
	}

	type E2eTaskEvent = TaskEvent;
	type E2eTaskPayload = TaskPayload;

	interface Window {
		__TAURI__?: TauriBridge;
		__GOBBO_E2E_DIALOGS__?: E2eDialogOverrides;
		__GOBBO_E2E_THROW__?: string | null;
		__GOBBO_E2E_TASKS__?: (event: E2eTaskEvent, payload: E2eTaskPayload) => Promise<void>;
	}
}
