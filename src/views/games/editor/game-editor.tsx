import { ArrowLeft, Download, FileCode, FolderOpen, Rocket, Save, SlidersHorizontal, Terminal } from "lucide-react";
import { useState } from "react";
import { SettingsLayout } from "@/components/app/page-layout";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { DefinitionSource, MoreOptions } from "@/views/games/editor/advanced-form";
import { AdvancedLinux } from "@/views/games/editor/advanced-linux";
import { ArchiveCheck } from "@/views/games/editor/archive-check";
import { QuickSetup } from "@/views/games/editor/quick-setup";
import { useGameEditor } from "@/views/games/editor/use-game-editor";

type EditorPart = "basics" | "more" | "linux" | "source";

/// The definition editor, in four parts. Basics is all most games need: the folder, how mods get in, the program and
/// the check. More options holds the rules a few games need, each folded under a sentence that says when; Linux and
/// Proton holds every value only Linux reads, so a Windows user never has to read it; the definition file is the TOML.
/// A save writes the real `game.toml` under the user's definitions and the caller adds the game.
export function GameEditor({ id, onClose }: { id: string | null; onClose: () => void }) {
	const { t } = useTranslation();
	const editor = useGameEditor(id, onClose);
	const [part, setPart] = useState<EditorPart>("basics");

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
				<div className="mx-auto grid w-full max-w-6xl gap-8 px-6 pt-6 pb-10">
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
						{/* The folder path is in the button's tooltip: a full AppData path was a line of noise under the title. */}
						{editor.definitionsFolder && editor.hasUserDefinitions && (
							<Button
								variant="ghost"
								size="sm"
								className="-ml-2 w-fit"
								title={editor.definitionsFolder}
								onClick={editor.openDefinitionsFolder}
							>
								<FolderOpen /> {t("editor.openUserFolder")}
							</Button>
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
									<div className="grid gap-8">
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
								),
							},
							{
								id: "more",
								label: t("editor.parts.more"),
								icon: SlidersHorizontal,
								content: (
									<MoreOptions
										draft={editor.draft}
										onChange={editor.update}
										directories={editor.inspection?.directories ?? []}
										programs={editor.inspection?.executables.map((executable) => executable.path) ?? []}
										folder={editor.folder}
									/>
								),
							},
							{
								id: "linux",
								label: t("editor.parts.linux"),
								icon: Terminal,
								content: (
									<div className="grid gap-6">
										<p className="text-muted-foreground text-sm text-pretty">{t("editor.linux.hint")}</p>
										<AdvancedLinux draft={editor.draft} onChange={editor.update} />
									</div>
								),
							},
							{
								id: "source",
								label: t("editor.parts.source"),
								icon: FileCode,
								content: (
									<DefinitionSource
										source={editor.source}
										sourceDirty={editor.sourceDirty}
										formText={editor.formText}
										sourceError={editor.sourceError}
										onSourceChange={editor.changeSource}
										onApplySource={editor.applySource}
									/>
								),
							},
						]}
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
