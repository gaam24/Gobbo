import type { CloseAction } from "@/bindings";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { SettingRow } from "@/views/settings/controls";

/// Only the four known values reach the app; anything else falls back to asking.
function actionOf(value: string): CloseAction {
	return value === "exit" || value === "minimize" || value === "hide_to_tray" ? value : "ask";
}

/// What closing the main window does. The closing dialog remembers a concrete choice here, so picking "Ask every time"
/// also brings the dialog back. Without a tray icon the tray choice is not offered, unless it is already saved.
export function CloseActionSetting({
	action,
	tray,
	onAction,
}: {
	action: CloseAction;
	tray: boolean;
	onAction: (action: CloseAction) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<SettingRow title={t("settings.closeAction.label")} htmlFor="app-close-action" help={t("settings.closeAction.hint")}>
			<Select value={action} onValueChange={(value) => void onAction(actionOf(value))}>
				<SelectTrigger id="app-close-action" className="w-full md:w-64">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="ask">{t("settings.closeAction.ask")}</SelectItem>
					{(tray || action === "hide_to_tray") && <SelectItem value="hide_to_tray">{t("settings.closeAction.hideToTray")}</SelectItem>}
					<SelectItem value="minimize">{t("settings.closeAction.minimize")}</SelectItem>
					<SelectItem value="exit">{t("settings.closeAction.exit")}</SelectItem>
				</SelectContent>
			</Select>
		</SettingRow>
	);
}
