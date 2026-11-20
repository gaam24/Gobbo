import { PathText } from "@/components/app/path-text";
import { Item, ItemActions, ItemContent } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { SettingRow } from "@/views/settings/controls";
import { PathActions, type PathProps } from "@/views/settings/path-actions";

/// The folder where Gobbo keeps the games, the mod library, the archives and the profiles. Removing a game leaves them in place.
export function DataFolderSettings({ path, onOpen, onCopy }: PathProps) {
	const { t } = useTranslation();
	return (
		<SettingRow title={t("settings.data.title")} help={t("settings.data.hint")}>
			<Item variant="outline" size="sm" className="min-w-0">
				<ItemContent className="min-w-0">
					<PathText path={path} className="text-muted-foreground" />
				</ItemContent>
				<ItemActions>
					<PathActions path={path} onOpen={onOpen} onCopy={onCopy} />
				</ItemActions>
			</Item>
		</SettingRow>
	);
}
