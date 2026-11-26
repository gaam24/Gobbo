import { ConfirmDialog } from "@/components/confirm-dialog";
import { useTranslation } from "@/i18n";

/// Asks before Gobbo restarts with administrator rights. The restart is the only way to load mods into a program that
/// Windows refuses to start at the normal integrity level, so the dialog says what will happen and what stops working
/// while Gobbo is elevated.
export function ElevationDialog({ onRestart, onClose }: { onRestart: () => void; onClose: () => void }) {
	const { t } = useTranslation();

	return (
		<ConfirmDialog
			open
			onOpenChange={(open) => !open && onClose()}
			title={t("elevation.title")}
			confirmLabel={t("elevation.restart")}
			destructive={false}
			size="default"
			onConfirm={onRestart}
		>
			<p>{t("elevation.why")}</p>
			<p>{t("elevation.warning")}</p>
			<p>{t("elevation.after")}</p>
		</ConfirmDialog>
	);
}
