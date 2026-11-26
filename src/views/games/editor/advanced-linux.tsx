import { SectionHeading, StatusLine } from "@/components/app/content-blocks";
import { useTranslation } from "@/i18n";
import type { DeployDraft, DetectDraft, ExecutableDraft, GameDraft, UserDataDraft } from "@/views/games/editor/draft";
import { EditorField, LinesField, TextInput } from "@/views/games/editor/fields";

/// Every value Gobbo reads only on Linux, in one place: the Linux launchers that find the game, the files of a native
/// Linux build, and the Linux paths of the programs and folders the other sections define. A Windows user can leave the
/// whole section empty; a shared definition still carries what a Linux user needs.
export function AdvancedLinux({ draft, onChange }: { draft: GameDraft; onChange: (draft: GameDraft) => void }) {
	const { t } = useTranslation();
	const detect = draft.detect;
	const sets = draft.user_data ?? [];

	const updateDetect = (patch: Partial<DetectDraft>) => onChange({ ...draft, detect: { ...detect, ...patch } });
	const updateDeploy = (patch: Partial<DeployDraft>) => onChange({ ...draft, deploy: { ...draft.deploy, ...patch } });
	const updateExecutable = (index: number, patch: Partial<ExecutableDraft>) =>
		onChange({
			...draft,
			executables: draft.executables.map((executable, at) => (at === index ? { ...executable, ...patch } : executable)),
		});
	const updateSet = (index: number, patch: Partial<UserDataDraft>) =>
		onChange({
			...draft,
			user_data: sets.map((set, at) => (at === index ? { ...set, ...patch } : set)),
		});

	return (
		<div className="grid gap-10">
			<section className="grid gap-4">
				<SectionHeading title={t("editor.linux.find")} />
				<div className="grid gap-4 md:grid-cols-2">
					<LinesField
						id="editor-heroic-slugs"
						label={t("editor.detection.heroicSlugs")}
						value={detect.heroic_slugs}
						onChange={(heroic_slugs) => updateDetect({ heroic_slugs })}
						rows={1}
					/>
					<LinesField
						id="editor-lutris-slugs"
						label={t("editor.detection.lutrisSlugs")}
						value={detect.lutris_slugs}
						onChange={(lutris_slugs) => updateDetect({ lutris_slugs })}
						rows={1}
					/>
				</div>
				<LinesField
					id="editor-linux-markers"
					label={t("editor.detection.linuxMarkers")}
					hint={t("editor.detection.linuxMarkersHint")}
					value={detect.linux_markers}
					onChange={(linux_markers) => updateDetect({ linux_markers })}
					rows={2}
					placeholder="GameName.x86_64"
				/>
			</section>

			<section className="grid gap-4 border-t pt-8">
				<SectionHeading title={t("editor.linux.programs")} />
				{draft.executables.length === 0 ? (
					<StatusLine>{t("editor.linux.programsEmpty")}</StatusLine>
				) : (
					draft.executables.map((executable, index) => (
						<EditorField
							key={index} // eslint-disable-line react/no-array-index-key -- one field per program row, edited in place
							id={`editor-exe-linux-${index}`}
							label={executable.name || executable.path || t("editor.detection.executable")}
							hint={t("editor.detection.executableLinuxHint")}
						>
							<TextInput
								id={`editor-exe-linux-${index}`}
								value={executable.path_linux ?? ""}
								onChange={(path_linux) => updateExecutable(index, { path_linux })}
								placeholder="GameName.x86_64"
							/>
						</EditorField>
					))
				)}
			</section>

			<section className="grid gap-4 border-t pt-8">
				<SectionHeading title={t("editor.linux.folders")} />
				<EditorField id="editor-root-linux" label={t("editor.deploy.rootLinux")} hint={t("editor.deploy.rootLinuxHint")}>
					<TextInput
						id="editor-root-linux"
						value={draft.deploy.root_linux ?? ""}
						onChange={(root_linux) => updateDeploy({ root_linux })}
						placeholder="{local_appdata}/Game"
					/>
				</EditorField>
				{sets.map((set, index) => (
					<EditorField
						key={index} // eslint-disable-line react/no-array-index-key -- one field per folder row, edited in place
						id={`editor-user-data-linux-${index}`}
						label={t("editor.linux.folderPath", { name: set.id || t("editor.profiles.userData") })}
						hint={t("editor.profiles.pathLinuxHint")}
					>
						<TextInput
							id={`editor-user-data-linux-${index}`}
							value={set.path_linux ?? ""}
							onChange={(path_linux) => updateSet(index, { path_linux })}
						/>
					</EditorField>
				))}
			</section>
		</div>
	);
}
