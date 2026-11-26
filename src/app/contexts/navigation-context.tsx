import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useUnsavedChanges } from "@/app/contexts/unsaved-context";
import { type AppView, isMainView, type MainView, VIEW_PREF } from "@/app/prefs";
import { readPref, writePref } from "@/lib/ui-prefs";

interface Navigation {
	view: AppView;
	navigate: (view: AppView) => void;
	/// Goes to the screen before this one. A screen opened from another has a way back without a URL router.
	back: () => void;
	canGoBack: boolean;
	/// Goes to the screen the last `back` left, like a browser's forward button. A new screen clears it.
	forward: () => void;
	canGoForward: boolean;
	openMods: () => void;
	openGameSettings: () => void;
	openGames: () => void;
	openAppSettings: () => void;
	openDownloads: () => void;
	openNotifications: () => void;
	openStorage: () => void;
}

const NavigationContext = createContext<Navigation | null>(null);

export function useNavigation(): Navigation {
	const context = useContext(NavigationContext);

	if (!context) {
		throw new Error("useNavigation must be used inside <NavigationProvider>.");
	}

	return context;
}

/// The navigation for a screen header, or `null` outside the provider. A header is drawn in server-rendered tests
/// too, where there is no history to go back to.
export function useOptionalNavigation(): Navigation | null {
	return useContext(NavigationContext);
}

/// The screen the user is on. It is deliberately a small typed router rather than a URL router: the Tauri window has no
/// address bar, and the active game is the only piece of navigation worth keeping across starts. If the app later needs
/// deep links or several windows, this provider is the one place that would be replaced with TanStack Router.
export function NavigationProvider({ children }: { children: ReactNode }) {
	const { requestLeave } = useUnsavedChanges();
	const [view, setView] = useState<AppView>(() => readPref<MainView>(VIEW_PREF, "mods", isMainView));
	const [history, setHistory] = useState<AppView[]>([]);
	const [future, setFuture] = useState<AppView[]>([]);
	/// The current view for `navigate`, so it keeps one identity across screens. It is read in event handlers, never
	/// during render; the effect keeps it in step when `setView` is called from somewhere else.
	const viewRef = useRef(view);
	useEffect(() => {
		viewRef.current = view;
	}, [view]);

	/// Only the two main views are remembered. A settings page or an overlay opened on demand should not be the first
	/// thing the user sees after a restart.
	useEffect(() => {
		if (view === "mods" || view === "games") writePref(VIEW_PREF, view);
	}, [view]);

	const navigate = useCallback(
		(next: AppView) => {
			const current = viewRef.current;

			if (current === next) return;
			requestLeave(() => {
				viewRef.current = next;
				setHistory((previous) => [...previous, current]);
				setFuture([]);
				setView(next);
			});
		},
		[requestLeave],
	);
	/// Back pops the last screen and keeps the one it leaves for `forward`, so the mouse's back and forward buttons and
	/// Alt+arrows move through the screens the way they do in a browser.
	const back = useCallback(() => {
		const previous = history.at(-1);

		if (previous === undefined) return;
		requestLeave(() => {
			setFuture((next) => [...next, viewRef.current]);
			viewRef.current = previous;
			setHistory(history.slice(0, -1));
			setView(previous);
		});
	}, [history, requestLeave]);
	const forward = useCallback(() => {
		const next = future.at(-1);

		if (next === undefined) return;
		requestLeave(() => {
			setHistory((previous) => [...previous, viewRef.current]);
			viewRef.current = next;
			setFuture(future.slice(0, -1));
			setView(next);
		});
	}, [future, requestLeave]);
	const openMods = useCallback(() => navigate("mods"), [navigate]);
	const openGameSettings = useCallback(() => navigate("settings"), [navigate]);
	const openGames = useCallback(() => navigate("games"), [navigate]);
	const openAppSettings = useCallback(() => navigate("app"), [navigate]);
	const openDownloads = useCallback(() => navigate("downloads"), [navigate]);
	const openNotifications = useCallback(() => navigate("notifications"), [navigate]);
	const openStorage = useCallback(() => navigate("storage"), [navigate]);

	const value = useMemo(
		(): Navigation => ({
			view,
			navigate,
			back,
			canGoBack: history.length > 0,
			forward,
			canGoForward: future.length > 0,
			openMods,
			openGameSettings,
			openGames,
			openAppSettings,
			openDownloads,
			openNotifications,
			openStorage,
		}),
		[
			back,
			forward,
			future.length,
			history.length,
			navigate,
			openAppSettings,
			openDownloads,
			openGameSettings,
			openGames,
			openMods,
			openNotifications,
			openStorage,
			view,
		],
	);

	return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}
