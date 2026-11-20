import { type ReactNode, useEffect } from "react";
import { useNavigation } from "@/app/contexts/navigation-context";
import { commands, type TrayLabels } from "@/bindings";
import { useTranslation } from "@/i18n";
import { useTauriListener } from "@/lib/use-tauri-listener";

/// The native menu lives outside the webview, so `t()` cannot reach it. The translated strings are sent to Rust once
/// per language change; the codes in the maps are the snake_case values the backend emits.
export function trayLabels(t: ReturnType<typeof useTranslation>["t"]): TrayLabels {
	return {
		name: t("app.shortName"),
		open: t("tray.open"),
		settings: t("tray.settings"),
		quit: t("tray.quit"),
		undo: t("tray.undo"),
		stop: t("header.forceClose"),
		profiles: t("tray.profiles"),
		games: t("tray.games"),
		working: t("tray.working"),
		open_working: t("tray.openWorking"),
		no_game: t("tray.noGame"),
		no_tasks: t("tray.noTasks"),
		changes: t("tray.changes"),
		attention: t("tray.attention"),
		applied: t("header.deployed"),
		game_running: t("header.running"),
		percent: t("tray.percent"),
		tooltip: t("tray.tooltip"),
		next: {
			set_game_folder: t("header.setGameFolder"),
			set_settings_folder: t("header.setSettingsFolder"),
			set_mods_folder: t("header.setModsFolder"),
			fix_problems: t("header.fixProblems"),
			import_mods: t("tray.next.importMods"),
			enable_mods: t("tray.next.enableMods"),
			apply: t("header.applyAndLaunch"),
			play: t("header.launch"),
		},
		busy: {
			applying: t("header.busyApplying"),
			launching: t("header.busyLaunching"),
			stopping: t("header.busyStopping"),
			updating: t("header.busyUpdating"),
		},
		kinds: {
			apply: t("tasks.kind.apply"),
			import: t("tasks.kind.import"),
			update: t("tasks.kind.update"),
			"update-all": t("tasks.kind.update-all"),
			requirement: t("tasks.kind.requirement"),
			pack: t("tasks.kind.pack"),
			"pack-link": t("tasks.kind.pack-link"),
			collection: t("tasks.kind.collection"),
		},
		states: {
			running: t("tasks.group.running"),
			waiting: t("tasks.group.waiting"),
			failed: t("tasks.group.failed"),
		},
		quit_title: t("tray.quitTitle"),
		quit_game: t("tray.quitGame"),
		quit_tasks: t("tray.quitTasks"),
		quit_stop: t("tray.quitStop"),
		quit_cancel: t("common.cancel"),
		quit_leave: t("tray.quitLeave"),
		notice_title: t("app.shortName"),
		notice_waiting: t("tray.noticeWaiting"),
		notice_failed: t("tray.noticeFailed"),
	};
}

/// Mounted above the screens: pushes the labels and opens the screen a tray click asked for. The window itself is shown
/// by the backend, so this only navigates.
export function TrayBridge({ children }: { children: ReactNode }) {
	const { t } = useTranslation();
	const { navigate } = useNavigation();

	useEffect(() => {
		void (async () => {
			try {
				await commands.setTrayLabels(trayLabels(t));
			} catch {
				// A missing icon leaves the window fully usable; the next language change tries again.
			}
		})();
	}, [t]);

	useTauriListener(async (scope) => {
		scope.add(
			await scope.listen<string>("tray-open-view", (event) => {
				switch (event.payload) {
					case "working":
						window.dispatchEvent(new Event("gobbo:tray-working"));
						break;
					case "mods":
						navigate("mods");
						break;
					case "settings":
						navigate("settings");
						break;
					case "app-settings":
						navigate("app");
						break;
					default:
						break;
				}
			}),
		);
	}, navigate);

	return children;
}

/// The active game and the busy state live only in the window. The tray remembers the last report after the window is
/// hidden; sending on every change keeps its status and its primary action in step with the header.
export function useTrayContext(game: string | undefined, busy: string | null) {
	useEffect(() => {
		void (async () => {
			try {
				await commands.setTrayContext(game ?? null, busy);
			} catch {
				// The tray keeps its last context; the next state change tries again.
			}
		})();
	}, [game, busy]);
}
