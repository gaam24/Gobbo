import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLinkFocus } from "@/app/hooks/use-link-focus";
import {
	type AppSettings_Serialize,
	type AppSettings as AppSettingsData,
	type CloseAction,
	commands,
	type SourceAccount,
	type SourceLinkStatus,
	type SourceLinksReport,
	type SourceProviderInfo,
	type StorageInfo,
} from "@/bindings";
import { applyLanguage, useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { withMuted } from "@/lib/notifications";
import { nameOfId, withoutKey } from "@/lib/records";
import { createQueue } from "@/lib/serial-queue";
import type { useNotices } from "@/lib/use-notices";
import type { LanguageSetting } from "@/locales/locale";
import { browserProviders } from "@/sources/access";
import { lostLinkProviders } from "@/sources/links";
import { readyProviders } from "@/sources/ready";

/// App settings are read first and written back with one change, so saving one keeps the others.
async function updateAppSettings(changes: Partial<AppSettingsData>) {
	const current = await unwrap(commands.appSettings());
	await unwrap(commands.saveAppSettings({ ...current, ...changes }));
}

/// The name of a provider for a message, or its id when the list does not know it. A minimal structural type keeps this
/// independent of the settings-context/sources module cycle.
function namesOf(providers: ReadonlyArray<{ id: string; name: string }>, ids: string[]) {
	return ids.map((id) => nameOfId(providers, id)).join(", ");
}

/// The providers whose links are lost, as one key. A notice is repeated only when this key changes.
function lostKey(statuses: SourceLinkStatus[]) {
	return lostLinkProviders(statuses).toSorted().join(",");
}

/// Starts a new account generation for a provider and returns it. A check that started earlier sees a different number
/// and does not write its account back.
function nextAccountGeneration(generations: { current: Record<string, number> }, provider: string): number {
	const next = (generations.current[provider] ?? 0) + 1;
	generations.current = { ...generations.current, [provider]: next };
	return next;
}

/// The app's settings: the providers and their keys (shared by all games), the language, the download links, the window
/// and the details column. Each change is written with the others and shown at once. The data folder is read once for the
/// settings page.
export function useAppSettings({
	notify,
	notifyError,
	attempt,
}: Pick<ReturnType<typeof useNotices>, "notify" | "notifyError" | "attempt">) {
	const { t } = useTranslation();
	const [providers, setProviders] = useState<SourceProviderInfo[]>([]);
	/// The provider list as of the last read, available synchronously to an action that chains on a key save. React may
	/// not have committed the new list when that action runs, so it must not rely on the render-time `browserProviders`.
	const providersRef = useRef<SourceProviderInfo[]>([]);
	const [keysSaved, setKeysSaved] = useState<Record<string, boolean>>({});
	const [sourceLinks, setSourceLinks] = useState<string[]>([]);
	const [linkStatus, setLinkStatus] = useState<SourceLinkStatus[]>([]);
	const [language, setLanguage] = useState<LanguageSetting>("system");
	const [rememberWindow, setRememberWindow] = useState(true);
	const [closeAction, setCloseAction] = useState<CloseAction>("ask");
	const [efficiencyMode, setEfficiencyMode] = useState<AppSettings_Serialize["efficiency_mode"]>("auto");
	const [checkCollectionsOnLaunch, setCheckCollectionsOnLaunch] = useState(true);
	const [checkCatalogOnLaunch, setCheckCatalogOnLaunch] = useState(true);
	const [hideWhenGameLaunches, setHideWhenGameLaunches] = useState(false);
	const [systemNotifications, setSystemNotifications] = useState(true);
	const [mutedNotifications, setMutedNotifications] = useState<string[]>([]);
	/// The list the last switch sent. Two quick clicks both build on it instead of on a render that has not happened yet.
	const mutedSent = useRef<string[]>([]);
	const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
	const [dataFolder, setDataFolder] = useState("");
	const [storage, setStorage] = useState<StorageInfo | null>(null);
	/// The account behind each saved key, as the last successful check described it. An absent provider is unknown: the
	/// first update still tries a direct download.
	const [accounts, setAccounts] = useState<Record<string, SourceAccount>>({});
	/// A check that started before a key was saved or cleared must not put the old account back.
	const accountGeneration = useRef<Record<string, number>>({});
	const reportedLost = useRef("");

	/// Reads the provider list again. Its `download_route` follows the saved account, which a key save, a check or a
	/// refused paid download changes in the backend. A failed read keeps the list it has.
	const refreshProviders = useCallback(async () => {
		try {
			const list = await commands.sourceProviders();
			providersRef.current = list;
			setProviders(list);
		} catch {
			// the last list stays; the next check or save reads it again
		}
	}, []);

	const rememberSourceAccount = useCallback((provider: string, account: SourceAccount) => {
		nextAccountGeneration(accountGeneration, provider);
		setAccounts((current) => ({ ...current, [provider]: account }));
	}, []);

	const forgetSourceAccount = useCallback((provider: string) => {
		nextAccountGeneration(accountGeneration, provider);
		setAccounts((current) => {
			if (!(provider in current)) return current;
			return withoutKey(current, provider);
		});
	}, []);

	/// Checks a provider key without a notice. The startup and the key save call it; a failed check leaves the account
	/// unknown. Validation caches the tier in the backend, which changes the download route, so the list is read again
	/// before this resolves: a caller that chains on the promise then sees the new route.
	const refreshSourceAccount = useCallback(
		async (provider: string, key: string | null): Promise<SourceAccount | null> => {
			const generation = nextAccountGeneration(accountGeneration, provider);

			try {
				const account = await unwrap(commands.validateSourceKey(provider, key));

				if (accountGeneration.current[provider] !== generation) return null;
				rememberSourceAccount(provider, account);
				await refreshProviders();
				return account;
			} catch {
				return null;
			}
		},
		[refreshProviders, rememberSourceAccount],
	);

	/// The providers the backend routes to the page instead of a direct download. Derived from the list, not kept apart.
	const browserOnly = useMemo(() => browserProviders(providers), [providers]);

	/// The same decision from the last read rather than from the last render. An action that just saved a key uses this,
	/// because awaiting the save does not mean React has committed the new context value yet.
	const currentBrowserProviders = useCallback(() => browserProviders(providersRef.current), []);

	/// Settings are read and written as one object. Two quick changes must not read the same old object and overwrite
	/// each other, so writes run one after another.
	const [queue] = useState(createQueue);
	// A failed write must not stop the next one; its error still reaches `attempt` through the promise `run` returns.
	const update = useCallback((changes: Partial<AppSettingsData>) => queue.run(() => updateAppSettings(changes)), [queue]);

	/// Saves one app setting with the others, then shows it. A failure is reported by `attempt` and the shown value stays.
	const saveSetting = (changes: Partial<AppSettingsData>, show: () => void | Promise<void>) =>
		void attempt(async () => {
			await update(changes);
			await show();
		});

	useEffect(() => {
		void (async () => {
			try {
				const [folder, info] = await Promise.all([commands.dataFolder(), unwrap(commands.storageInfo())]);
				setDataFolder(folder);
				setStorage(info);
			} catch {
				// without the folder the settings page shows no path; nothing else depends on it
			}
		})();
	}, []);

	/// What one pass over the download links changed: the providers it took back, and the ones it still could not. A pass
	/// with `repeat: false` reports the lost providers only when they differ from the last report, so refocusing does not
	/// repeat the same error.
	const applyLinkReport = useCallback(
		(
			report: SourceLinksReport,
			names: SourceProviderInfo[],
			{
				lostMessage,
				repeat,
			}: {
				lostMessage: "notice.sourceLinksLost" | "notice.sourceLinksStillLost";
				repeat: boolean;
			},
		) => {
			setLinkStatus(report.statuses);

			if (report.repaired.length > 0)
				notify(
					t("notice.sourceLinksRepaired", {
						names: namesOf(names, report.repaired),
					}),
				);
			const lost = lostLinkProviders(report.statuses);
			const key = lostKey(report.statuses);
			const changed = key !== reportedLost.current;
			reportedLost.current = key;

			if (lost.length > 0 && (repeat || changed)) notifyError(t(lostMessage, { names: namesOf(names, lost) }));
		},
		[notify, notifyError, t],
	);

	/// The providers come from the build, and each key stays in the credential store. Both are read once, at the start. A
	/// failure is reported, because a provider's saved key decides whether update checks can run. The download links are checked
	/// in the same pass, so another program that took them is replaced before the first link arrives.
	const startupLinks = useRef(false);
	useEffect(() => {
		void attempt(async () => {
			const [list, report] = await Promise.all([commands.sourceProviders(), unwrap(commands.ensureSourceLinks())]);
			providersRef.current = list;
			setProviders(list);

			if (startupLinks.current) {
				setLinkStatus(report.statuses);
			} else {
				startupLinks.current = true;
				applyLinkReport(report, list, {
					lostMessage: "notice.sourceLinksLost",
					repeat: true,
				});
			}

			const saved = await Promise.all(
				list
					.filter((provider) => provider.needs_key)
					.map(async (provider) => [provider.id, await unwrap(commands.sourceKeySaved(provider.id))] as const),
			);
			const savedMap = Object.fromEntries(saved);
			setKeysSaved(savedMap);
			// The saved key decides whether an update downloads or opens the files page. One request per provider, in the
			// background: a failure leaves the account unknown and the update path tries the direct download first.
			await Promise.all(
				list.filter((provider) => provider.needs_key && savedMap[provider.id]).map((provider) => refreshSourceAccount(provider.id, null)),
			);
		});
	}, [applyLinkReport, attempt, refreshSourceAccount]);

	/// Reads what the system does now without taking anything back. The settings page calls this when it opens.
	const refreshSourceLinks = useCallback(() => {
		void attempt(async () => {
			const status = await unwrap(commands.sourceLinkStatus());
			setLinkStatus(status);
			reportedLost.current = lostKey(status);
		});
	}, [attempt]);

	/// Takes back every download link the settings give Gobbo. A failure stays a warning in the settings.
	const ensureSourceLinks = useCallback(() => {
		void attempt(async () => {
			const report = await unwrap(commands.ensureSourceLinks());
			applyLinkReport(report, providers, {
				lostMessage: "notice.sourceLinksStillLost",
				repeat: true,
			});
		});
	}, [applyLinkReport, attempt, providers]);

	/// A takeover while the window was in the background is repaired when the user comes back. A repair says so; a
	/// failure is reported once per new set of lost links, so refocusing does not repeat the same error.
	const checkSourceLinksOnFocus = useCallback(() => {
		void attempt(async () => {
			const report = await unwrap(commands.ensureSourceLinks());
			applyLinkReport(report, providers, {
				lostMessage: "notice.sourceLinksLost",
				repeat: false,
			});
		});
	}, [applyLinkReport, attempt, providers]);
	useLinkFocus(checkSourceLinksOnFocus);

	/// Applies the settings read at startup.
	const loadAppSettings = useCallback((settings: AppSettings_Serialize) => {
		setSourceLinks(settings.source_links);
		setLanguage(settings.language);
		setRememberWindow(settings.remember_window);
		setCloseAction(settings.close_action);
		setEfficiencyMode(settings.efficiency_mode);
		setCheckCollectionsOnLaunch(settings.check_collections_on_launch);
		setCheckCatalogOnLaunch(settings.check_catalog_on_launch);
		setHideWhenGameLaunches(settings.hide_when_game_launches);
		setSystemNotifications(settings.system_notifications);
		setMutedNotifications(settings.muted_notifications);
		mutedSent.current = settings.muted_notifications;
		setInspectorCollapsed(settings.inspector_collapsed);
	}, []);

	/// The keys are shared by all games, so they are saved outside `forGame`. The boolean lets the field stay filled
	/// when the credential store rejects the key.
	const saveSourceKey = (provider: string, key: string) =>
		attempt(async () => {
			await unwrap(commands.saveSourceKey(provider, key));
			setKeysSaved((saved) => ({ ...saved, [provider]: Boolean(key) }));
			forgetSourceAccount(provider);
			notify(
				t(key ? "notice.sourceKeySaved" : "notice.sourceKeyRemoved", {
					name: namesOf(providers, [provider]),
				}),
			);
			await refreshProviders();
			// Wait for validation and the route refresh too: an update click right after saving must see the cached tier.
			if (key) await refreshSourceAccount(provider, key);
		});

	/// Opens the page where the user makes the key, in the browser. The backend knows the address.
	const openSourceKeyPage = (provider: string) =>
		void attempt(async () => {
			await unwrap(commands.openSourceKeyPage(provider));
		});

	/// Saves whether the details column is folded. The change shows at once and is kept across restarts.
	const saveInspectorCollapsed = (collapsed: boolean) => {
		saveSetting({ inspector_collapsed: collapsed }, () => setInspectorCollapsed(collapsed));
	};

	/// The language is an app setting: it is saved with the others, and the interface switches at once.
	const saveLanguage = (value: LanguageSetting) => {
		saveSetting({ language: value }, () => {
			setLanguage(value);
			return applyLanguage(value);
		});
	};

	/// The window's size and place come back at the next start, so the change shows then.
	const saveRememberWindow = (on: boolean) => {
		saveSetting({ remember_window: on }, () => setRememberWindow(on));
	};

	/// What closing the window does. The backend reads the saved choice on the next window close. It resolves once the
	/// file is written, so the closing dialog can save a remembered "exit" before the app ends.
	const saveCloseAction = async (action: CloseAction) => {
		await attempt(async () => {
			await update({ close_action: action });
			setCloseAction(action);
		});
	};

	/// Windows 11 Efficiency mode is an app setting: the backend applies it at once.
	const saveEfficiencyMode = (mode: AppSettings_Serialize["efficiency_mode"]) => {
		saveSetting({ efficiency_mode: mode }, () => setEfficiencyMode(mode));
	};

	/// Collection revisions are checked when a game opens. The change takes effect at the next game open.
	const saveCheckCollectionsOnLaunch = (on: boolean) => {
		saveSetting({ check_collections_on_launch: on }, () => setCheckCollectionsOnLaunch(on));
	};

	/// The official catalog is checked when the Games page opens. A change is read at the next page open.
	const saveCheckCatalogOnLaunch = (on: boolean) => {
		saveSetting({ check_catalog_on_launch: on }, () => setCheckCatalogOnLaunch(on));
	};

	/// The next game launch reads this choice. Setting it takes effect only after a game actually starts.
	const saveHideWhenGameLaunches = (on: boolean) => {
		saveSetting({ hide_when_game_launches: on }, () => setHideWhenGameLaunches(on));
	};

	const saveSystemNotifications = (on: boolean) => {
		saveSetting({ system_notifications: on }, () => setSystemNotifications(on));
	};

	/// One switch of one notification, by its code (see `@/lib/notifications`).
	const saveNotification = (code: string, on: boolean) => {
		const next = withMuted(mutedSent.current, code, on);
		mutedSent.current = next;
		saveSetting({ muted_notifications: next }, () => setMutedNotifications(next));
	};

	/// Takes the download links of the given providers. A provider added registers its links with the system; one left
	/// out gives them up.
	const saveSourceLinks = (taken: string[]) => {
		const added = taken.filter((id) => !sourceLinks.includes(id));
		const removed = sourceLinks.filter((id) => !taken.includes(id));
		void attempt(async () => {
			await update({ source_links: taken });
			setSourceLinks(taken);

			if (added.length > 0) notify(t("notice.sourceLinksOn", { names: namesOf(providers, added) }));

			if (removed.length > 0) notify(t("notice.sourceLinksOff", { names: namesOf(providers, removed) }));
			const status = await unwrap(commands.sourceLinkStatus());
			setLinkStatus(status);
			reportedLost.current = lostKey(status);
		});
	};

	/// A new set only when the providers or the keys change: the memoized list rows compare it by identity.
	const ready = useMemo(() => readyProviders(providers, keysSaved), [providers, keysSaved]);

	return {
		dataFolder,
		storage,
		providers,
		keysSaved,
		accounts,
		browserProviders: browserOnly,
		currentBrowserProviders,
		rememberSourceAccount,
		refreshProviders,
		readyProviders: ready,
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
	};
}
