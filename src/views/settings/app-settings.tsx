import { FolderOpen, KeyRound, LifeBuoy, Link2Off, RefreshCw, Settings2 } from "lucide-react";
import { useState } from "react";
import type { AppSettings_Serialize, CloseAction, SourceAccount, SourceLinkStatus, SourceProviderInfo, StorageInfo } from "@/bindings";
import { SettingsLayout, type SettingsSection } from "@/components/app/page-layout";
import { PageFrame } from "@/components/page-frame";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { usePlatform, useTrayAvailable } from "@/lib/platform";
import type { LanguageSetting } from "@/locales/locale";
import type { KeyCheck } from "@/sources/key-check";
import { lostLinkSchemes } from "@/sources/links";
import { ProviderKeySection } from "@/sources/provider-key-section";
import { CloseActionSetting } from "@/views/settings/close-action-setting";
import { Section, SettingRow, SettingsGroup } from "@/views/settings/controls";
import { DataFolderSettings } from "@/views/settings/data-folder-settings";
import { EfficiencySetting } from "@/views/settings/efficiency-setting";
import { LanguageSettings } from "@/views/settings/language-settings";
import { StorageSettings } from "@/views/settings/storage-settings";
import { SupportSection } from "@/views/settings/support-section";
import {
	CheckCatalogSetting,
	CheckCollectionsSetting,
	HideWhenGameLaunchesSetting,
	ProviderLinkSetting,
	RememberWindowSetting,
	SystemNotificationsSetting,
} from "@/views/settings/toggle-settings";
import { UpdatesSection } from "@/views/settings/updates-section";

/// The providers as the settings show them: the list, whether each key is saved, and the providers whose download links
/// Gobbo takes.
export interface ProviderSettingsData {
	list: SourceProviderInfo[];
	keysSaved: Record<string, boolean>;
	/// The account behind each saved key that was checked; a missing entry is unknown.
	accounts: Record<string, SourceAccount>;
	links: string[];
	/// What the system does for the link schemes the settings take.
	status: SourceLinkStatus[];
}

/// What the provider rows do: save or check a key, open its page, and set the providers whose links Gobbo takes.
export interface ProviderSettingsActions {
	onSaveKey: (provider: string, key: string) => Promise<boolean>;
	onValidateKey: (provider: string, key: string | null) => Promise<KeyCheck>;
	onOpenKeyPage: (provider: string) => void;
	onLinks: (providers: string[]) => void | Promise<void>;
	/// Registers again the link schemes another program took over.
	onRetakeLinks: () => void | Promise<void>;
}

type AppSettingsTab = "interface" | "providers" | "files" | "updates" | "support";

/// The settings of Gobbo itself, shared by all games, split into the same vertical tabs as the game settings: the
/// interface, the mod providers, their download links, the files Gobbo keeps, updates and support.
export function AppSettings({
	language,
	onLanguage,
	providers,
	providerActions,
	rememberWindow,
	onRememberWindow,
	closeAction,
	onCloseAction,
	efficiencyMode,
	onEfficiencyMode,
	checkCollectionsOnLaunch,
	onCheckCollectionsOnLaunch,
	checkCatalogOnLaunch,
	onCheckCatalogOnLaunch,
	hideWhenGameLaunches,
	onHideWhenGameLaunches,
	systemNotifications,
	onSystemNotifications,
	mutedNotifications,
	onNotification,
	dataFolder,
	storage,
	onOpenFolder,
	onCopy,
	gameId,
}: {
	language: LanguageSetting;
	onLanguage: (language: LanguageSetting) => void | Promise<void>;
	providers: ProviderSettingsData;
	providerActions: ProviderSettingsActions;
	/// The main window's size and place, restored at the start when this is on.
	rememberWindow: boolean;
	onRememberWindow: (on: boolean) => void | Promise<void>;
	/// What closing the main window does, including "ask every time".
	closeAction: CloseAction;
	onCloseAction: (action: CloseAction) => void | Promise<void>;
	/// Windows 11 Efficiency mode. Other systems do not show the row.
	efficiencyMode: AppSettings_Serialize["efficiency_mode"];
	onEfficiencyMode: (mode: AppSettings_Serialize["efficiency_mode"]) => void | Promise<void>;
	/// The active profile's collection revisions are read again when a game opens.
	checkCollectionsOnLaunch: boolean;
	onCheckCollectionsOnLaunch: (on: boolean) => void | Promise<void>;
	/// The official catalog is read when the Games page opens.
	checkCatalogOnLaunch: boolean;
	onCheckCatalogOnLaunch: (on: boolean) => void | Promise<void>;
	/// The window moves to the tray once a game starts.
	hideWhenGameLaunches: boolean;
	onHideWhenGameLaunches: (on: boolean) => void | Promise<void>;
	/// System notifications and the individual events that produce them.
	systemNotifications: boolean;
	onSystemNotifications: (on: boolean) => void | Promise<void>;
	mutedNotifications: string[];
	onNotification: (code: string, on: boolean) => void | Promise<void>;
	/// The data folder, and the actions for it.
	dataFolder: string;
	/// The storage mode and the file that records it.
	storage: StorageInfo | null;
	onOpenFolder: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	/// The current game, or null when no game is added; the support zip names it.
	gameId: string | null;
}) {
	const { t } = useTranslation();
	/// The link handler is registered on Windows and Linux; macOS takes its links from the app bundle.
	const platform = usePlatform();
	const tray = useTrayAvailable();
	const linkProviders = providers.list.filter((provider) => provider.schemes.length > 0);
	const lostSchemes = lostLinkSchemes(providers.status);
	const [tab, setTab] = useState<AppSettingsTab>("interface");

	const tabs: Array<SettingsSection<AppSettingsTab>> = [
		{
			id: "interface",
			label: t("settings.groups.interface"),
			icon: Settings2,
			content: (
				<Section id="settings-app-interface" title={t("settings.groups.interface")}>
					<LanguageSettings language={language} onLanguage={onLanguage} />
					<RememberWindowSetting on={rememberWindow} onChange={onRememberWindow} />
					<CloseActionSetting action={closeAction} tray={tray} onAction={onCloseAction} />
					{tray && <HideWhenGameLaunchesSetting on={hideWhenGameLaunches} onChange={onHideWhenGameLaunches} />}
					<SystemNotificationsSetting
						on={systemNotifications}
						muted={mutedNotifications}
						onChange={onSystemNotifications}
						onNotification={onNotification}
					/>
					<CheckCollectionsSetting on={checkCollectionsOnLaunch} onChange={onCheckCollectionsOnLaunch} />
					<CheckCatalogSetting on={checkCatalogOnLaunch} onChange={onCheckCatalogOnLaunch} />
					{platform === "windows" && <EfficiencySetting mode={efficiencyMode} onMode={onEfficiencyMode} />}
				</Section>
			),
		},
		{
			id: "providers",
			label: t("settings.groups.providers"),
			icon: KeyRound,
			content: (
				<Section id="settings-app-providers" title={t("settings.groups.providers")} hint={t("settings.groups.providersHint")}>
					{providers.list.map((provider, index) => {
						const showLinks = platform !== "macos" && provider.schemes.length > 0;
						return (
							<SettingsGroup key={provider.id} title={provider.name} className={index === 0 ? undefined : "mt-10"}>
								{provider.needs_key ? (
									<ProviderKeySection
										provider={provider}
										title={t("providers.key.titleShort")}
										hasKey={Boolean(providers.keysSaved[provider.id])}
										account={providers.accounts[provider.id]}
										onSave={(key) => providerActions.onSaveKey(provider.id, key)}
										onValidate={(key) => providerActions.onValidateKey(provider.id, key)}
										onOpenKeyPage={() => providerActions.onOpenKeyPage(provider.id)}
									/>
								) : (
									<SettingRow title={t("providers.key.titleShort")}>
										<p className="text-muted-foreground text-sm">{t("providers.key.noKey", { name: provider.name })}</p>
									</SettingRow>
								)}
								{showLinks && (
									<ProviderLinkSetting
										provider={provider}
										on={providers.links.includes(provider.id)}
										onChange={(on) =>
											providerActions.onLinks(on ? [...providers.links, provider.id] : providers.links.filter((id) => id !== provider.id))
										}
									/>
								)}
							</SettingsGroup>
						);
					})}
				</Section>
			),
		},
		{
			id: "files",
			label: t("settings.groups.files"),
			icon: FolderOpen,
			content: (
				<Section id="settings-app-files" title={t("settings.groups.files")} hint={t("settings.groups.filesHint")}>
					<StorageSettings storage={storage} />
					<DataFolderSettings path={dataFolder} onOpen={onOpenFolder} onCopy={onCopy} />
				</Section>
			),
		},
		{
			id: "updates",
			label: t("settings.groups.updates"),
			icon: RefreshCw,
			content: (
				<Section id="settings-app-updates" title={t("settings.groups.updates")}>
					<UpdatesSection />
				</Section>
			),
		},
		{
			id: "support",
			label: t("settings.groups.support"),
			icon: LifeBuoy,
			content: (
				<Section id="settings-app-support" title={t("settings.groups.support")} hint={t("settings.groups.supportHint")}>
					<SupportSection gameId={gameId} />
				</Section>
			),
		},
	];

	const active = tabs.some((entry) => entry.id === tab) ? tab : (tabs[0]?.id ?? "interface");

	return (
		<PageFrame title={t("app.settings")} hint={t("settings.appHint")} padded>
			{platform !== "macos" && linkProviders.length > 0 && lostSchemes.length > 0 && (
				<Alert variant="destructive">
					<Link2Off aria-hidden />
					<AlertTitle>{t("providers.links.lostTitle")}</AlertTitle>
					<AlertDescription className="grid gap-2">
						<span>{t("providers.links.lost", { schemes: lostSchemes.join(", ") })}</span>
						<Button size="sm" variant="outline" className="justify-self-start" onClick={() => void providerActions.onRetakeLinks()}>
							{t("providers.links.retake")}
						</Button>
					</AlertDescription>
				</Alert>
			)}
			<SettingsLayout label={t("settings.nav.label")} sections={tabs} value={active} onValueChange={setTab} />
		</PageFrame>
	);
}
