import { Children, isValidElement, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useTranslation } from "@/i18n";

/// The widths a dialog can take. Each is a whole class name, so Tailwind finds it in this file.
const WIDTHS = { sm: "sm:max-w-sm", lg: "sm:max-w-lg", xl: "sm:max-w-xl", "3xl": "sm:max-w-3xl" } as const;

/// A dialog that stays open until it is closed. Its body and its footer are the children. The body spaces its blocks
/// 16 px apart, so a dialog does not add its own `grid gap-4` wrapper; lists inside it use `BoxList`.
export function ModalDialog({
	title,
	description,
	width = "xl",
	className,
	open = true,
	onClose,
	children,
}: {
	title: ReactNode;
	/// Omitted for a dialog whose title says it all, such as a rename.
	description?: ReactNode;
	width?: keyof typeof WIDTHS;
	/// Extra classes for the content, such as a scroll limit or a larger body.
	className?: string;
	/// Defaults to open: most callers mount the dialog only while it is needed.
	open?: boolean;
	onClose: () => void;
	children: ReactNode;
}) {
	const { t } = useTranslation();
	// The footer stays outside the scrolling body, so a dialog that does not fit the window keeps its actions visible.
	const parts = Children.toArray(children);
	const last = parts.at(-1);
	const footer = isValidElement(last) && (last.type === DialogFooter || last.type === DialogCloseFooter) ? last : null;
	const body = footer ? parts.slice(0, -1) : parts;

	return (
		<Dialog open={open} onOpenChange={(next) => !next && onClose()}>
			<DialogContent
				closeLabel={t("common.close")}
				className={`flex max-h-[85vh] flex-col overflow-hidden ${WIDTHS[width]} ${className ?? ""}`}
			>
				<DialogHeader className="shrink-0">
					<DialogTitle>{title}</DialogTitle>
					{description && <DialogDescription>{description}</DialogDescription>}
				</DialogHeader>
				<div className="grid min-h-0 min-w-0 flex-1 content-start gap-4 overflow-auto text-sm">{body}</div>
				{footer}
			</DialogContent>
		</Dialog>
	);
}

/// The footer most dialogs end with: one button that closes the dialog.
export function DialogCloseFooter({ onClose }: { onClose: () => void }) {
	const { t } = useTranslation();
	return (
		<DialogFooter>
			<Button variant="outline" onClick={onClose}>
				{t("common.close")}
			</Button>
		</DialogFooter>
	);
}

/// Closes the dialog first and then runs the action, so the dialog is gone while the action works.
export function closeThen(onClose: () => void) {
	return (action: () => void | Promise<void>) => {
		onClose();
		void action();
	};
}
