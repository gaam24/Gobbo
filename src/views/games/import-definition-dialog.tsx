import { FileUp } from "lucide-react";
import { useEffect, useState } from "react";
import type { DefinitionPackagePreview_Serialize } from "@/bindings";
import { commands } from "@/bindings";
import { ModalDialog } from "@/components/modal-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { registerTask, removeTask } from "@/lib/tasks-store";

/// The preview of a shared definition package. The import writes nothing until the user confirms, and an existing user
/// definition or loader turns the action into an explicit replace.
export function DefinitionImportDialog({
	path,
	onClose,
	onImported,
}: {
	path: string;
	onClose: () => void;
	/// Re-reads the catalog after the write. The provider owns it so this lazy screen does not pull the games context
	/// into the first paint.
	onImported: (name: string) => Promise<void>;
}) {
	const { t } = useTranslation();
	const [preview, setPreview] = useState<DefinitionPackagePreview_Serialize | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [importError, setImportError] = useState<string | null>(null);
	const [importing, setImporting] = useState(false);

	useEffect(() => {
		const live = { current: true };
		void (async () => {
			try {
				const result = await unwrap(commands.inspectDefinitionPackage(path));

				if (live.current) setPreview(result);
			} catch (failure) {
				if (live.current) setError(messageOf(failure));
			}
		})();
		return () => {
			live.current = false;
		};
	}, [path]);

	const importPackage = async () => {
		if (!preview || preview.problems.length > 0) return;
		const id = `definition-import:${path}`;
		registerTask({
			id,
			game: null,
			kind: "install",
			title: t("editor.import.import"),
			detail: preview.name || path,
			state: "running",
			source: "local",
		});
		setImporting(true);
		setImportError(null);

		try {
			const result = await unwrap(commands.importDefinitionPackage(path, preview.overwrites.length > 0));
			await onImported(result.name);
			onClose();
		} catch (failure) {
			setImportError(messageOf(failure));
		}
		// Both paths remove the task; the command keeps running if the dialog was closed meanwhile.
		removeTask(id);
		setImporting(false);
	};

	const replace = (preview?.overwrites.length ?? 0) > 0;
	return (
		<ModalDialog title={t("editor.import.title")} description={t("editor.import.hint")} width="3xl" onClose={onClose}>
			{error && (
				<Alert variant="destructive">
					<AlertTitle>{t("editor.import.failed")}</AlertTitle>
					<AlertDescription>{error}</AlertDescription>
				</Alert>
			)}
			{preview === null && error === null && (
				<div className="flex items-center justify-center py-10">
					<Spinner aria-label={t("editor.import.loading")} />
				</div>
			)}
			{preview && (
				<>
					<div className="flex items-start gap-4">
						{preview.icon ? (
							<img src={preview.icon} alt="" className="size-16 shrink-0 rounded-md object-cover" />
						) : (
							<div className="bg-muted text-muted-foreground flex size-16 shrink-0 items-center justify-center rounded-md text-xl font-semibold">
								{preview.name.trim().charAt(0).toUpperCase() || "?"}
							</div>
						)}
						<div className="grid min-w-0 gap-1">
							<span className="truncate font-medium">{preview.name || t("editor.import.unnamed")}</span>
							<span className="text-muted-foreground font-mono text-xs">{preview.id || "—"}</span>
							<span className="flex flex-wrap gap-1.5">
								<Badge variant="outline">{t("games.userDefinition")}</Badge>
								{preview.loader && (
									<Badge variant="secondary">
										{preview.loader.name}
										{preview.loader.variant ? ` · ${preview.loader.variant}` : ""}
									</Badge>
								)}
							</span>
						</div>
					</div>

					<div className="grid gap-1 text-sm">
						<span className="text-muted-foreground text-xs">{t("editor.import.files")}</span>
						<div className="flex flex-wrap gap-1.5">
							{preview.files.map((file) => (
								<code key={file} className="bg-muted rounded px-1.5 py-0.5 font-mono text-xs">
									{file}
								</code>
							))}
						</div>
					</div>

					{preview.problems.length > 0 && (
						<Alert variant="destructive">
							<AlertTitle>{t("editor.import.problems")}</AlertTitle>
							<AlertDescription>
								<ul className="grid gap-1">
									{preview.problems.map((problem) => (
										<li key={`${problem.section}-${problem.message}`}>{problem.message}</li>
									))}
								</ul>
							</AlertDescription>
						</Alert>
					)}

					{replace && (
						<Alert variant="destructive">
							<AlertTitle>{t("editor.import.overwritesTitle")}</AlertTitle>
							<AlertDescription>
								{t("editor.import.overwrites", {
									items: preview.overwrites.join(", "),
								})}
							</AlertDescription>
						</Alert>
					)}

					{preview.warnings.length > 0 && (
						<Alert>
							<AlertTitle>{t("editor.import.warnings")}</AlertTitle>
							<AlertDescription>
								<ul className="grid gap-1">
									{preview.warnings.map((warning) => (
										<li key={warning}>{warning}</li>
									))}
								</ul>
							</AlertDescription>
						</Alert>
					)}

					{importError && (
						<Alert variant="destructive">
							<AlertDescription>{importError}</AlertDescription>
						</Alert>
					)}
				</>
			)}

			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={!preview || preview.problems.length > 0 || importing} onClick={() => void importPackage()}>
					{importing ? <Spinner aria-hidden /> : <FileUp />}
					{replace ? t("editor.import.replace") : t("editor.import.import")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}
