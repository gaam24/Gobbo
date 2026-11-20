//! The game settings file: the shape, its file name and the read used by the context and the features.

use serde::{Deserialize, Serialize};

use crate::context::Data;
use crate::error::{AppError, io_at};
use crate::infra::json;
use std::fs;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Serializes the edits of one list in the settings file. Each edit reads the newest file, changes one entry and saves,
/// so two edits running at once would otherwise each save a copy without the other's change.
static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

fn lock_settings() -> MutexGuard<'static, ()> {
    SETTINGS_WRITE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The settings file in a game's data folder.
pub const CONFIG_FILE: &str = "config.json";

/// Settings entered in the app. Saved in `config.json` in the game's data folder.
#[derive(Debug, Default, Clone, Serialize, Deserialize, specta::Type)]
pub struct Settings {
    /// The game folder, for example `.../The Witcher 3`.
    pub game_root: String,
    /// The folder with `mods.settings`. Empty when the game does not use it.
    pub settings_dir: String,
    /// The folder that holds the game's mods when it is not the game folder, for example
    /// `%APPDATA%/MyGame`. Empty means "use the definition or the game folder".
    #[serde(default)]
    pub mods_root: String,
    /// The folder with the `proton` file that starts the game. Empty when Gobbo picks Proton itself.
    #[serde(default)]
    pub proton_dir: String,
    /// Environment variables for the launched game, one per line: `NAME=value`.
    #[serde(default)]
    pub launch_env: String,
    /// Parameters added after the game file, separated by spaces.
    #[serde(default)]
    pub launch_args: String,
    /// The Proton options switched on, by id (`wined3d`, `proton_log`, ...). See `domain::proton_options`. Linux only.
    #[serde(default)]
    pub proton_options: Vec<String>,
    /// Frame rate cap for Proton games, written for DXVK and vkd3d-proton. `None` means unlimited. Linux only.
    #[serde(default)]
    pub proton_fps_limit: Option<u32>,
    /// Files and folders that belong to one profile: saves, configuration, `mods.settings` and similar.
    /// Absolute paths. The settings file is added automatically.
    #[serde(default)]
    pub profile_data: Vec<String>,
}

impl Settings {
    /// The folder-valued settings. Saving trims them and the path guard allows them; a new folder field is added here
    /// once instead of in every caller.
    pub const fn paths(&self) -> [&str; 4] {
        [
            self.game_root.as_str(),
            self.settings_dir.as_str(),
            self.mods_root.as_str(),
            self.proton_dir.as_str(),
        ]
    }

    /// Mutable form of [`Settings::paths`], for the save-time trim.
    pub const fn paths_mut(&mut self) -> [&mut String; 4] {
        [
            &mut self.game_root,
            &mut self.settings_dir,
            &mut self.mods_root,
            &mut self.proton_dir,
        ]
    }
}

/// Reads a game's settings. A missing file means the default settings.
pub fn load_settings(data: &Data) -> Settings {
    let path = data.config();
    let parse = || Ok::<Settings, AppError>(json::read_json_or_default(&path));
    data.settings_cache().map_or_else(
        || json::read_json_or_default(&path),
        |cache| cache.load_cloned(&path, parse).unwrap_or_default(),
    )
}

/// Saves a game's settings after trimming the paths. It waits for a list edit that is running, so a whole save and an
/// edit of one list never write over each other.
pub fn save_settings(root: &Data, game_id: &str, settings: Settings) -> Result<(), AppError> {
    let _write = lock_settings();
    write_settings(root, game_id, settings)
}

/// `save_settings` for a caller that already holds the settings lock.
fn write_settings(root: &Data, game_id: &str, settings: Settings) -> Result<(), AppError> {
    crate::games::load(game_id)?;
    let data = root.game(game_id);
    let mut settings = settings;

    for value in settings.paths_mut() {
        let trimmed = value.trim().to_string();
        *value = trimmed;
    }

    let settings = Settings {
        game_root: settings.game_root,
        settings_dir: settings.settings_dir,
        mods_root: settings.mods_root,
        proton_dir: settings.proton_dir,
        launch_env: settings.launch_env.trim().to_string(),
        launch_args: settings.launch_args.trim().to_string(),
        proton_options: crate::domain::proton_options::known(&settings.proton_options),
        proton_fps_limit: settings
            .proton_fps_limit
            .filter(|frames| *frames > 0 && *frames <= 1000),
        profile_data: settings
            .profile_data
            .into_iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect(),
    };
    crate::domain::arguments::parse_env(&settings.launch_env)?;
    fs::create_dir_all(&data.dir).map_err(io_at(&data.dir))?;
    json::write_json(&data.config(), &settings)?;
    data.remember_settings(&settings);
    Ok(())
}

/// Switches one Proton option on or off, on top of the newest saved list. Nothing changes on a computer that offers no
/// Proton options (Windows), and an unknown id is dropped by the rule, not written.
pub fn set_proton_option(
    root: &Data,
    game_id: &str,
    option: &str,
    on: bool,
) -> Result<(), AppError> {
    if !crate::domain::proton_options::available()
        .iter()
        .any(|id| id == option)
    {
        return Ok(());
    }

    let _write = lock_settings();
    let mut settings = load_settings(&root.game(game_id));
    settings.proton_options =
        crate::domain::proton_options::switched(&settings.proton_options, option, on);
    write_settings(root, game_id, settings)
}

/// Adds a file or folder to the profile data, on top of the newest saved list. A path already listed, or a blank one,
/// changes nothing.
pub fn add_profile_data(root: &Data, game_id: &str, path: &str) -> Result<(), AppError> {
    let path = path.trim();

    if path.is_empty() {
        return Ok(());
    }

    let _write = lock_settings();
    let mut settings = load_settings(&root.game(game_id));

    if settings.profile_data.iter().any(|kept| kept == path) {
        return Ok(());
    }

    settings.profile_data.push(path.to_string());
    write_settings(root, game_id, settings)
}

/// Removes a path from the profile data, on top of the newest saved list. A path that is not listed changes nothing.
pub fn remove_profile_data(root: &Data, game_id: &str, path: &str) -> Result<(), AppError> {
    let path = path.trim();
    let _write = lock_settings();
    let mut settings = load_settings(&root.game(game_id));

    if !settings.profile_data.iter().any(|kept| kept == path) {
        return Ok(());
    }

    settings.profile_data.retain(|kept| kept != path);
    write_settings(root, game_id, settings)
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn the_path_list_trims_every_folder_setting() {
        let mut settings = Settings {
            game_root: " /game ".into(),
            settings_dir: " /settings ".into(),
            mods_root: " /mods ".into(),
            proton_dir: " /proton ".into(),
            ..Settings::default()
        };

        for value in settings.paths_mut() {
            let trimmed = value.trim().to_string();
            *value = trimmed;
        }

        assert_eq!(settings.paths(), ["/game", "/settings", "/mods", "/proton"]);
    }
}
