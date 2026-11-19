import { ActivityCenter } from "@/app/shell/activity-center";
import { AppDialogs } from "@/app/shell/app-dialogs";
import { AppMain } from "@/app/shell/app-main";
import { CommandPaletteHost } from "@/app/shell/command-palette-host";
import { FileDropHost } from "@/app/shell/file-drop-host";
import { GameRailHost } from "@/app/shell/game-rail-host";
import { ModsInspectorHost } from "@/app/shell/mods-inspector-host";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";
import { e2eForcedFailure, isE2e } from "@/lib/e2e-bridge";
import { loadEvents } from "@/lib/tauri";

/// The task spec needs the window's own event path, not an event injected from WebDriver. The e2e build exposes a real
/// Tauri emitter with the backend task payload shape, so the registry listener consumes the same events it does in the
/// app. The production build drops this branch entirely.
if (isE2e) {
	// oxlint-disable-next-line eslint/no-underscore-dangle -- the hook follows the existing __GOBBO_E2E_* naming convention.
	window.__GOBBO_E2E_TASKS__ = async (event, payload) => {
		const { emit } = await loadEvents();
		await emit(event, payload);
	};
}

/// The shell: the rail, the main column, the details column and every overlay that can appear above them. It has no
/// game logic of its own; the providers own the state and the pages own the screens.
export function AppShell() {
	const { t } = useTranslation();
	// The E2E build can force a failure above every page boundary, to check the whole-window fallback as well. The
	// session copy survives a reload, so the spec can arm the failure before the window starts.
	if (e2eForcedFailure("shell")) {
		throw new Error("E2E forced render failure in the shell");
	}
	return (
		<TooltipProvider>
			<Toaster position="top-right" closeButtonAriaLabel={t("toasts.dismiss")} containerAriaLabel={t("notifications.title")} />
			<CommandPaletteHost />
			<div className="bg-background text-foreground flex h-screen overflow-hidden">
				<GameRailHost />
				<main className="flex min-h-0 min-w-0 flex-1 flex-col">
					<AppMain />
				</main>
				<ModsInspectorHost />
			</div>
			<ActivityCenter />
			<AppDialogs />
			<FileDropHost />
		</TooltipProvider>
	);
}
