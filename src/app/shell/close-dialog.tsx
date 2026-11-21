import { useState } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useSettingsActions } from "@/app/contexts/settings-context";
import { CloseDialogChoice, type CloseRequest } from "@/app/shell/close-dialog-view";
import { type CloseAction, commands } from "@/bindings";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { useTauriListener } from "@/lib/use-tauri-listener";

const CLOSE_REQUESTED_EVENT = "close-requested";

/// The closing dialog. Rust blocks the window close and sends the event. It is mounted above the shell's error boundary,
/// so a window whose screens failed can still be closed. The backend performs the choice; exiting stops a running game
/// with the same confirmation the tray uses.
export function CloseDialog() {
	const { t } = useTranslation();
	const { attempt } = useNoticeActions();
	const { saveCloseAction } = useSettingsActions();
	const [open, setOpen] = useState(false);
	// Kept after closing, so the content does not change while the dialog fades out.
	const [request, setRequest] = useState<CloseRequest>({ tray: true, game_running: false });
	const [remember, setRemember] = useState(false);

	useTauriListener(async (scope) => {
		scope.add(
			await scope.listen<CloseRequest>(CLOSE_REQUESTED_EVENT, (event) => {
				setRequest(event.payload);
				setRemember(false);
				setOpen(true);
			}),
		);
	});

	/// `ask` is Cancel: the backend only forgets the pending close. A remembered choice is saved first, through the
	/// settings like any other, so it is on disk before an exit and the settings screen shows it at once.
	const choose = (action: CloseAction) => {
		setOpen(false);
		void (async () => {
			if (remember && action !== "ask") await saveCloseAction(action);
			await attempt(async () => {
				await unwrap(commands.chooseCloseAction(action));
			});
		})();
	};

	return (
		<ModalDialog width="sm" open={open} title={t("closeDialog.title")} onClose={() => choose("ask")}>
			<CloseDialogChoice request={request} remember={remember} onRemember={setRemember} onChoose={choose} />
			<DialogFooter>
				<Button variant="outline" onClick={() => choose("ask")}>
					{t("common.cancel")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}
