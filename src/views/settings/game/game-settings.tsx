import { FlaskConical, FolderOpen, Play, Save, SlidersHorizontal, Trash2, Wrench } from "lucide-react";
import { useState } from "react";
import type { GameView, Settings, Status } from "@/bindings";
import { SettingsLayout, type SettingsSection } from "@/components/app/page-layout";
import { ConfirmRemoveGame } from "@/components/confirm-remove-game";
import { PageFrame } from "@/components/page-frame";
import { useTranslation } from "@/i18n";
import { usePlatform } from "@/lib/platform";
import { AdvancedSection } from "@/views/settings/game/advanced-section";
import { FilesSection } from "@/views/settings/game/files-section";
import { ProtonSection } from "@/views/settings/game/proton-section";
import { RemoveSection } from "@/views/settings/game/remove-section";
import { SavesSection, type UserDataActions } from "@/views/settings/game/saves-section";
import { StartSection } from "@/views/settings/game/start-section";
import { ToolsSection } from "@/views/settings/game/tools-section";
import { type SettingsListActions, useGameSettings } from "@/views/settings/game/use-game-settings";

type GameSettingsTab = "files" | "start" | "proton" | "saves" | "tools" | "advanced" | "remove";

/// The settings of one game, split into tabs. Every control saves itself: a switch or a select at once, a text field when
/// it loses focus. The page has no Save button and no second copy of the settings.
export function GameSettings({
	status,
	game,
	onSave,
	lists,
	userData,
	onOpen,
	onCopy,
	onInstallPrerequisites,
	onRemove,
	onExportDefinition,
	onEditDefinition,
}: {
	status: Status;
	game: GameView | undefined;
	onSave: (patch: Partial<Settings>) => void | Promise<void>;
	/// The Proton switches and the profile data, each changed on its own.
	lists: SettingsListActions;
	/// The user-data choices and the move back, shared with the saves section.
	userData: UserDataActions;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	/// Installs what a tool needs in the game's Proton prefix. The button asks first.
	onInstallPrerequisites: (toolId: string) => void | Promise<void>;
	/// Removes the game from the list. The game's files and mods stay on disk.
	onRemove: (id: string) => void | Promise<void>;
	/// Writes the game definition and its icon as a shareable zip.
	onExportDefinition?: () => void | Promise<void>;
	/// Opens the definition editor for a user definition.
	onEditDefinition?: () => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const platform = usePlatform();
	const [confirmRemove, setConfirmRemove] = useState(false);
	const [tab, setTab] = useState<GameSettingsTab>("files");
	const form = useGameSettings(status, onSave, lists);

	const tabs: Array<SettingsSection<GameSettingsTab>> = [
		{
			id: "files",
			label: t("settings.files.title"),
			icon: FolderOpen,
			content: (
				<FilesSection
					status={status}
					gameRoot={form.gameRoot}
					setGameRoot={form.setGameRoot}
					settingsDir={form.settingsDir}
					setSettingsDir={form.setSettingsDir}
					modsRoot={form.modsRoot}
					setModsRoot={form.setModsRoot}
					onOpen={onOpen}
					onCopy={onCopy}
					onCommit={form.saveTextFields}
				/>
			),
		},
		{
			id: "start",
			label: t("settings.start.title"),
			icon: Play,
			content: (
				<StartSection
					status={status}
					launchEnv={form.launchEnv}
					setLaunchEnv={form.setLaunchEnv}
					launchArgs={form.launchArgs}
					setLaunchArgs={form.setLaunchArgs}
					onCopy={onCopy}
					onCommit={form.saveTextFields}
				/>
			),
		},
		...(platform === "linux"
			? [
					{
						id: "proton" as const,
						label: t("settings.proton.title"),
						icon: FlaskConical,
						content: (
							<ProtonSection
								status={status}
								protonBuilds={form.protonBuilds}
								protonDir={form.protonDir}
								refreshing={form.protonRefreshing}
								onPickBuild={form.setProtonBuild}
								onPickFolder={form.pickProton}
								onRefreshBuilds={form.refreshProtonBuilds}
								offered={form.protonOptions}
								onSwitch={form.toggleProtonOption}
								fpsLimit={form.protonFpsLimit}
								onFpsLimit={form.setProtonFpsLimit}
							/>
						),
					},
				]
			: []),
		{
			id: "saves",
			label: t("settings.saves.title"),
			icon: Save,
			content: (
				<SavesSection
					status={status}
					userData={userData}
					onOpen={onOpen}
					profileData={form.profileData}
					onAddProfileData={form.addProfileData}
					onRemoveProfileData={form.removeProfileData}
				/>
			),
		},
		...(status.tools.length > 0
			? [
					{
						id: "tools" as const,
						label: t("settings.tools.title"),
						icon: Wrench,
						content: (
							<ToolsSection status={status} toolState={form.toolState} onInstallPrerequisites={onInstallPrerequisites} onOpen={onOpen} />
						),
					},
				]
			: []),
		{
			id: "advanced",
			label: t("settings.advanced.title"),
			icon: SlidersHorizontal,
			content: <AdvancedSection game={game} onExportDefinition={onExportDefinition} onEditDefinition={onEditDefinition} />,
		},
		{
			id: "remove",
			label: t("settings.remove.title"),
			icon: Trash2,
			danger: true,
			content: <RemoveSection status={status} game={game} onRequestRemove={() => setConfirmRemove(true)} />,
		},
	];

	const active = tabs.some((entry) => entry.id === tab) ? tab : (tabs[0]?.id ?? "files");

	return (
		<PageFrame
			title={t("settings.game.heading", { name: status.game_name })}
			meta={status.game_subtitle}
			hint={t("settings.autoSave")}
			padded
		>
			<SettingsLayout label={t("settings.game.sections")} sections={tabs} value={active} onValueChange={setTab} />

			<ConfirmRemoveGame
				name={status.game_name}
				open={confirmRemove}
				onOpenChange={setConfirmRemove}
				onConfirm={() => {
					if (game) void onRemove(game.id);
				}}
			/>
		</PageFrame>
	);
}
