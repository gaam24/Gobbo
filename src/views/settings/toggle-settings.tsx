import { APP_NOTIFICATIONS, NOTIFICATION_OUTCOMES, type SourceProviderInfo, TASK_KINDS } from "@/bindings";
import { Switch } from "@/components/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useTranslation } from "@/i18n";
import { taskNotification } from "@/lib/notifications";
import { SettingRow } from "@/views/settings/controls";

/// Whether Gobbo takes the download links of one provider (for example `nxm://` for Nexus Mods). Each provider has its
/// own switch, off until the user turns it on, because only one program on the computer can take the links. The switch
/// has no text of its own: the sentence that says what it does is the row's hint.
export function ProviderLinkSetting({
	provider,
	on,
	onChange,
}: {
	provider: SourceProviderInfo;
	on: boolean;
	onChange: (on: boolean) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const id = `${provider.id}-links`;
	const toggle = t("providers.links.toggle", { name: provider.name });
	return (
		<SettingRow
			title={t("providers.links.title")}
			htmlFor={id}
			hint={toggle}
			help={t("providers.links.hint", {
				name: provider.name,
				scheme: provider.schemes.join(", "),
			})}
		>
			<Switch id={id} aria-label={toggle} checked={on} onCheckedChange={(checked) => void onChange(checked)} />
		</SettingRow>
	);
}

/// Whether the main window opens where it was closed. It is on from the start; a change is read at the next start.
export function RememberWindowSetting({ on, onChange }: { on: boolean; onChange: (on: boolean) => void | Promise<void> }) {
	const { t } = useTranslation();
	const toggle = t("settings.window.toggle");
	return (
		<SettingRow title={t("settings.window.title")} htmlFor="remember-window" hint={toggle} help={t("settings.window.hint")}>
			<Switch id="remember-window" aria-label={toggle} checked={on} onCheckedChange={(checked) => void onChange(checked)} />
		</SettingRow>
	);
}

/// Whether a game open reads the current revision of the active profile's collections. One read per collection, through
/// the provider that owns it; the manual check in the collection menu works either way.
export function CheckCollectionsSetting({ on, onChange }: { on: boolean; onChange: (on: boolean) => void | Promise<void> }) {
	const { t } = useTranslation();
	const toggle = t("settings.collections.toggle");
	return (
		<SettingRow
			title={t("settings.collections.title")}
			htmlFor="check-collections-on-launch"
			hint={toggle}
			help={t("settings.collections.hint")}
		>
			<Switch id="check-collections-on-launch" aria-label={toggle} checked={on} onCheckedChange={(checked) => void onChange(checked)} />
		</SettingRow>
	);
}

/// Whether the official game catalog is read when the Games page opens. The manual check in the page header works
/// either way.
export function CheckCatalogSetting({ on, onChange }: { on: boolean; onChange: (on: boolean) => void | Promise<void> }) {
	const { t } = useTranslation();
	const toggle = t("settings.catalog.toggle");
	return (
		<SettingRow title={t("settings.catalog.title")} htmlFor="check-catalog-on-launch" hint={toggle} help={t("settings.catalog.hint")}>
			<Switch id="check-catalog-on-launch" aria-label={toggle} checked={on} onCheckedChange={(checked) => void onChange(checked)} />
		</SettingRow>
	);
}

/// Whether a game launch takes the window to the tray. Off by default: starting a game never hides Gobbo by surprise.
export function HideWhenGameLaunchesSetting({ on, onChange }: { on: boolean; onChange: (on: boolean) => void | Promise<void> }) {
	const { t } = useTranslation();
	const toggle = t("settings.hideWhenGameLaunches.toggle");
	return (
		<SettingRow
			title={t("settings.hideWhenGameLaunches.title")}
			htmlFor="hide-when-game-launches"
			hint={toggle}
			help={t("settings.hideWhenGameLaunches.hint")}
		>
			<Switch id="hide-when-game-launches" aria-label={toggle} checked={on} onCheckedChange={(checked) => void onChange(checked)} />
		</SettingRow>
	);
}

/// The main switch, then one switch per task kind and outcome, and one per app event. The codes come from the generated
/// bindings, so a new task kind adds a row here; `t` only accepts keys that exist in both locales, so a missing label
/// fails the type check.
export function SystemNotificationsSetting({
	on,
	muted,
	onChange,
	onNotification,
}: {
	on: boolean;
	muted: string[];
	onChange: (on: boolean) => void | Promise<void>;
	onNotification: (code: string, on: boolean) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const toggle = t("settings.notifications.toggle");
	return (
		<>
			<SettingRow
				title={t("settings.notifications.title")}
				htmlFor="system-notifications"
				hint={toggle}
				help={t("settings.notifications.hint")}
			>
				<Switch id="system-notifications" aria-label={toggle} checked={on} onCheckedChange={(checked) => void onChange(checked)} />
			</SettingRow>
			{on && (
				<SettingRow title={t("settings.notifications.eventsTitle")} hint={t("settings.notifications.eventsHint")}>
					<Table>
						<TableHeader>
							<TableRow className="hover:bg-transparent">
								<TableHead className="text-muted-foreground font-normal">{t("settings.notifications.task")}</TableHead>
								{NOTIFICATION_OUTCOMES.map((outcome) => (
									<TableHead key={outcome} className="text-muted-foreground w-24 text-center font-normal">
										{t(`settings.notifications.outcomes.${outcome}`)}
									</TableHead>
								))}
							</TableRow>
						</TableHeader>
						<TableBody>
							{TASK_KINDS.map((kind) => {
								const task = t(`settings.notifications.kinds.${kind}`);
								return (
									<TableRow key={kind}>
										<TableCell className="whitespace-normal">{task}</TableCell>
										{NOTIFICATION_OUTCOMES.map((outcome) => {
											const code = taskNotification(kind, outcome);
											return (
												<TableCell key={outcome} className="text-center">
													<Switch
														id={`notification-${code}`}
														aria-label={t("settings.notifications.switchLabel", {
															task,
															outcome: t(`settings.notifications.outcomes.${outcome}`),
														})}
														checked={!muted.includes(code)}
														onCheckedChange={(checked) => void onNotification(code, checked)}
													/>
												</TableCell>
											);
										})}
									</TableRow>
								);
							})}
							{/* An app event has one switch; it sits in the last column so every switch lines up. */}
							{APP_NOTIFICATIONS.map((code) => (
								<TableRow key={code}>
									<TableCell colSpan={NOTIFICATION_OUTCOMES.length} className="whitespace-normal">
										<label htmlFor={`notification-${code}`}>{t(`settings.notifications.app.${code}`)}</label>
									</TableCell>
									<TableCell className="text-center">
										<Switch
											id={`notification-${code}`}
											checked={!muted.includes(code)}
											onCheckedChange={(checked) => void onNotification(code, checked)}
										/>
									</TableCell>
								</TableRow>
							))}
						</TableBody>
					</Table>
				</SettingRow>
			)}
		</>
	);
}
