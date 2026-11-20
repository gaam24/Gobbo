import { Check } from "lucide-react";
import { useEffect, useState } from "react";
import { ModalDialog } from "@/components/modal-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldLabel } from "@/components/ui/field";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { messageOf } from "@/lib/ipc";
import { pickFolder } from "@/lib/tauri";

/// How long the folder check waits after the last change to the folder, so it does not run on every keystroke.
const CHECK_DELAY_MS = 300;

/// What the folder check says about the folder in the dialog.
type FolderCheck =
	| { state: "idle" }
	| { state: "checking" }
	| { state: "found"; marker: string | null }
	| { state: "error"; message: string };

/// The result of the folder check as text, with an icon for a passed check. Errors are text too, announced as an alert.
function CheckStatus({ check }: { check: FolderCheck }) {
	const { t } = useTranslation();

	if (check.state === "idle") return null;

	if (check.state === "checking") {
		return (
			<p className="text-muted-foreground flex items-center gap-2 text-sm">
				<Spinner aria-hidden />
				{t("games.locate.checking")}
			</p>
		);
	}

	if (check.state === "error") {
		return (
			<Alert variant="destructive">
				<AlertDescription>{check.message}</AlertDescription>
			</Alert>
		);
	}

	return (
		<p role="status" className="text-primary flex items-center gap-2 text-sm font-medium">
			<Check aria-hidden className="size-4" />
			{check.marker ? t("games.locate.found", { marker: check.marker }) : t("games.locate.foundNoMarker")}
		</p>
	);
}

/// The check to show for a folder: idle when it is empty, the stored result when it belongs to this folder, and running otherwise.
function currentCheck(folder: string, checked: { folder: string; result: FolderCheck } | null): FolderCheck {
	if (!folder) return { state: "idle" };

	if (checked?.folder === folder) return checked.result;
	return { state: "checking" };
}

/// Asks for the folder of a game that is not in the Steam libraries. The folder is checked as the user types, and the game
/// can be added only when the check passes.
export function LocateDialog({
	id,
	name,
	onAdd,
	onCheck,
	onClose,
}: {
	id: string;
	name: string;
	onAdd: (id: string, gameRoot: string) => Promise<boolean>;
	onCheck: (id: string, gameRoot: string) => Promise<string | null>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [folder, setFolder] = useState("");
	const [checked, setChecked] = useState<{
		folder: string;
		result: FolderCheck;
	} | null>(null);
	const [busy, setBusy] = useState(false);
	const trimmed = folder.trim();

	useEffect(() => {
		if (!trimmed) return;
		let current = true;
		const timer = window.setTimeout(() => {
			void (async () => {
				let result: FolderCheck;

				try {
					result = { state: "found", marker: await onCheck(id, trimmed) };
				} catch (failure) {
					result = { state: "error", message: messageOf(failure) };
				}

				if (current) setChecked({ folder: trimmed, result });
			})();
		}, CHECK_DELAY_MS);
		return () => {
			current = false;
			window.clearTimeout(timer);
		};
	}, [id, trimmed, onCheck]);

	// Empty folder: idle. A result for another folder is old, so the check shows as running until its own result comes.
	const check = currentCheck(trimmed, checked);

	const browse = async () => {
		const picked = await pickFolder();

		if (picked) setFolder(picked);
	};

	const submit = async () => {
		if (busy || check.state !== "found") return;
		setBusy(true);
		const added = await onAdd(id, trimmed);
		setBusy(false);

		if (added) onClose();
	};

	return (
		<ModalDialog title={t("games.locate.title", { name })} description={t("games.locate.hint")} width="lg" onClose={onClose}>
			<Field>
				<FieldLabel htmlFor="locate-folder">{t("games.locate.folderLabel")}</FieldLabel>
				<InputGroup>
					<InputGroupInput
						id="locate-folder"
						className="font-mono text-sm"
						value={folder}
						placeholder={t("games.locate.placeholder")}
						spellCheck={false}
						onChange={(event) => setFolder(event.currentTarget.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") void submit();
						}}
					/>
					<InputGroupAddon align="inline-end">
						<InputGroupButton onClick={() => void browse()}>{t("games.locate.browse")}</InputGroupButton>
					</InputGroupAddon>
				</InputGroup>
			</Field>
			<CheckStatus check={check} />
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={check.state !== "found" || busy} onClick={() => void submit()}>
					{t("games.locate.submit")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}
