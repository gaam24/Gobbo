import { useState } from "react";
import { commands, type Settings, type Status } from "@/bindings";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { pickFile, pickFolder } from "@/lib/tauri";
import { useLoad } from "@/lib/use-load";
import { isProtonOption, type ProtonOption } from "@/views/settings/game/proton-options";

/// The changes to the Proton switches and the profile data. The backend merges each one into its newest list, so a
/// second change made before the first one answers cannot undo it.
export interface SettingsListActions {
	setProtonOption: (option: ProtonOption, on: boolean) => void | Promise<void>;
	addProfileData: (path: string) => void | Promise<void>;
	removeProfileData: (path: string) => void | Promise<void>;
}

/// The editable copy of a game's settings. The backend status is the source of truth. The paths and the start options
/// are drafts until a field loses focus; every other change saves at once, starting from the last settings the backend
/// answered. Nothing waits for a Save button, so a Proton switch and a folder path behave the same way.
export function useGameSettings(status: Status, onSave: (patch: Partial<Settings>) => void | Promise<void>, lists: SettingsListActions) {
	const { t } = useTranslation();
	const [gameRoot, setGameRoot] = useState(status.settings.game_root);
	const [settingsDir, setSettingsDir] = useState(status.settings.settings_dir);
	const [modsRoot, setModsRoot] = useState(status.settings.mods_root ?? "");
	const [launchEnv, setLaunchEnv] = useState(status.settings.launch_env ?? "");
	const [launchArgs, setLaunchArgs] = useState(status.settings.launch_args ?? "");
	// The Proton builds on this computer; undefined until the list arrives. Windows and macOS answer an empty list.
	const [protonReload, setProtonReload] = useState(0);
	const [protonRefreshing, setProtonRefreshing] = useState(false);
	const protonBuilds = useLoad("proton-builds", () => unwrap(commands.protonBuilds()), {
		reloadKey: protonReload,
		onSuccess: () => setProtonRefreshing(false),
		onError: () => setProtonRefreshing(false),
	});
	// Asks the backend for the builds again, so one installed while the page is open appears without a restart.
	const refreshProtonBuilds = () => {
		setProtonRefreshing(true);
		setProtonReload((value) => value + 1);
	};
	// The Proton options this computer offers, known to the interface; empty on Windows, and until the list arrives.
	const protonOptions = (useLoad("proton-options", () => commands.protonOptions()) ?? []).filter((id): id is ProtonOption =>
		isProtonOption(id),
	);

	/// Whether a tool's program is in the game folder. Without a game folder nothing can be found.
	const toolState = (folder: string | null | undefined) => {
		if (folder) return t("settings.tools.found");
		return status.game_ready ? t("settings.tools.missing") : t("settings.tools.noGame");
	};

	/// A field lost focus: save the whole text part of the form. The hook trims what the backend trims, so the fields
	/// show the saved value and a trailing space does not look like a pending change. A failed save keeps the draft and
	/// reports its error; the user can correct the field and leave it again.
	const saveTextFields = () => {
		const next = {
			game_root: gameRoot.trim(),
			settings_dir: settingsDir.trim(),
			mods_root: modsRoot.trim(),
			launch_env: launchEnv.trim(),
			launch_args: launchArgs.trim(),
		};
		setGameRoot(next.game_root);
		setSettingsDir(next.settings_dir);
		setModsRoot(next.mods_root);
		setLaunchEnv(next.launch_env);
		setLaunchArgs(next.launch_args);
		// Moving through the fields is not a change: an untouched form does not write the settings file on every blur.
		const unchanged =
			next.game_root === status.settings.game_root &&
			next.settings_dir === status.settings.settings_dir &&
			next.mods_root === (status.settings.mods_root ?? "") &&
			next.launch_env === (status.settings.launch_env ?? "") &&
			next.launch_args === (status.settings.launch_args ?? "");
		if (!unchanged) void onSave(next);
	};

	/// A control that saves at once. The action merges the patch with the newest settings, so two quick changes cannot
	/// send an older copy over a newer one.
	const saveNow = (patch: Partial<Settings>) => onSave(patch);

	const toggleProtonOption = (option: ProtonOption, on: boolean) => lists.setProtonOption(option, on);

	const setProtonBuild = (folder: string) => saveNow({ proton_dir: folder });

	const setProtonFpsLimit = (limit: number | null) => saveNow({ proton_fps_limit: limit });

	const pickProton = async () => {
		const picked = await pickFolder();

		if (picked) void setProtonBuild(picked);
	};

	const profileData = status.settings.profile_data ?? [];

	const addProfileData = async (directory: boolean) => {
		const picked = directory ? await pickFolder() : await pickFile();

		if (picked) await lists.addProfileData(picked);
	};

	return {
		gameRoot,
		setGameRoot,
		settingsDir,
		setSettingsDir,
		modsRoot,
		setModsRoot,
		launchEnv,
		setLaunchEnv,
		launchArgs,
		setLaunchArgs,
		protonBuilds,
		protonRefreshing,
		refreshProtonBuilds,
		protonDir: status.settings.proton_dir ?? "",
		setProtonBuild,
		pickProton,
		protonOptions,
		toggleProtonOption,
		protonFpsLimit: status.settings.proton_fps_limit ?? null,
		setProtonFpsLimit,
		profileData,
		addProfileData,
		removeProfileData: lists.removeProfileData,
		toolState,
		saveTextFields,
		saveNow,
	};
}
