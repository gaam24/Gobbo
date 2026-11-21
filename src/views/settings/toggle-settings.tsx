import type { SourceProviderInfo } from "@/bindings";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
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
