import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { useSettingsActions, useSettingsData } from "@/app/contexts/settings-context";
import { INSPECTOR_PREF, isWidth } from "@/app/prefs";
import type { PaletteItem } from "@/lib/palette";
import { readPref, writePref } from "@/lib/ui-prefs";
import { useMediaQuery } from "@/lib/use-media-query";
import type { UpdateTarget } from "@/sources/update-targets";

/// A modal that belongs to the app shell rather than to one screen. A discriminated union keeps the payload next to the
/// kind, so a dialog cannot be opened with the wrong data.
export type AppDialog =
	| { kind: "deploy"; launch: boolean; executable: string | null; request: number }
	| { kind: "overwrite" }
	| { kind: "pack-export" }
	| { kind: "pack-import"; path: string }
	| { kind: "pack-link"; link: string }
	| { kind: "collection" }
	| { kind: "definition-import"; path: string }
	| { kind: "manual-updates"; targets: UpdateTarget[] }
	| { kind: "remove-mod"; modId: string }
	| { kind: "remove-version"; modId: string; folder: string }
	| { kind: "remove-page"; pageId: string }
	| { kind: "reinstall"; modId: string; folder: string; name: string }
	| { kind: "archive-report"; folder: string; name: string }
	| { kind: "setup" }
	| { kind: "health" }
	| { kind: "sort" }
	| { kind: "community-profile" }
	| { kind: "snapshots" }
	| { kind: "conflicts" }
	| { kind: "shortcuts" }
	| { kind: "group-edit"; id?: string; members?: string[] }
	| { kind: "translation-winners"; groupId: string }
	| { kind: "translation-hints" };

export type DetailsTab = "overview" | "conflicts" | "files" | "sources";

interface UiState {
	/// The shell dialogs, oldest first. Only the last one is drawn; closing it brings the previous one back.
	dialogs: AppDialog[];
	/// The dialog on top, for a component that only needs to know whether one is open.
	dialog: AppDialog | null;
	palette: PaletteItem[] | null;
	consoleOpen: boolean;
	wide: boolean;
	drawerOpen: boolean;
	inspectorCollapsed: boolean;
	inspectorOpen: boolean;
	inspectorWidth: number | null;
	/// The tab the details column should show. `detailsRequest` changes even when the tab stays the same, so a second
	/// click on the same conflict counter still opens it.
	detailsTab: DetailsTab;
	detailsRequest: number;
}

const UiStateContext = createContext<UiState | null>(null);

export function useUiState(): UiState {
	const context = useContext(UiStateContext);
	if (!context) {
		throw new Error("useUiState must be used inside <UiProvider>.");
	}

	return context;
}

interface UiActions {
	/// Adds a dialog on top of the stack. It never replaces an open one.
	openDialog: (dialog: AppDialog) => void;
	/// Closes the dialog on top.
	closeDialog: () => void;
	/// Closes every shell dialog, for a change of game or a reset.
	closeAllDialogs: () => void;
	openPalette: (items: PaletteItem[]) => void;
	closePalette: () => void;
	setConsoleOpen: (open: boolean) => void;
	toggleConsole: () => void;
	setDrawerOpen: (open: boolean) => void;
	toggleInspector: () => void | Promise<void>;
	showDetails: (tab?: DetailsTab) => void;
	resizeInspector: (width: number | null) => void;
}

const UiActionsContext = createContext<UiActions | null>(null);

export function useUiActions(): UiActions {
	const context = useContext(UiActionsContext);
	if (!context) {
		throw new Error("useUiActions must be used inside <UiProvider>.");
	}

	return context;
}

/// The transient UI of the shell: dialogs, the palette, the console and the details column. These are not app settings
/// and do not belong in the workspace, which owns the active game.
export function UiProvider({ children }: { children: ReactNode }) {
	const { inspectorCollapsed } = useSettingsData();
	const { saveInspectorCollapsed } = useSettingsActions();
	const [dialogs, setDialogs] = useState<AppDialog[]>([]);
	const [palette, setPalette] = useState<PaletteItem[] | null>(null);
	const [consoleOpen, setConsoleOpen] = useState(false);
	const [drawerOpen, setDrawerOpen] = useState(false);
	const [detailsTab, setDetailsTab] = useState<DetailsTab>("overview");
	const [detailsRequest, setDetailsRequest] = useState(0);
	const [inspectorWidth, setInspectorWidth] = useState(() => readPref<number | null>(INSPECTOR_PREF, null, isWidth));
	const wide = useMediaQuery("(min-width: 80rem)");

	const openDialog = useCallback((next: AppDialog) => setDialogs((current) => [...current, next]), []);
	const closeDialog = useCallback(() => setDialogs((current) => current.slice(0, -1)), []);
	const closeAllDialogs = useCallback(() => setDialogs([]), []);
	const openPalette = useCallback((items: PaletteItem[]) => setPalette((current) => (current ? null : items)), []);
	const closePalette = useCallback(() => setPalette(null), []);
	const toggleConsole = useCallback(() => setConsoleOpen((open) => !open), []);
	const resizeInspector = useCallback((width: number | null) => {
		setInspectorWidth(width);
		writePref(INSPECTOR_PREF, width);
	}, []);

	/// A wide window folds the details column; a narrow one slides it in as a sheet. The width is remembered across starts.
	const toggleInspector = useCallback(() => {
		if (!wide) {
			setDrawerOpen((open) => !open);
			return Promise.resolve();
		}
		return saveInspectorCollapsed(!inspectorCollapsed);
	}, [inspectorCollapsed, saveInspectorCollapsed, wide]);
	const showDetails = useCallback(
		(tab: DetailsTab = "overview") => {
			setDetailsTab(tab);
			setDetailsRequest((request) => request + 1);
			if (!wide) setDrawerOpen(true);
			else if (inspectorCollapsed) void toggleInspector();
		},
		[inspectorCollapsed, toggleInspector, wide],
	);

	const state = useMemo(
		(): UiState => ({
			dialogs,
			dialog: dialogs.at(-1) ?? null,
			palette,
			consoleOpen,
			wide,
			drawerOpen,
			inspectorCollapsed,
			inspectorOpen: wide ? !inspectorCollapsed : drawerOpen,
			inspectorWidth,
			detailsTab,
			detailsRequest,
		}),
		[consoleOpen, detailsRequest, detailsTab, dialogs, drawerOpen, inspectorCollapsed, inspectorWidth, palette, wide],
	);
	const actions = useMemo(
		(): UiActions => ({
			openDialog,
			closeDialog,
			closeAllDialogs,
			openPalette,
			closePalette,
			setConsoleOpen,
			toggleConsole,
			setDrawerOpen,
			toggleInspector,
			showDetails,
			resizeInspector,
		}),
		[closeAllDialogs, closeDialog, closePalette, openDialog, openPalette, resizeInspector, showDetails, toggleConsole, toggleInspector],
	);

	return (
		<UiStateContext.Provider value={state}>
			<UiActionsContext.Provider value={actions}>{children}</UiActionsContext.Provider>
		</UiStateContext.Provider>
	);
}
