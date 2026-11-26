import { useState } from "react";
import { StatusLine } from "@/components/app/content-blocks";
import { useTranslation } from "@/i18n";
import { pickFile } from "@/lib/tauri";
import type { DetectDraft, ExecutableDraft, GameDraft } from "@/views/games/editor/draft";
import { AddRowButton, EditorField, LinesField, RepeatRow, SwitchField, TextInput } from "@/views/games/editor/fields";
import { FoundPicker } from "@/views/games/editor/found-picker";
import { relativeToFolder } from "@/views/games/editor/game-path";

/// The detection fields: how Gobbo finds the folder again on this computer and on another one. The Linux-only values
/// (Heroic and Lutris, native Linux files and programs) live in `AdvancedLinux`. The quick step fills
/// the marker and the Steam values from the folder; this section is for the rest and for a second store. `programs` are
/// the programs Gobbo found in the game folder, and a program browsed for is made relative to `folder`.
export function AdvancedDetection({
	draft,
	onChange,
	programs,
	folder,
}: {
	draft: GameDraft;
	onChange: (draft: GameDraft) => void;
	programs: string[];
	folder: string;
}) {
	const { t } = useTranslation();
	const detect = draft.detect;
	const [outsideRow, setOutsideRow] = useState<number | null>(null);

	const update = (patch: Partial<DetectDraft>) => onChange({ ...draft, detect: { ...detect, ...patch } });
	const updateExecutable = (index: number, patch: Partial<ExecutableDraft>) => {
		const executables = draft.executables.map((executable, at) => (at === index ? { ...executable, ...patch } : executable));
		onChange({ ...draft, executables });
	};

	const browseProgram = async (index: number) => {
		const picked = await pickFile();

		if (picked === null) return;

		const relative = relativeToFolder(folder, picked);
		setOutsideRow(relative === null ? index : null);

		if (relative !== null) updateExecutable(index, { path: relative });
	};

	return (
		<div className="grid gap-6">
			<div className="grid gap-4 md:grid-cols-2">
				<LinesField
					id="editor-steam-ids"
					label={t("editor.detection.steamIds")}
					hint={t("editor.detection.steamIdsHint")}
					value={(detect.steam_app_ids ?? []).map(String)}
					onChange={(lines) =>
						update({
							steam_app_ids: lines.map((line) => Number.parseInt(line, 10)).filter((id) => Number.isInteger(id) && id > 0),
						})
					}
					rows={1}
					placeholder="292030"
				/>
				<LinesField
					id="editor-steam-dirs"
					label={t("editor.detection.steamDirs")}
					hint={t("editor.detection.steamDirsHint")}
					value={detect.steam_install_dirs}
					onChange={(steam_install_dirs) => update({ steam_install_dirs })}
					rows={1}
				/>
			</div>
			<div className="grid gap-4 md:grid-cols-2">
				<LinesField
					id="editor-gog-ids"
					label={t("editor.detection.gogIds")}
					value={detect.gog_ids}
					onChange={(gog_ids) => update({ gog_ids })}
					rows={1}
				/>
				<LinesField
					id="editor-epic-ids"
					label={t("editor.detection.epicIds")}
					value={detect.epic_ids}
					onChange={(epic_ids) => update({ epic_ids })}
					rows={1}
				/>
			</div>
			<LinesField
				id="editor-markers"
				label={t("editor.detection.markers")}
				hint={t("editor.detection.markersHint")}
				value={detect.markers}
				onChange={(markers) => update({ markers })}
				rows={3}
				placeholder={"bin/game.exe\ncontent/base"}
			/>
			<SwitchField
				id="editor-any-marker"
				label={t("editor.detection.anyMarker")}
				hint={t("editor.detection.anyMarkerHint")}
				checked={detect.any_marker ?? false}
				onChange={(any_marker) => update({ any_marker })}
			/>

			<div className="border-t pt-6">
				<div className="mb-4 flex items-center justify-between gap-3">
					<h3 className="text-sm font-semibold">{t("editor.detection.executables")}</h3>
					<AddRowButton
						label={t("editor.detection.addExecutable")}
						onClick={() =>
							onChange({
								...draft,
								executables: [...draft.executables, { name: "", path: "" }],
							})
						}
					/>
				</div>
				<div className="grid gap-3">
					{draft.executables.map((executable, index) => (
						<RepeatRow
							key={index} // eslint-disable-line react/no-array-index-key -- the rows are edited in place
							title={executable.name || executable.path || t("editor.detection.executable")}
							onRemove={() =>
								onChange({
									...draft,
									executables: draft.executables.filter((_, at) => at !== index),
								})
							}
						>
							<div className="grid gap-4 md:grid-cols-2">
								<EditorField id={`editor-exe-name-${index}`} label={t("editor.detection.executableName")}>
									<TextInput
										id={`editor-exe-name-${index}`}
										value={executable.name}
										onChange={(name) => updateExecutable(index, { name })}
									/>
								</EditorField>
								<EditorField id={`editor-exe-path-${index}`} label={t("editor.detection.executablePath")}>
									<FoundPicker
										id={`editor-exe-path-${index}`}
										value={executable.path}
										onChange={(path) => {
											setOutsideRow(null);
											updateExecutable(index, { path });
										}}
										found={programs}
										menuLabel={t("editor.quick.foundPrograms")}
										onBrowse={folder.trim() === "" ? undefined : () => void browseProgram(index)}
									/>
									{outsideRow === index && <StatusLine tone="error">{t("editor.quick.programOutside")}</StatusLine>}
								</EditorField>
								<LinesField
									id={`editor-exe-args-${index}`}
									label={t("editor.detection.executableArgs")}
									value={executable.args}
									onChange={(args) => updateExecutable(index, { args })}
									rows={2}
									placeholder="-windowed"
								/>
							</div>
						</RepeatRow>
					))}
				</div>
			</div>
		</div>
	);
}
