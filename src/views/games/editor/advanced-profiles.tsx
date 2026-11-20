import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTranslation } from "@/i18n";
import type { GameDraft, UserDataDraft } from "@/views/games/editor/draft";
import { EditorField, LinesField, RepeatableSection, RepeatRow, SwitchField, TextInput } from "@/views/games/editor/fields";

const KIND_LABELS = [
	{ value: "user-data", label: "settings.userData.kinds.user-data" },
	{ value: "settings", label: "settings.userData.kinds.settings" },
	{ value: "saves", label: "settings.userData.kinds.saves" },
	{ value: "config", label: "settings.userData.kinds.config" },
	{ value: "mods", label: "settings.userData.kinds.mods" },
] as const;

/// The folders the game keeps outside its own folder: saves, settings, config, mods. Each one can be shared between
/// profiles or kept separate. `profile_data` is the older, copy-only form of the same idea and stays for simple files.
export function AdvancedProfiles({ draft, onChange }: { draft: GameDraft; onChange: (draft: GameDraft) => void }) {
	const { t } = useTranslation();
	const sets = draft.user_data ?? [];

	const update = (index: number, patch: Partial<UserDataDraft>) =>
		onChange({
			...draft,
			user_data: sets.map((set, at) => (at === index ? { ...set, ...patch } : set)),
		});

	return (
		<div className="grid gap-8">
			<RepeatableSection
				title={t("editor.profiles.userData")}
				hint={t("editor.profiles.userDataHint")}
				addLabel={t("editor.profiles.addUserData")}
				onAdd={() =>
					onChange({
						...draft,
						user_data: [
							...sets,
							{
								id: "",
								kind: "saves",
								path: "{documents}/",
								mode: "redirect",
								default: "shared",
							},
						],
					})
				}
			>
				{sets.map((set, index) => (
					<RepeatRow
						key={`user-data-${set.id}`}
						title={set.id || t("editor.profiles.userData")}
						onRemove={() =>
							onChange({
								...draft,
								user_data: sets.filter((_, at) => at !== index),
							})
						}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-user-data-id-${index}`} label={t("editor.profiles.id")} hint={t("editor.profiles.idHint")}>
								<TextInput
									id={`editor-user-data-id-${index}`}
									value={set.id}
									onChange={(id) => update(index, { id })}
									placeholder="saves"
								/>
							</EditorField>
							<EditorField id={`editor-user-data-kind-${index}`} label={t("editor.profiles.kind")}>
								<Select value={set.kind ?? "user-data"} onValueChange={(kind) => update(index, { kind })}>
									<SelectTrigger id={`editor-user-data-kind-${index}`} className="w-full">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										{KIND_LABELS.map(({ value, label }) => (
											<SelectItem key={value} value={value}>
												{t(label)}
											</SelectItem>
										))}
									</SelectContent>
								</Select>
							</EditorField>
							<EditorField
								id={`editor-user-data-path-${index}`}
								label={t("editor.profiles.path")}
								hint={t("editor.profiles.pathHint")}
								className="md:col-span-2"
							>
								<TextInput
									id={`editor-user-data-path-${index}`}
									value={set.path}
									onChange={(path) => update(index, { path })}
									placeholder="{documents}/Example Game"
								/>
							</EditorField>
							<EditorField
								id={`editor-user-data-linux-${index}`}
								label={t("editor.profiles.pathLinux")}
								hint={t("editor.profiles.pathLinuxHint")}
							>
								<TextInput
									id={`editor-user-data-linux-${index}`}
									value={set.path_linux ?? ""}
									onChange={(path_linux) => update(index, { path_linux })}
								/>
							</EditorField>
							<EditorField id={`editor-user-data-mode-${index}`} label={t("editor.profiles.mode")} hint={t("editor.profiles.modeHint")}>
								<Select value={set.mode ?? "redirect"} onValueChange={(mode) => update(index, { mode })}>
									<SelectTrigger id={`editor-user-data-mode-${index}`} className="w-full">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										<SelectItem value="redirect">{t("editor.profiles.modeRedirect")}</SelectItem>
										<SelectItem value="copy">{t("editor.profiles.modeCopy")}</SelectItem>
									</SelectContent>
								</Select>
							</EditorField>
							<EditorField id={`editor-user-data-default-${index}`} label={t("editor.profiles.default")}>
								<Select value={set.default ?? "shared"} onValueChange={(value) => update(index, { default: value })}>
									<SelectTrigger id={`editor-user-data-default-${index}`} className="w-full">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										<SelectItem value="shared">{t("editor.profiles.defaultShared")}</SelectItem>
										<SelectItem value="isolated">{t("editor.profiles.defaultIsolated")}</SelectItem>
									</SelectContent>
								</Select>
							</EditorField>
						</div>
						<SwitchField
							id={`editor-user-data-inherit-${index}`}
							label={t("editor.profiles.inherit")}
							hint={t("editor.profiles.inheritHint")}
							checked={set.inherit ?? true}
							onChange={(inherit) => update(index, { inherit })}
						/>
					</RepeatRow>
				))}
			</RepeatableSection>

			<div className="border-t pt-6">
				<LinesField
					id="editor-profile-data"
					label={t("editor.profiles.profileData")}
					hint={t("editor.profiles.profileDataHint")}
					value={draft.profile_data}
					onChange={(profile_data) => onChange({ ...draft, profile_data })}
					rows={2}
					placeholder="{documents}/Example Game/settings.ini"
				/>
			</div>
		</div>
	);
}
