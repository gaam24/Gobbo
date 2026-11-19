import { useState } from "react";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";

/// A button that asks for confirmation in a dialog before it runs the action.
export function ConfirmButton({
	label,
	title,
	description,
	confirmLabel,
	disabled,
	destructive = true,
	onConfirm,
}: {
	label: string;
	title: string;
	description?: string;
	confirmLabel?: string;
	disabled?: boolean;
	/// The confirm button is red by default, for deleting. An install is not destructive.
	destructive?: boolean;
	onConfirm: () => void | Promise<void>;
}) {
	const [open, setOpen] = useState(false);
	return (
		<>
			<Button type="button" variant="outline" size="sm" disabled={disabled} onClick={() => setOpen(true)}>
				{label}
			</Button>
			<ConfirmDialog
				open={open}
				onOpenChange={setOpen}
				title={title}
				description={description}
				confirmLabel={confirmLabel}
				destructive={destructive}
				onConfirm={onConfirm}
			/>
		</>
	);
}
