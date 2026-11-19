/// The one place that knows how the window reaches the WebDriver seam. The helpers return `null`/`false` in a
/// production build; Vite replaces `MODE` at build time, which lets the bundler drop the test branches entirely.

import type { E2eDialogOverrides } from "@/lib/e2e-contract";

/// Whether this bundle was built with `--mode e2e`.
export const isE2e = import.meta.env.MODE === "e2e";

/// The dialog answers the suite installed, or `null` when native dialogs should be used.
export function e2eDialogs(): E2eDialogOverrides | null {
	return isE2e ? (window.__GOBBO_E2E_DIALOGS__ ?? null) : null;
}

/// Whether the suite armed a render failure for `scope` (`"shell"` or a view id).
export function e2eForcedFailure(scope: string): boolean {
	return isE2e && (window.__GOBBO_E2E_THROW__ === scope || sessionStorage.getItem("gobbo.e2e.throw") === scope);
}
