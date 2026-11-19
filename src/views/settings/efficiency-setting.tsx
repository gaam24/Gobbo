import type { AppSettings_Serialize } from "@/bindings";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import { SettingRow } from "@/views/settings/controls";

type EfficiencyMode = AppSettings_Serialize["efficiency_mode"];

/// Only the three known values reach the app; anything else falls back to automatic.
function modeOf(value: string): EfficiencyMode {
	return value === "off" || value === "always" ? value : "auto";
}

/// Windows 11 Efficiency mode (EcoQoS). It is hidden on other systems, which have no equivalent.
export function EfficiencySetting({ mode, onMode }: { mode: EfficiencyMode; onMode: (mode: EfficiencyMode) => void | Promise<void> }) {
	const { t } = useTranslation();
	return (
		<SettingRow title={t("settings.efficiency.label")} htmlFor="app-efficiency" help={t("settings.efficiency.hint")}>
			<Select value={mode} onValueChange={(value) => void onMode(modeOf(value))}>
				<SelectTrigger id="app-efficiency" className="w-full md:w-64">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="off">{t("settings.efficiency.off")}</SelectItem>
					<SelectItem value="auto">{t("settings.efficiency.auto")}</SelectItem>
					<SelectItem value="always">{t("settings.efficiency.always")}</SelectItem>
				</SelectContent>
			</Select>
		</SettingRow>
	);
}
