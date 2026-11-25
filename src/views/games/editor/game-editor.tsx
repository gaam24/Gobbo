import { ArrowLeft, Download, FolderOpen, Save } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { AdvancedForm } from "@/views/games/editor/advanced-form";
import { ArchiveCheck } from "@/views/games/editor/archive-check";
import { QuickSetup } from "@/views/games/editor/quick-setup";
import { useGameEditor } from "@/views/games/editor/use-game-editor";

/// The definition editor. The quick part is a short wizard; the advanced part below it holds every remaining field and
/// the TOML source. A save writes the real `game.toml` under the user's definitions and the caller adds the game.
export function GameEditor({ id, onClose }: { id: string | null; onClose: () => void }) {
	const { t } = useTranslation();
	const editor = useGameEditor(id, onClose);

	if (!editor.loaded) {
		return (
			<div className="flex min-h-0 flex-1 items-center justify-center">
				<Spinner aria-label={t("editor.loading")} />
			</div>
		);
	}

	if (editor.loadError) {
		return (
			<div className="mx-auto grid w-full max-w-3xl gap-4 px-6 py-8">
				<Alert variant="destructive">
					<AlertTitle>{t("editor.loadFailed")}</AlertTitle>
					<AlertDescription>{editor.loadError}</AlertDescription>
				</Alert>
				<Button variant="outline" className="w-fit" onClick={editor.onClose}>
					<ArrowLeft /> {t("common.back")}
				</Button>
			</div>
		);
	}

	let footerStatus: string | null = null;

	if (editor.dirty) footerStatus = t("editor.unsaved");
	else if (editor.check?.ok) footerStatus = t("editor.ready");
	const titleName = editor.draft.name.trim() !== "" ? editor.draft.name : (id ?? "");

	return (
		<div className="flex min-h-0 flex-1 flex-col">
			<div className="min-h-0 flex-1 overflow-y-auto">
				<div className="mx-auto grid w-full max-w-3xl gap-8 px-6 pt-6 pb-10">
					<header className="grid gap-2">
						<div className="flex flex-wrap items-center justify-between gap-3">
							<Button variant="ghost" size="sm" className="-ml-2 w-fit" onClick={() => editor.requestLeave(editor.onClose)}>
								<ArrowLeft /> {t("common.back")}
							</Button>
							{editor.editing && editor.origin === "user" && (
								<Button variant="outline" size="sm" onClick={() => void editor.exportDefinition()}>
									<Download /> {t("settings.game.exportDefinition")}
								</Button>
							)}
						</div>
						<h1 className="text-xl font-semibold">{editor.editing ? t("editor.editTitle", { name: titleName }) : t("editor.newTitle")}</h1>
						<p className="text-muted-foreground text-sm text-pretty">{t("editor.hint")}</p>
						{editor.definitionsFolder && (
							<div className="text-muted-foreground flex flex-wrap items-center gap-2 text-xs">
								<span>{t("editor.userFolder", { path: editor.definitionsFolder })}</span>
								{editor.hasUserDefinitions && (
									<Button variant="ghost" size="sm" onClick={editor.openDefinitionsFolder}>
										<FolderOpen /> {t("editor.openUserFolder")}
									</Button>
								)}
							</div>
						)}
					</header>

					{editor.presetError && (
						<Alert variant="destructive">
							<AlertTitle>{t("editor.presetsFailed")}</AlertTitle>
							<AlertDescription>{editor.presetError}</AlertDescription>
						</Alert>
					)}
					{editor.checkError && (
						<Alert variant="destructive">
							<AlertDescription>{editor.checkError}</AlertDescription>
						</Alert>
					)}

					<QuickSetup
						draft={editor.draft}
						onChange={editor.update}
						folder={editor.folder}
						onFolderChange={editor.setFolder}
						onBrowse={() => void editor.browse()}
						inspection={editor.inspection}
						inspecting={editor.inspecting}
						inspectError={editor.inspectError}
						presets={editor.presets}
						layout={editor.layout}
						onLayoutChange={editor.chooseLayout}
						check={editor.check}
						issues={editor.issues}
						editing={editor.editing}
						idWarning={editor.idWarning}
					/>

					<ArchiveCheck text={editor.activeText} />

					<AdvancedForm
						draft={editor.draft}
						onChange={editor.update}
						directories={editor.inspection?.directories ?? []}
						programs={editor.inspection?.executables.map((executable) => executable.path) ?? []}
						folder={editor.folder}
						source={editor.source}
						sourceDirty={editor.sourceDirty}
						formText={editor.formText}
						sourceError={editor.sourceError}
						onSourceChange={editor.changeSource}
						onApplySource={editor.applySource}
					/>
				</div>
			</div>

			<footer className="bg-background flex items-center justify-between gap-3 border-t px-6 py-3">
				<span className="text-muted-foreground text-sm" role="status">
					{footerStatus}
				</span>
				<div className="flex items-center gap-2">
					<Button variant="ghost" onClick={() => editor.requestLeave(editor.onClose)}>
						{t("common.cancel")}
					</Button>
					<Button disabled={editor.blocked} onClick={() => void editor.save()}>
						{editor.saving ? <Spinner aria-hidden /> : <Save />}
						{editor.editing || editor.folder.trim() === "" ? t("editor.save") : t("editor.saveAndAdd")}
					</Button>
				</div>
			</footer>
		</div>
	);
}
