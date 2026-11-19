/// <reference types="vite/client" />
import type { E2eDialogOverrides, E2eTaskEvent, E2eTaskPayload } from "./lib/e2e-contract";

declare global {
	interface Window {
		/// Test-only dialog overrides, read only when the web bundle is built with `--mode e2e`.
		__GOBBO_E2E_DIALOGS__?: E2eDialogOverrides;
		/// Test-only render failure; `PageFor` throws for a matching view and `AppShell` for `shell`.
		__GOBBO_E2E_THROW__?: string | null;
		/// Test-only emitter for real Tauri task events, installed only in the `e2e` build.
		__GOBBO_E2E_TASKS__?: (event: E2eTaskEvent, payload: E2eTaskPayload) => Promise<void>;
	}
}
