import type { Status } from "@/bindings";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import { commitOnEnter, Section, SettingRow } from "@/views/settings/controls";
import { PathActions } from "@/views/settings/path-actions";

/// One folder row: the typed draft commits on blur or Enter, and the buttons act on what is typed, not on what was saved.
function PathSettingRow({
	title,
	htmlFor,
	hint,
	value,
	placeholder,
	path,
	onChange,
	onCommit,
	onOpen,
	onCopy,
}: {
	title: string;
	htmlFor: string;
	hint: string;
	value: string;
	placeholder: string;
	path: string;
	onChange: (value: string) => void;
	onCommit: () => void;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
}) {
	return (
		<SettingRow title={title} htmlFor={htmlFor} hint={hint}>
			<div className="flex flex-wrap gap-2">
				<Input
					id={htmlFor}
					value={value}
					title={value || undefined}
					onChange={(event) => onChange(event.currentTarget.value)}
					onBlur={onCommit}
					onKeyDown={commitOnEnter}
					placeholder={placeholder}
					spellCheck={false}
					className="min-w-0 flex-1 basis-64"
				/>
				<PathActions path={path} onOpen={onOpen} onCopy={onCopy} />
			</div>
		</SettingRow>
	);
}

/// The game folder and the folders the mods read from. They are drafts until the form is saved, so the buttons next to
/// each one open and copy what the user has typed, not only what was saved.
export function FilesSection({
	status,
	gameRoot,
	setGameRoot,
	settingsDir,
	setSettingsDir,
	modsRoot,
	setModsRoot,
	onOpen,
	onCopy,
	onCommit,
}: {
	status: Status;
	gameRoot: string;
	setGameRoot: (value: string) => void;
	settingsDir: string;
	setSettingsDir: (value: string) => void;
	modsRoot: string;
	setModsRoot: (value: string) => void;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	/// Saves the typed paths when a field loses focus or Enter is pressed.
	onCommit: () => void;
}) {
	const { t } = useTranslation();

	return (
		<Section id="settings-files" title={t("settings.files.title")} hint={t("settings.files.hint")}>
			<PathSettingRow
				title={t("settings.gameRoot.title")}
				htmlFor="game-root"
				hint={status.game_ready ? t("settings.folder.exists") : t("settings.gameRoot.missing")}
				value={gameRoot}
				placeholder={t("settings.gameRoot.placeholder")}
				path={gameRoot}
				onChange={setGameRoot}
				onCommit={onCommit}
				onOpen={onOpen}
				onCopy={onCopy}
			/>

			{status.needs_settings_dir && (
				<PathSettingRow
					title={t("settings.settingsDir.title")}
					htmlFor="settings-dir"
					hint={status.settings_ready ? t("settings.folder.exists") : t("settings.settingsDir.missing")}
					value={settingsDir}
					placeholder={t("settings.settingsDir.placeholder")}
					path={settingsDir}
					onChange={setSettingsDir}
					onCommit={onCommit}
					onOpen={onOpen}
					onCopy={onCopy}
				/>
			)}

			{status.needs_mods_root && (
				<PathSettingRow
					title={t("settings.modsRoot.title")}
					htmlFor="mods-root"
					hint={status.mods_ready ? t("settings.modsRoot.hint") : t("settings.modsRoot.missing")}
					value={modsRoot}
					placeholder={status.mods_root || t("settings.modsRoot.placeholder")}
					path={modsRoot || status.mods_root}
					onChange={setModsRoot}
					onCommit={onCommit}
					onOpen={onOpen}
					onCopy={onCopy}
				/>
			)}
		</Section>
	);
}
