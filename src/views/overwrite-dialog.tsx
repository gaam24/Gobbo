import { FolderOpen } from "lucide-react";
import { useState } from "react";
import { commands } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { ConfirmButton } from "@/components/confirm-button";
import { closeThen, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";

/// What the game and its tools wrote while they ran in the overlay, kept out of the game folder: a merged script, a
/// generated patch, a config. They can become a mod of their own, enabled at the top of the list, or go.
export function OverwriteDialog({
	gameId,
	locale,
	onToMod,
	onClear,
	onOpenFolder,
	onClose,
}: {
	gameId: string;
	locale: string;
	onToMod: (name: string) => void | Promise<void>;
	onClear: () => void | Promise<void>;
	onOpenFolder: (path: string) => void | Promise<void>;
	onClose: () => void;
}) {
	const { t } = useTranslation();
	const [failure, setFailure] = useState<string | null>(null);
	const view = useLoad(gameId, () => unwrap(commands.overwriteFiles(gameId)), {
		onError: (error) => setFailure(messageOf(error)),
	});
	const [name, setName] = useState("");
	const total = view?.files.reduce((sum, file) => sum + file.size_kb, 0) ?? 0;
	const run = closeThen(onClose);

	return (
		<ModalDialog title={t("overwrite.title")} description={t("overwrite.hint")} onClose={onClose}>
			{failure && <FieldError>{failure}</FieldError>}
			{view && (
				<>
					<div className="flex items-center gap-2">
						<span className="text-muted-foreground flex-1">
							{t("overwrite.count", { count: view.files.length, size: formatSize(total, locale) })}
						</span>
						<Button variant="ghost" size="sm" onClick={() => void onOpenFolder(view.folder)}>
							<FolderOpen /> {t("overwrite.open")}
						</Button>
					</div>
					<BoxList scroll>
						{view.files.map((file) => (
							<BoxListRow key={file.path}>
								<ItemContent className="min-w-0 flex-row items-center gap-3">
									<ItemTitle className="min-w-0 flex-1 truncate font-mono text-xs font-normal">{file.path}</ItemTitle>
									<ItemDescription className="shrink-0 font-mono text-xs tabular-nums">{formatSize(file.size_kb, locale)}</ItemDescription>
								</ItemContent>
							</BoxListRow>
						))}
					</BoxList>
					<Field>
						<FieldLabel htmlFor="overwrite-name">{t("overwrite.name")}</FieldLabel>
						<Input
							id="overwrite-name"
							value={name}
							placeholder={t("overwrite.placeholder")}
							onChange={(event) => setName(event.currentTarget.value)}
						/>
					</Field>
				</>
			)}
			<DialogFooter>
				<span className="sm:mr-auto">
					<ConfirmButton
						label={t("overwrite.clear")}
						title={t("overwrite.clearTitle")}
						description={t("overwrite.clearBody")}
						confirmLabel={t("overwrite.clear")}
						disabled={!view?.files.length || failure !== null}
						onConfirm={() => run(onClear)}
					/>
				</span>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={!(view?.files.length && name.trim()) || failure !== null} onClick={() => run(() => onToMod(name.trim()))}>
					{t("overwrite.toMod")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}
