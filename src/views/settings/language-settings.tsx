import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import type { LanguageSetting } from "@/locales/locale";
import { SettingRow } from "@/views/settings/controls";

/// Only the three known values reach the app; anything else falls back to the system language.
function languageOf(value: string): LanguageSetting {
	return value === "en" || value === "pl" ? value : "system";
}

/// The interface language. It belongs to the whole app, so it is shown without a game too, and saved at once.
export function LanguageSettings({
	language,
	onLanguage,
}: {
	language: LanguageSetting;
	onLanguage: (language: LanguageSetting) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<SettingRow title={t("settings.language.label")} htmlFor="app-language" help={t("settings.language.hint")}>
			<Select value={language} onValueChange={(value) => void onLanguage(languageOf(value))}>
				<SelectTrigger id="app-language" className="w-full md:w-64">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="system">{t("settings.language.system")}</SelectItem>
					<SelectItem value="en">{t("settings.language.english")}</SelectItem>
					<SelectItem value="pl">{t("settings.language.polish")}</SelectItem>
				</SelectContent>
			</Select>
		</SettingRow>
	);
}
