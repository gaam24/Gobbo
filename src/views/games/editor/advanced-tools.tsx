import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import type { GameDraft, PrerequisiteDraft, StepDraft, ToolDraft } from "@/views/games/editor/draft";
import { AddRowButton, EditorField, LinesField, RepeatableSection, RepeatRow, TextInput } from "@/views/games/editor/fields";

/// Programs the game definition knows about: tools that make files for the game, and steps that run inside the overlay
/// before or after it. Both are rare, so they live at the end of the advanced section.
export function AdvancedTools({ draft, onChange }: { draft: GameDraft; onChange: (draft: GameDraft) => void }) {
	const { t } = useTranslation();
	const tools = draft.tools ?? [];
	const steps = draft.steps ?? [];

	const setTools = (next: ToolDraft[]) => onChange({ ...draft, tools: next });
	const setTool = (index: number, patch: Partial<ToolDraft>) =>
		setTools(tools.map((tool, at) => (at === index ? { ...tool, ...patch } : tool)));
	const setPrerequisites = (index: number, next: PrerequisiteDraft[]) => setTool(index, { prerequisites: next });
	const setPrerequisite = (index: number, prerequisite: number, patch: Partial<PrerequisiteDraft>) => {
		const list = tools[index]?.prerequisites ?? [];
		setPrerequisites(
			index,
			list.map((entry, at) => (at === prerequisite ? { ...entry, ...patch } : entry)),
		);
	};

	const setSteps = (next: StepDraft[]) => onChange({ ...draft, steps: next });
	const setStep = (index: number, patch: Partial<StepDraft>) =>
		setSteps(steps.map((step, at) => (at === index ? { ...step, ...patch } : step)));

	return (
		<div className="grid gap-8">
			<RepeatableSection
				title={t("editor.tools.tools")}
				hint={t("editor.tools.toolsHint")}
				addLabel={t("editor.tools.addTool")}
				onAdd={() =>
					setTools([
						...tools,
						{
							id: "",
							name: "",
							output: "generated-mod",
							detect: [],
							source: undefined,
							prerequisites: [],
						},
					])
				}
			>
				{tools.map((tool, index) => (
					<RepeatRow
						key={`tool-${tool.id}`}
						title={tool.name || tool.id || t("editor.tools.tool")}
						onRemove={() => setTools(tools.filter((_, at) => at !== index))}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-tool-id-${index}`} label={t("editor.tools.id")}>
								<TextInput id={`editor-tool-id-${index}`} value={tool.id} onChange={(id) => setTool(index, { id })} />
							</EditorField>
							<EditorField id={`editor-tool-name-${index}`} label={t("editor.tools.name")}>
								<TextInput id={`editor-tool-name-${index}`} value={tool.name} onChange={(name) => setTool(index, { name })} />
							</EditorField>
							<EditorField id={`editor-tool-output-${index}`} label={t("editor.tools.output")} hint={t("editor.tools.outputHint")}>
								<TextInput
									id={`editor-tool-output-${index}`}
									value={tool.output}
									onChange={(output) => setTool(index, { output })}
									placeholder="generated-mod"
								/>
							</EditorField>
							<LinesField
								id={`editor-tool-detect-${index}`}
								label={t("editor.tools.detect")}
								value={tool.detect}
								onChange={(detect) => setTool(index, { detect })}
								rows={1}
								placeholder="tools/program.exe"
							/>
							<EditorField id={`editor-tool-source-provider-${index}`} label={t("editor.tools.sourceProvider")}>
								<TextInput
									id={`editor-tool-source-provider-${index}`}
									value={tool.source?.provider ?? ""}
									onChange={(provider) =>
										setTool(index, {
											source: { provider, mod_id: tool.source?.mod_id ?? "" },
										})
									}
									placeholder="provider-id"
								/>
							</EditorField>
							<EditorField id={`editor-tool-source-mod-${index}`} label={t("editor.tools.sourceMod")}>
								<TextInput
									id={`editor-tool-source-mod-${index}`}
									value={tool.source?.mod_id ?? ""}
									onChange={(mod_id) =>
										setTool(index, {
											source: { provider: tool.source?.provider ?? "", mod_id },
										})
									}
									placeholder="8405"
								/>
							</EditorField>
						</div>

						<div className="border-t pt-4">
							<div className="mb-3 flex items-center justify-between gap-3">
								<span className="text-xs font-medium">{t("editor.tools.prerequisites")}</span>
								<AddRowButton
									label={t("editor.tools.addPrerequisite")}
									onClick={() =>
										setPrerequisites(index, [
											...(tool.prerequisites ?? []),
											{
												name: "",
												url: "",
												sha512: "",
												args: [],
												check: "",
												check_starts_with: "",
											},
										])
									}
								/>
							</div>
							<div className="grid gap-3">
								{(tool.prerequisites ?? []).map((prerequisite, at) => (
									<RepeatRow
										key={`prerequisite-${index}-${at}`} // eslint-disable-line react/no-array-index-key -- same reason
										title={prerequisite.name || t("editor.tools.prerequisite")}
										onRemove={() =>
											setPrerequisites(
												index,
												(tool.prerequisites ?? []).filter((_, keep) => keep !== at),
											)
										}
									>
										<div className="grid gap-4 md:grid-cols-2">
											<EditorField id={`editor-prereq-name-${index}-${at}`} label={t("editor.tools.prerequisiteName")}>
												<TextInput
													id={`editor-prereq-name-${index}-${at}`}
													value={prerequisite.name}
													onChange={(name) => setPrerequisite(index, at, { name })}
												/>
											</EditorField>
											<EditorField id={`editor-prereq-url-${index}-${at}`} label={t("editor.tools.prerequisiteUrl")}>
												<TextInput
													id={`editor-prereq-url-${index}-${at}`}
													value={prerequisite.url}
													onChange={(url) => setPrerequisite(index, at, { url })}
													placeholder="https://"
												/>
											</EditorField>
											<EditorField
												id={`editor-prereq-sha-${index}-${at}`}
												label={t("editor.tools.prerequisiteSha")}
												hint={t("editor.tools.prerequisiteShaHint")}
												className="md:col-span-2"
											>
												<TextInput
													id={`editor-prereq-sha-${index}-${at}`}
													value={prerequisite.sha512}
													onChange={(sha512) => setPrerequisite(index, at, { sha512 })}
												/>
											</EditorField>
											<LinesField
												id={`editor-prereq-args-${index}-${at}`}
												label={t("editor.tools.prerequisiteArgs")}
												value={prerequisite.args}
												onChange={(args) => setPrerequisite(index, at, { args })}
												rows={1}
											/>
											<EditorField id={`editor-prereq-check-${index}-${at}`} label={t("editor.tools.prerequisiteCheck")}>
												<TextInput
													id={`editor-prereq-check-${index}-${at}`}
													value={prerequisite.check}
													onChange={(check) => setPrerequisite(index, at, { check })}
													placeholder="drive_c/Program Files/dotnet"
												/>
											</EditorField>
											<EditorField id={`editor-prereq-prefix-${index}-${at}`} label={t("editor.tools.prerequisiteCheckStarts")}>
												<TextInput
													id={`editor-prereq-prefix-${index}-${at}`}
													value={prerequisite.check_starts_with}
													onChange={(check_starts_with) => setPrerequisite(index, at, { check_starts_with })}
													placeholder="8."
												/>
											</EditorField>
										</div>
									</RepeatRow>
								))}
							</div>
						</div>
					</RepeatRow>
				))}
			</RepeatableSection>

			<RepeatableSection
				divided
				title={t("editor.tools.steps")}
				hint={t("editor.tools.stepsHint")}
				addLabel={t("editor.tools.addStep")}
				onAdd={() => setSteps([...steps, { id: "", phase: "before_start", program: "", args: [] }])}
			>
				{steps.map((step, index) => (
					<RepeatRow
						key={`step-${index}`} // eslint-disable-line react/no-array-index-key -- same reason
						title={step.id || t("editor.tools.step")}
						onRemove={() => setSteps(steps.filter((_, at) => at !== index))}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-step-id-${index}`} label={t("editor.tools.id")}>
								<TextInput id={`editor-step-id-${index}`} value={step.id} onChange={(id) => setStep(index, { id })} />
							</EditorField>
							<EditorField id={`editor-step-phase-${index}`} label={t("editor.tools.phase")}>
								<Select
									value={step.phase}
									onValueChange={(phase) =>
										setStep(index, {
											phase: phase === "after_end" ? "after_end" : "before_start",
										})
									}
								>
									<SelectTrigger id={`editor-step-phase-${index}`} className="w-full">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										<SelectItem value="before_start">{t("editor.tools.phaseBefore")}</SelectItem>
										<SelectItem value="after_end">{t("editor.tools.phaseAfter")}</SelectItem>
									</SelectContent>
								</Select>
							</EditorField>
							<EditorField id={`editor-step-program-${index}`} label={t("editor.tools.program")} hint={t("editor.tools.programHint")}>
								<TextInput
									id={`editor-step-program-${index}`}
									value={step.program}
									onChange={(program) => setStep(index, { program })}
									placeholder="tools/compiler.exe"
								/>
							</EditorField>
							<LinesField
								id={`editor-step-args-${index}`}
								label={t("editor.tools.args")}
								value={step.args}
								onChange={(args) => setStep(index, { args })}
								rows={1}
							/>
							<EditorField id={`editor-step-when-${index}`} label={t("editor.tools.whenDir")} hint={t("editor.tools.whenDirHint")}>
								<TextInput
									id={`editor-step-when-${index}`}
									value={step.when_dir ?? ""}
									onChange={(when_dir) => setStep(index, { when_dir })}
								/>
							</EditorField>
							<LinesField
								id={`editor-step-clean-${index}`}
								label={t("editor.tools.clean")}
								value={step.clean}
								onChange={(clean) => setStep(index, { clean })}
								rows={1}
							/>
							<LinesField
								id={`editor-step-outputs-${index}`}
								label={t("editor.tools.outputs")}
								value={step.outputs}
								onChange={(outputs) => setStep(index, { outputs })}
								rows={1}
							/>
							<EditorField id={`editor-step-log-${index}`} label={t("editor.tools.log")}>
								<TextInput id={`editor-step-log-${index}`} value={step.log ?? ""} onChange={(log) => setStep(index, { log })} />
							</EditorField>
						</div>
					</RepeatRow>
				))}
			</RepeatableSection>
		</div>
	);
}
