import { createContext, type ReactNode, useContext, useMemo } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useAppSettings } from "@/app/use-app-settings";
import type { AppSettings_Serialize, CloseAction, SourceAccount, SourceLinkStatus, SourceProviderInfo, StorageInfo } from "@/bindings";
import type { LanguageSetting } from "@/locales/locale";

/// The app settings the interface reads. They change when the user edits them, not while a game is being modded.
interface SettingsData {
	dataFolder: string;
	storage: StorageInfo | null;
	/// The providers the build knows, and whether each key is saved (by provider id; only providers with a key have one).
	providers: SourceProviderInfo[];
	keysSaved: Record<string, boolean>;
	/// The account behind each checked key, as the settings show it.
	accounts: Record<string, SourceAccount>;
	/// The providers whose downloads the backend routes to the files page (`download_route`). The rest download in Gobbo.
	browserProviders: ReadonlySet<string>;
	/// The providers Gobbo can ask now: no key needed, or the key is saved.
	readyProviders: ReadonlySet<string>;
	language: LanguageSetting;
	/// The providers, by id, whose download links Gobbo takes.
	sourceLinks: string[];
	/// What the system does for each download-link scheme the settings take.
	linkStatus: SourceLinkStatus[];
	rememberWindow: boolean;
	closeAction: CloseAction;
	efficiencyMode: AppSettings_Serialize["efficiency_mode"];
	/// Whether the active profile's collection revisions are read again when a game opens.
	checkCollectionsOnLaunch: boolean;
	/// Whether the official catalog is read when the Games page opens.
	checkCatalogOnLaunch: boolean;
	/// Whether the window moves to the tray once a game starts.
	hideWhenGameLaunches: boolean;
	/// Whether notifications may be shown while the window is hidden.
	systemNotifications: boolean;
	/// The notification kinds the user turned off.
	mutedNotifications: string[];
	inspectorCollapsed: boolean;
}

const SettingsDataContext = createContext<SettingsData | null>(null);

export function useSettingsData(): SettingsData {
	const context = useContext(SettingsDataContext);

	if (!context) {
		throw new Error("useSettingsData must be used inside <SettingsProvider>.");
	}

	return context;
}

/// The settings the startup code loads once and the settings page writes back.
interface SettingsActions {
	loadAppSettings: (settings: AppSettings_Serialize) => void;
	saveInspectorCollapsed: (collapsed: boolean) => void | Promise<void>;
	saveSourceKey: (provider: string, key: string) => Promise<boolean>;
	openSourceKeyPage: (provider: string) => void;
	saveLanguage: (language: LanguageSetting) => void | Promise<void>;
	saveRememberWindow: (on: boolean) => void | Promise<void>;
	saveCloseAction: (action: CloseAction) => Promise<void>;
	saveEfficiencyMode: (mode: AppSettings_Serialize["efficiency_mode"]) => void | Promise<void>;
	saveCheckCollectionsOnLaunch: (on: boolean) => void | Promise<void>;
	saveCheckCatalogOnLaunch: (on: boolean) => void | Promise<void>;
	saveHideWhenGameLaunches: (on: boolean) => void | Promise<void>;
	saveSystemNotifications: (on: boolean) => void | Promise<void>;
	saveNotification: (code: string, on: boolean) => void | Promise<void>;
	saveSourceLinks: (providers: string[]) => void | Promise<void>;
	refreshSourceLinks: () => void | Promise<void>;
	ensureSourceLinks: () => void | Promise<void>;
	rememberSourceAccount: (provider: string, account: SourceAccount) => void;
	/// Reads the provider list again, so a changed download route shows.
	refreshProviders: () => Promise<void>;
	/// The download route from the last read, for an action that chains right after a key save.
	currentBrowserProviders: () => ReadonlySet<string>;
}

const SettingsActionsContext = createContext<SettingsActions | null>(null);

export function useSettingsActions(): SettingsActions {
	const context = useContext(SettingsActionsContext);

	if (!context) {
		throw new Error("useSettingsActions must be used inside <SettingsProvider>.");
	}

	return context;
}

/// App settings are shared by every game: the language, the provider keys, the download links, the window defaults and
/// the details column.
export function SettingsProvider({ children }: { children: ReactNode }) {
	const { notify, notifyError, attempt } = useNoticeActions();
	const {
		dataFolder,
		storage,
		providers,
		keysSaved,
		accounts,
		browserProviders,
		currentBrowserProviders,
		rememberSourceAccount,
		refreshProviders,
		readyProviders,
		language,
		sourceLinks,
		linkStatus,
		rememberWindow,
		closeAction,
		efficiencyMode,
		checkCollectionsOnLaunch,
		checkCatalogOnLaunch,
		hideWhenGameLaunches,
		systemNotifications,
		mutedNotifications,
		inspectorCollapsed,
		loadAppSettings,
		saveInspectorCollapsed,
		saveSourceKey,
		openSourceKeyPage,
		saveLanguage,
		saveRememberWindow,
		saveCloseAction,
		saveEfficiencyMode,
		saveCheckCollectionsOnLaunch,
		saveCheckCatalogOnLaunch,
		saveHideWhenGameLaunches,
		saveSystemNotifications,
		saveNotification,
		saveSourceLinks,
		refreshSourceLinks,
		ensureSourceLinks,
	} = useAppSettings({ notify, notifyError, attempt });

	const data = useMemo(
		(): SettingsData => ({
			dataFolder,
			storage,
			providers,
			keysSaved,
			accounts,
			browserProviders,
			readyProviders,
			language,
			sourceLinks,
			linkStatus,
			rememberWindow,
			closeAction,
			efficiencyMode,
			checkCollectionsOnLaunch,
			checkCatalogOnLaunch,
			hideWhenGameLaunches,
			systemNotifications,
			mutedNotifications,
			inspectorCollapsed,
		}),
		[
			accounts,
			browserProviders,
			checkCatalogOnLaunch,
			checkCollectionsOnLaunch,
			dataFolder,
			mutedNotifications,
			efficiencyMode,
			hideWhenGameLaunches,
			inspectorCollapsed,
			keysSaved,
			language,
			linkStatus,
			closeAction,
			providers,
			readyProviders,
			sourceLinks,
			rememberWindow,
			storage,
			systemNotifications,
		],
	);
	const actions = useMemo(
		(): SettingsActions => ({
			loadAppSettings,
			saveInspectorCollapsed,
			saveSourceKey,
			openSourceKeyPage,
			saveLanguage,
			saveRememberWindow,
			saveCloseAction,
			saveEfficiencyMode,
			saveCheckCollectionsOnLaunch,
			saveCheckCatalogOnLaunch,
			saveHideWhenGameLaunches,
			saveSystemNotifications,
			saveNotification,
			saveSourceLinks,
			refreshSourceLinks,
			ensureSourceLinks,
			rememberSourceAccount,
			refreshProviders,
			currentBrowserProviders,
		}),
		[
			currentBrowserProviders,
			ensureSourceLinks,
			loadAppSettings,
			openSourceKeyPage,
			refreshProviders,
			refreshSourceLinks,
			rememberSourceAccount,
			saveCheckCatalogOnLaunch,
			saveCheckCollectionsOnLaunch,
			saveEfficiencyMode,
			saveHideWhenGameLaunches,
			saveInspectorCollapsed,
			saveNotification,
			saveSystemNotifications,
			saveLanguage,
			saveRememberWindow,
			saveCloseAction,
			saveSourceKey,
			saveSourceLinks,
		],
	);

	return (
		<SettingsDataContext.Provider value={data}>
			<SettingsActionsContext.Provider value={actions}>{children}</SettingsActionsContext.Provider>
		</SettingsDataContext.Provider>
	);
}
