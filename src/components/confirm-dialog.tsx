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
	size = "sm",
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
	/// `sm` is the narrow question with two equal buttons. A long confirm label needs `default`, which gives the text
	/// room and stacks the buttons when the window is narrow.
	size?: "default" | "sm";
	onConfirm: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent size={size}>
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
