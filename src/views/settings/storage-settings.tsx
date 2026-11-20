import type { StorageInfo } from "@/bindings";
import { useTranslation } from "@/i18n";
import { SettingRow } from "@/views/settings/controls";

/// The mode the running copy uses and the file that records the choice. Changing the folder is a first-start
/// decision; removing the file asks the question again.
const MODE_KEYS = {
	portable: "settings.storage.portable",
	global: "settings.storage.global",
	custom: "settings.storage.custom",
} as const;

export function StorageSettings({ storage }: { storage: StorageInfo | null }) {
	const { t } = useTranslation();

	if (!storage) return null;

	return (
		<SettingRow title={t("settings.storage.title")} hint={t("settings.storage.hint")}>
			<div className="flex min-w-0 flex-col gap-1">
				<span className="text-sm font-medium">{t(MODE_KEYS[storage.mode])}</span>
				{storage.mode === "custom" ? (
					<span className="text-muted-foreground text-xs text-pretty">{t("settings.storage.customHint")}</span>
				) : (
					storage.decision_file && (
						<>
							<span className="text-muted-foreground text-xs">{t("settings.storage.decision")}</span>
							<span className="text-muted-foreground font-mono text-xs break-all" title={storage.decision_file}>
								{storage.decision_file}
							</span>
							<span className="text-muted-foreground text-xs text-pretty">{t("settings.storage.decisionHint")}</span>
						</>
					)
				)}
			</div>
		</SettingRow>
	);
}
