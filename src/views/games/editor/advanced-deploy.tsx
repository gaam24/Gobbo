import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import type { DeployDraft, GameDraft, MirrorDraft, MoveDraft, SharedDraft, SlotDraft } from "@/views/games/editor/draft";
import { EditorField, LinesField, RepeatableSection, RepeatRow, SwitchField, TextInput } from "@/views/games/editor/fields";

/// Every deploy rule: the folders a mod may use, the game paths it may keep, and the archive names that stand for a
/// game folder. The quick step writes only the first two; this section is where a user adds a mirror or an alias.
export function AdvancedDeploy({
	draft,
	onChange,
	directories,
}: {
	draft: GameDraft;
	onChange: (draft: GameDraft) => void;
	/// The folders found in the game, offered as suggestions.
	directories: string[];
}) {
	const { t } = useTranslation();
	const deploy = draft.deploy;
	const slots = deploy.slots ?? [];
	const mirrors = deploy.mirror ?? [];
	const shared = deploy.shared ?? [];
	const moves = deploy.move ?? [];
	const aliases = Object.entries(deploy.aliases ?? {});

	const update = (patch: Partial<DeployDraft>) => onChange({ ...draft, deploy: { ...deploy, ...patch } });
	const updateSlot = (index: number, patch: Partial<SlotDraft>) =>
		update({
			slots: slots.map((slot, at) => (at === index ? { ...slot, ...patch } : slot)),
		});
	const updateMirror = (index: number, patch: Partial<MirrorDraft>) =>
		update({
			mirror: mirrors.map((mirror, at) => (at === index ? { ...mirror, ...patch } : mirror)),
		});
	const updateShared = (index: number, patch: Partial<SharedDraft>) =>
		update({
			shared: shared.map((entry, at) => (at === index ? { ...entry, ...patch } : entry)),
		});
	const updateMove = (index: number, patch: Partial<MoveDraft>) =>
		update({
			move: moves.map((entry, at) => (at === index ? { ...entry, ...patch } : entry)),
		});

	const aliasRows = (next: Record<string, string>) => update({ aliases: next });
	const setAlias = (index: number, name: string, target: string) => {
		const next: Record<string, string> = {};
		aliases.forEach(([key, value], at) => {
			next[at === index ? name : key] = at === index ? target : value;
		});
		aliasRows(next);
	};
	const addAlias = () => {
		let name = "alias";
		let suffix = 2;

		while (name in (deploy.aliases ?? {})) {
			name = `alias${suffix}`;
			suffix += 1;
		}

		aliasRows({ ...deploy.aliases, [name]: "" });
	};

	return (
		<div className="grid gap-8">
			<div className="grid gap-4 md:grid-cols-2">
				<EditorField id="editor-root" label={t("editor.deploy.root")} hint={t("editor.deploy.rootHint")}>
					<TextInput id="editor-root" value={deploy.root ?? ""} onChange={(root) => update({ root })} placeholder="{game}" />
				</EditorField>
				<EditorField id="editor-root-linux" label={t("editor.deploy.rootLinux")} hint={t("editor.deploy.rootLinuxHint")}>
					<TextInput
						id="editor-root-linux"
						value={deploy.root_linux ?? ""}
						onChange={(root_linux) => update({ root_linux })}
						placeholder="{local_appdata}/Game"
					/>
				</EditorField>
			</div>
			<LinesField
				id="editor-root-files"
				label={t("editor.deploy.rootFiles")}
				hint={t("editor.deploy.rootFilesHint")}
				value={deploy.root_files}
				onChange={(root_files) => update({ root_files })}
				rows={2}
				placeholder="winhttp.dll"
			/>
			<LinesField
				id="editor-protected"
				label={t("editor.deploy.protected")}
				hint={t("editor.deploy.protectedHint")}
				value={deploy.protected}
				onChange={(protectedFolders) => update({ protected: protectedFolders })}
				rows={2}
				placeholder="bin"
			/>
			<SwitchField
				id="editor-resource-names"
				label={t("editor.deploy.resourceNames")}
				hint={t("editor.deploy.resourceNamesHint")}
				checked={deploy.resource_names ?? false}
				onChange={(resource_names) => update({ resource_names })}
			/>
			<SwitchField
				id="editor-full-root"
				label={t("editor.deploy.fullRoot")}
				hint={t("editor.deploy.fullRootHint")}
				checked={deploy.full_root ?? false}
				onChange={(full_root) => update({ full_root })}
			/>

			<RepeatableSection
				divided
				title={t("editor.deploy.slots")}
				hint={t("editor.deploy.slotsHint")}
				addLabel={t("editor.deploy.addSlot")}
				onAdd={() => update({ slots: [...slots, { dir: "" }] })}
			>
				{slots.map((slot, index) => (
					<RepeatRow
						key={slot.dir}
						title={slot.dir || t("editor.deploy.slot")}
						onRemove={() => update({ slots: slots.filter((_, at) => at !== index) })}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-slot-dir-${index}`} label={t("editor.deploy.slotDir")}>
								<TextInput
									id={`editor-slot-dir-${index}`}
									value={slot.dir}
									onChange={(dir) => updateSlot(index, { dir })}
									placeholder={suggest(directories, "Mods")}
								/>
							</EditorField>
							<EditorField id={`editor-slot-prefix-${index}`} label={t("editor.deploy.slotPrefix")}>
								<TextInput
									id={`editor-slot-prefix-${index}`}
									value={slot.prefix ?? ""}
									onChange={(prefix) => updateSlot(index, { prefix })}
								/>
							</EditorField>
							<EditorField
								id={`editor-slot-marker-${index}`}
								label={t("editor.deploy.slotMarker")}
								hint={t("editor.deploy.slotMarkerHint")}
							>
								<TextInput
									id={`editor-slot-marker-${index}`}
									value={slot.marker ?? ""}
									onChange={(marker) => updateSlot(index, { marker })}
									placeholder="content"
								/>
							</EditorField>
							<LinesField
								id={`editor-slot-ext-${index}`}
								label={t("editor.deploy.extensions")}
								value={slot.extensions}
								onChange={(extensions) => updateSlot(index, { extensions })}
								rows={1}
								placeholder=".pak"
							/>
							<LinesField
								id={`editor-slot-root-ext-${index}`}
								label={t("editor.deploy.rootExtensions")}
								value={slot.root_extensions}
								onChange={(root_extensions) => updateSlot(index, { root_extensions })}
								rows={1}
								placeholder=".dll"
							/>
							<LinesField
								id={`editor-slot-root-folder-ext-${index}`}
								label={t("editor.deploy.rootFolderExtensions")}
								value={slot.root_folder_extensions}
								onChange={(root_folder_extensions) => updateSlot(index, { root_folder_extensions })}
								rows={1}
								placeholder=".dll"
							/>
						</div>
						<SwitchField
							id={`editor-slot-flat-${index}`}
							label={t("editor.deploy.slotFlat")}
							hint={t("editor.deploy.slotFlatHint")}
							checked={slot.flat ?? false}
							onChange={(flat) => updateSlot(index, { flat })}
						/>
					</RepeatRow>
				))}
			</RepeatableSection>

			<RepeatableSection
				divided
				title={t("editor.deploy.mirrors")}
				hint={t("editor.deploy.mirrorsHint")}
				addLabel={t("editor.deploy.addMirror")}
				onAdd={() => update({ mirror: [...mirrors, { dir: "" }] })}
			>
				{mirrors.map((mirror, index) => (
					<RepeatRow
						key={`mirror-${index}`} // eslint-disable-line react/no-array-index-key -- same reason
						title={mirror.dir || t("editor.deploy.mirror")}
						onRemove={() => update({ mirror: mirrors.filter((_, at) => at !== index) })}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-mirror-dir-${index}`} label={t("editor.deploy.mirrorDir")}>
								<TextInput
									id={`editor-mirror-dir-${index}`}
									value={mirror.dir}
									onChange={(dir) => updateMirror(index, { dir })}
									placeholder={suggest(directories, "Data")}
								/>
							</EditorField>
							<EditorField id={`editor-mirror-prefix-${index}`} label={t("editor.deploy.mirrorPrefix")}>
								<TextInput
									id={`editor-mirror-prefix-${index}`}
									value={mirror.prefix ?? ""}
									onChange={(prefix) => updateMirror(index, { prefix })}
								/>
							</EditorField>
							<LinesField
								id={`editor-mirror-ext-${index}`}
								label={t("editor.deploy.extensions")}
								value={mirror.extensions}
								onChange={(extensions) => updateMirror(index, { extensions })}
								rows={1}
								placeholder=".esp, .bsa"
							/>
						</div>
					</RepeatRow>
				))}
			</RepeatableSection>

			<RepeatableSection
				divided
				title={t("editor.deploy.moves")}
				hint={t("editor.deploy.movesHint")}
				addLabel={t("editor.deploy.addMove")}
				onAdd={() => update({ move: [...moves, { to: "", extensions: [] }] })}
			>
				{moves.map((entry, index) => (
					<RepeatRow
						key={`move-${index}`} // eslint-disable-line react/no-array-index-key -- same reason
						title={entry.to || t("editor.deploy.move")}
						onRemove={() => update({ move: moves.filter((_, at) => at !== index) })}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-move-to-${index}`} label={t("editor.deploy.moveTo")}>
								<TextInput
									id={`editor-move-to-${index}`}
									value={entry.to}
									onChange={(to) => updateMove(index, { to })}
									placeholder="archive/pc/mod"
								/>
							</EditorField>
							<LinesField
								id={`editor-move-ext-${index}`}
								label={t("editor.deploy.extensions")}
								value={entry.extensions}
								onChange={(extensions) => updateMove(index, { extensions })}
								rows={1}
								placeholder=".archive, .xl"
							/>
						</div>
					</RepeatRow>
				))}
			</RepeatableSection>

			<RepeatableSection
				divided
				title={t("editor.deploy.shared")}
				hint={t("editor.deploy.sharedHint")}
				addLabel={t("editor.deploy.addShared")}
				onAdd={() => update({ shared: [...shared, { dir: "", extensions: [] }] })}
			>
				{shared.map((entry, index) => (
					<RepeatRow
						key={`shared-${index}`} // eslint-disable-line react/no-array-index-key -- same reason
						title={entry.dir?.trim() ? entry.dir : t("editor.deploy.shared")}
						onRemove={() => update({ shared: shared.filter((_, at) => at !== index) })}
					>
						<div className="grid gap-4 md:grid-cols-2">
							<EditorField id={`editor-shared-dir-${index}`} label={t("editor.deploy.sharedDir")} hint={t("editor.deploy.sharedDirHint")}>
								<TextInput
									id={`editor-shared-dir-${index}`}
									value={entry.dir ?? ""}
									onChange={(dir) => updateShared(index, { dir })}
									placeholder="content"
								/>
							</EditorField>
							<LinesField
								id={`editor-shared-ext-${index}`}
								label={t("editor.deploy.extensions")}
								value={entry.extensions}
								onChange={(extensions) => updateShared(index, { extensions })}
								rows={1}
								placeholder=".ws, .xml"
							/>
						</div>
					</RepeatRow>
				))}
			</RepeatableSection>

			<RepeatableSection
				divided
				title={t("editor.deploy.aliases")}
				hint={t("editor.deploy.aliasesHint")}
				addLabel={t("editor.deploy.addAlias")}
				onAdd={addAlias}
			>
				{aliases.map(([name, target], index) => (
					<div key={name} className="grid gap-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto] md:items-end">
						<EditorField id={`editor-alias-name-${index}`} label={t("editor.deploy.aliasName")}>
							<TextInput
								id={`editor-alias-name-${index}`}
								value={name}
								onChange={(next) => setAlias(index, next, target)}
								placeholder="plugins"
							/>
						</EditorField>
						<EditorField id={`editor-alias-target-${index}`} label={t("editor.deploy.aliasTarget")}>
							<TextInput
								id={`editor-alias-target-${index}`}
								value={target}
								onChange={(next) => setAlias(index, name, next)}
								placeholder="BepInEx/plugins"
							/>
						</EditorField>
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							aria-label={t("editor.removeRow")}
							onClick={() => {
								const next = { ...deploy.aliases };
								delete next[name];
								aliasRows(next);
							}}
						>
							<X />
						</Button>
					</div>
				))}
			</RepeatableSection>
		</div>
	);
}

/// The first suggestion that resembles a usual folder name, else the placeholder itself. It only fills a placeholder,
/// so a game with unusual folders is not forced into a name from another game.
function suggest(directories: string[], usual: string): string {
	return directories.find((directory) => directory.toLowerCase() === usual.toLowerCase()) ?? usual;
}
