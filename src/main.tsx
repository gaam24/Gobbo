import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app";
import { commands } from "./bindings";
import { applyLanguage } from "./i18n";
import { unwrap } from "./lib/ipc";
import "./index.css";

/// The webview's own menu (Back, Reload, Inspect Element) has no place in the app. Text fields keep it for copy and
/// paste, the app's own menus stop it before it gets here, and Shift+right-click still opens it in development.
/// A render error or a promise nobody handled goes to the backend log, the same file a support bundle carries. The
/// helper's catch keeps a failing log command from turning into another unhandled rejection.
async function logFrontendFailure(message: string, detail: string): Promise<void> {
	try {
		await commands.logFrontendError(message, detail);
	} catch {
		// Logging must never fail the page it is reporting on.
	}
}

window.addEventListener("error", (event) => {
	void logFrontendFailure(event.message, event.error instanceof Error ? (event.error.stack ?? "") : "");
});
window.addEventListener("unhandledrejection", (event) => {
	const reason = event.reason;
	const detail = reason instanceof Error ? (reason.stack ?? reason.message) : String(reason);
	void logFrontendFailure("unhandled rejection", detail);
});

window.addEventListener("contextmenu", (event) => {
	const editable = event.target instanceof Element && event.target.closest("input, textarea, [contenteditable='true']");
	if (editable || (import.meta.env.DEV && event.shiftKey)) return;
	event.preventDefault();
});

/// Reads the saved language before the first render, so the interface never flashes in the wrong language.
async function start() {
	try {
		const settings = await unwrap(commands.appSettings());
		await applyLanguage(settings.language);
	} catch {
		// Without the backend (a plain browser preview) the system language stays.
	}

	const root = document.getElementById("root");
	if (root) {
		if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("gallery")) {
			const { UiGallery } = await import("./dev/ui-gallery");
			ReactDOM.createRoot(root).render(<UiGallery />);
			return;
		}
		if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("settings-mockup")) {
			const { SettingsMockup } = await import("./dev/settings-mockup");
			ReactDOM.createRoot(root).render(<SettingsMockup />);
			return;
		}
		ReactDOM.createRoot(root).render(
			<React.StrictMode>
				<App />
			</React.StrictMode>,
		);
	}
}

void start();
