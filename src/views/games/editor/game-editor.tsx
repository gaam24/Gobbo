import { ArrowLeft, Download, FileCode, FolderOpen, Rocket, Save, SlidersHorizontal, Terminal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { PageBody, PageHeader, SettingsLayout } from "@/components/app/page-layout";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { hasCtrl, hasOpenDialog, MOUSE_BACK } from "@/lib/keyboard";
import { shortcutKey } from "@/lib/shortcuts";
import { DefinitionSource, MoreOptions } from "@/views/games/editor/advanced-form";
import { AdvancedLinux } from "@/views/games/editor/advanced-linux";
import { ArchiveCheck } from "@/views/games/editor/archive-check";
import { QuickSetup } from "@/views/games/editor/quick-setup";
import { useGameEditor } from "@/views/games/editor/use-game-editor";
import { Section } from "@/views/settings/controls";

type EditorPart = "basics" | "more" | "linux" | "source";

const SAVE_KEY = shortcutKey("save");
const BACK_KEY = shortcutKey("back");

/// The definition editor, in four parts. Basics is all most games need: the folder, how mods get in, the program and
/// the check. More options holds the rules a few games need, each folded under a sentence that says when; Linux and
/// Proton holds every value only Linux reads, so a Windows user never has to read it; the definition file is the TOML.
/// A save writes the real `game.toml` under the user's definitions and the caller adds the game.
export function GameEditor({ id, onClose }: { id: string | null; onClose: () => void }) {
	const { t } = useTranslation();
	const editor = useGameEditor(id, onClose);
	const [part, setPart] = useState<EditorPart>("basics");
	const leave = () => editor.requestLeave(editor.onClose);
	const latest = useRef({ leave, save: editor.save, blocked: editor.blocked });
	useEffect(() => {
		latest.current = { leave, save: editor.save, blocked: editor.blocked };
	});

	// The editor owns back and save while it is open: Ctrl+S saves, and Alt+← or the mouse's back button leaves through
	// the same unsaved-changes question as the back arrow. The global shortcuts step aside while the editor is open.
	useEffect(() => {
		const onKey = (event: KeyboardEvent) => {
			if (hasOpenDialog()) return;

			if (hasCtrl(event) && event.key.toLowerCase() === SAVE_KEY) {
				event.preventDefault();

				if (!latest.current.blocked) void latest.current.save();
			} else if (event.altKey && event.key === BACK_KEY) {
				event.preventDefault();
				latest.current.leave();
			}
		};
		const onMouseUp = (event: MouseEvent) => {
			if (event.button !== MOUSE_BACK || hasOpenDialog()) return;
			event.preventDefault();
			latest.current.leave();
		};
		window.addEventListener("keydown", onKey);
		window.addEventListener("mouseup", onMouseUp);
		return () => {
			window.removeEventListener("keydown", onKey);
			window.removeEventListener("mouseup", onMouseUp);
		};
	}, []);

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

	const title = editor.editing ? t("editor.editTitle", { name: titleName }) : t("editor.newTitle");

	// The same frame as Gobbo's and a game's settings: the header with its way back, the parts on the left and the
	// chosen part in the same column. Only the footer is the editor's own, because a draft is saved on purpose.
	return (
		<div className="flex min-h-0 flex-1 flex-col">
			<PageHeader
				title={title}
				hint={t("editor.hint")}
				onBack={leave}
				actions={
					<>
						{/* The folder path is in the tooltip: a full AppData path was a line of noise under the title. */}
						{editor.definitionsFolder && editor.hasUserDefinitions && (
							<Button variant="ghost" size="sm" title={editor.definitionsFolder} onClick={editor.openDefinitionsFolder}>
								<FolderOpen /> {t("editor.openUserFolder")}
							</Button>
						)}
						{editor.editing && editor.origin === "user" && (
							<Button variant="outline" size="sm" onClick={() => void editor.exportDefinition()}>
								<Download /> {t("settings.game.exportDefinition")}
							</Button>
						)}
					</>
				}
			/>
			<PageBody padded>
				<div className="grid gap-6">
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

					<SettingsLayout
						label={t("editor.parts.label")}
						value={part}
						onValueChange={setPart}
						sections={[
							{
								id: "basics",
								label: t("editor.parts.basics"),
								icon: Rocket,
								content: (
									<Section id="editor-part-basics" title={t("editor.parts.basics")} hint={t("editor.partHints.basics")}>
										<div className="mt-6 grid gap-8">
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
										</div>
									</Section>
								),
							},
							{
								id: "more",
								label: t("editor.parts.more"),
								icon: SlidersHorizontal,
								content: (
									<Section id="editor-part-more" title={t("editor.parts.more")}>
										<MoreOptions
											draft={editor.draft}
											onChange={editor.update}
											directories={editor.inspection?.directories ?? []}
											programs={editor.inspection?.executables.map((executable) => executable.path) ?? []}
											folder={editor.folder}
										/>
									</Section>
								),
							},
							{
								id: "linux",
								label: t("editor.parts.linux"),
								icon: Terminal,
								content: (
									<Section id="editor-part-linux" title={t("editor.parts.linux")} hint={t("editor.linux.hint")}>
										<div className="mt-6">
											<AdvancedLinux draft={editor.draft} onChange={editor.update} />
										</div>
									</Section>
								),
							},
							{
								id: "source",
								label: t("editor.parts.source"),
								icon: FileCode,
								content: (
									<Section id="editor-part-source" title={t("editor.parts.source")}>
										<DefinitionSource
											source={editor.source}
											sourceDirty={editor.sourceDirty}
											formText={editor.formText}
											sourceError={editor.sourceError}
											onSourceChange={editor.changeSource}
											onApplySource={editor.applySource}
										/>
									</Section>
								),
							},
						]}
					/>
				</div>
			</PageBody>

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
