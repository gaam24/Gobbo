import { ConfirmDialog } from "@/components/confirm-dialog";
import { useTranslation } from "@/i18n";

/// Asks before a game leaves Gobbo. The game's files and mods stay on disk, in the removed folder of the Storage page.
export function ConfirmRemoveGame({
	name,
	open,
	onOpenChange,
	onConfirm,
}: {
	/// The game's name, for the title.
	name: string;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onConfirm: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<ConfirmDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("games.confirmRemove.title", { name })}
			description={t("games.confirmRemove.body")}
			confirmLabel={t("games.action.remove")}
			onConfirm={onConfirm}
		/>
	);
}
