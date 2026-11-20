import type { Status } from "@/bindings";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";
import { commitOnEnter, Section, SettingRow } from "@/views/settings/controls";
import { PathActions } from "@/views/settings/path-actions";

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
			<SettingRow
				title={t("settings.gameRoot.title")}
				htmlFor="game-root"
				hint={status.game_ready ? t("settings.folder.exists") : t("settings.gameRoot.missing")}
			>
				<div className="flex flex-wrap gap-2">
					<Input
						id="game-root"
						value={gameRoot}
						onChange={(event) => setGameRoot(event.currentTarget.value)}
						onBlur={onCommit}
						onKeyDown={commitOnEnter}
						placeholder={t("settings.gameRoot.placeholder")}
						spellCheck={false}
						className="min-w-0 flex-1 basis-64"
					/>
					<PathActions path={gameRoot} onOpen={onOpen} onCopy={onCopy} />
				</div>
			</SettingRow>

			{status.needs_settings_dir && (
				<SettingRow
					title={t("settings.settingsDir.title")}
					htmlFor="settings-dir"
					hint={status.settings_ready ? t("settings.folder.exists") : t("settings.settingsDir.missing")}
				>
					<div className="flex flex-wrap gap-2">
						<Input
							id="settings-dir"
							value={settingsDir}
							onChange={(event) => setSettingsDir(event.currentTarget.value)}
							onBlur={onCommit}
							onKeyDown={commitOnEnter}
							placeholder={t("settings.settingsDir.placeholder")}
							spellCheck={false}
							className="min-w-0 flex-1 basis-64"
						/>
						<PathActions path={settingsDir} onOpen={onOpen} onCopy={onCopy} />
					</div>
				</SettingRow>
			)}

			{status.needs_mods_root && (
				<SettingRow
					title={t("settings.modsRoot.title")}
					htmlFor="mods-root"
					hint={status.mods_ready ? t("settings.modsRoot.hint") : t("settings.modsRoot.missing")}
				>
					<div className="flex flex-wrap gap-2">
						<Input
							id="mods-root"
							value={modsRoot}
							onChange={(event) => setModsRoot(event.currentTarget.value)}
							onBlur={onCommit}
							onKeyDown={commitOnEnter}
							placeholder={status.mods_root || t("settings.modsRoot.placeholder")}
							spellCheck={false}
							className="min-w-0 flex-1 basis-64"
						/>
						<PathActions path={modsRoot || status.mods_root} onOpen={onOpen} onCopy={onCopy} />
					</div>
				</SettingRow>
			)}
		</Section>
	);
}
