import type { ReactNode } from "react";
import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { useTranslation } from "@/i18n";

/// The dialog that asks before an action runs. The caller keeps `open`, so a button or a menu item can open it.
export function ConfirmDialog({
	open,
	onOpenChange,
	title,
	description,
	children,
	confirmLabel,
	confirmDisabled = false,
	destructive = true,
	onConfirm,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	description?: string;
	/// Extra content in place of `description`, such as the profiles a delete affects. It sits in the description, so it
	/// is read as part of the question.
	children?: ReactNode;
	confirmLabel?: string;
	/// Keeps the confirm button off while the action cannot run yet, for example before its data has loaded.
	confirmDisabled?: boolean;
	/// The confirm button is red by default, for deleting. An install is not destructive.
	destructive?: boolean;
	onConfirm: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent size="sm">
				<AlertDialogHeader>
					<AlertDialogTitle>{title}</AlertDialogTitle>
					{children ? (
						<AlertDialogDescription asChild>
							<div className="grid gap-3">{children}</div>
						</AlertDialogDescription>
					) : (
						description && <AlertDialogDescription>{description}</AlertDialogDescription>
					)}
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
					<AlertDialogAction variant={destructive ? "destructive" : "default"} disabled={confirmDisabled} onClick={() => void onConfirm()}>
						{confirmLabel ?? t("common.delete")}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
}
