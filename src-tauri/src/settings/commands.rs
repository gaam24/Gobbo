//! The app's own settings: the storage choice, the data folder, the platform and opening paths. Provider keys are
//! handled by `sources::commands`.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use super::storage;
use crate::commands::blocking;
use crate::commands::with_data;
use crate::context::Data;
use crate::context::managed::{GameDataUsage, ManagedGame};
use crate::domain::source::{SourceLinkStatus, SourceLinksReport};
use crate::error::AppError;
use crate::settings::config::AppSettings;
use crate::settings::config::CloseAction;
use crate::settings::config::Settings;

crate::commands::game_commands! {
    /// Saves the settings of a game.
    change save_settings(settings: Settings) = |root, game| super::config::save_settings(root, game, settings);

    /// Switches one Proton option on or off. The newest saved list is changed, so a quick second switch keeps the first.
    change set_proton_option(option: String, on: bool) =
        |root, game| crate::context::settings::set_proton_option(root, game, &option, on);

    /// Adds a file or folder to the profile data of a game. A path already listed changes nothing.
    change add_profile_data(path: String) = |root, game| crate::context::settings::add_profile_data(root, game, &path);

    /// Removes a file or folder from the profile data of a game.
    change remove_profile_data(path: String) =
        |root, game| crate::context::settings::remove_profile_data(root, game, &path);

    /// Adds a game with its folder. Answers the games added to Gobbo.
    query add_game(game_root: String) -> Vec<ManagedGame> =
        |root, game| crate::settings::managed::add_game(root, game, &game_root);

    /// Removes a game from Gobbo. Its data stays in `games/<id>.removed`. Answers the games that are left.
    query remove_game() -> Vec<ManagedGame> = crate::settings::managed::remove_game;

    /// Restores the data of a removed game and adds it to the list again.
    query restore_game() -> Vec<ManagedGame> = crate::settings::managed::restore_game;

    /// Deletes the data of a removed game. The active data folder of the same game is not touched.
    query delete_removed_game() -> () = crate::settings::managed::delete_removed_game;
}

/// App settings shared by all games, for example the interface language.
#[tauri::command]
#[specta::specta]
pub async fn app_settings(data: State<'_, Data>) -> Result<AppSettings, AppError> {
    with_data(data, |root| Ok(super::config::load_app_settings(root))).await
}

/// Saves the app settings. Each provider in `source_links` makes Gobbo the system's handler of its deep links; a provider
/// left out gives its links up, when Gobbo has them.
#[tauri::command]
#[specta::specta]
pub async fn save_app_settings(
    app: AppHandle,
    data: State<'_, Data>,
    settings: AppSettings,
) -> Result<(), AppError> {
    let efficiency = settings.efficiency_mode;
    with_data(data, move |root| {
        super::links::apply(&app, &settings.source_links)?;
        let saved = super::config::save_app_settings(root, settings);

        if saved.is_ok() {
            let (focused, minimized) =
                app.get_webview_window("main")
                    .map_or((true, false), |window| {
                        (
                            window.is_focused().unwrap_or(true),
                            window.is_minimized().unwrap_or(false),
                        )
                    });
            super::config::apply_efficiency(efficiency, focused, minimized);
        }

        saved
    })
    .await
}

/// Performs the choice made in the closing dialog; `Ask` is its Cancel. "Don't show this again" is saved by the window
/// like every other app setting, before this runs. Exiting goes through the tray's safe quit, which still asks before
/// stopping a running game.
#[tauri::command]
#[specta::specta]
pub async fn choose_close_action(app: AppHandle, action: CloseAction) -> Result<(), AppError> {
    crate::tray::close::apply(&app, action);
    Ok(())
}

/// The system's handler for each download-link scheme the settings take, and whether it is this Gobbo. Nothing is
/// changed.
#[tauri::command]
#[specta::specta]
pub async fn source_link_status(
    app: AppHandle,
    data: State<'_, Data>,
) -> Result<Vec<SourceLinkStatus>, AppError> {
    with_data(data, move |root| {
        let settings = super::config::load_app_settings(root);
        Ok(super::links::status(&app, &settings.source_links))
    })
    .await
}

/// Takes back the download links of every provider the settings take, when another program has them. Answers what the
/// system uses now, and which providers Gobbo registered again.
#[tauri::command]
#[specta::specta]
pub async fn ensure_source_links(
    app: AppHandle,
    data: State<'_, Data>,
) -> Result<SourceLinksReport, AppError> {
    with_data(data, move |root| {
        let settings = super::config::load_app_settings(root);
        Ok(super::links::repair(&app, &settings.source_links))
    })
    .await
}

/// The data folder, shown in the global settings.
#[tauri::command]
#[specta::specta]
pub fn data_folder(data: State<'_, Data>) -> String {
    data.inner().folder_text()
}

/// The storage mode, the data folder and the file that records the choice.
#[tauri::command]
#[specta::specta]
pub async fn storage_info(data: State<'_, Data>) -> Result<storage::StorageInfo, AppError> {
    with_data(data, |root| Ok(storage::info(root))).await
}

/// The operating system: `windows`, `linux` or `macos`. The interface hides what does not apply to it, such as Proton.
#[tauri::command]
#[specta::specta]
pub const fn platform() -> &'static str {
    std::env::consts::OS
}

/// The Proton builds on this computer, for the game settings to offer. Empty on Windows, where there is no Proton.
#[tauri::command]
#[specta::specta]
pub async fn proton_builds() -> Result<Vec<crate::platform::ProtonBuild>, AppError> {
    blocking(|| {
        let home = crate::games::paths::home_dir()?;
        let installs = crate::games::stores::steam::steam_installs(&home);
        let libraries = crate::games::stores::steam::libraries_in(&installs);
        Ok(crate::platform::installed_protons(&installs, &libraries))
    })
    .await
}

/// The Proton options the game settings can switch on, on this computer. Empty on Windows, where there is no Proton.
#[tauri::command]
#[specta::specta]
pub fn proton_options() -> Vec<String> {
    crate::domain::proton_options::available()
}

/// Opens a folder in the system file manager, or reveals a file in its folder. Only paths that belong
/// to Gobbo's data folder or to the configured game folders are accepted, so the webview cannot ask
/// the system to open an arbitrary file or program.
#[tauri::command]
#[specta::specta]
pub async fn open_path(
    data: State<'_, Data>,
    game: Option<String>,
    path: String,
    reveal: bool,
) -> Result<(), AppError> {
    with_data(data, move |root| {
        let target = PathBuf::from(path.trim());
        // The interface asks either for a folder to open or for a file to reveal. Opening a file, for
        // example an executable inside the game folder, is never a valid request from the webview.
        let valid_kind = if reveal {
            target.is_file()
        } else {
            target.is_dir()
        };

        if !valid_kind || !crate::settings::guard::may_open(root, game.as_deref(), &target) {
            return Err(AppError::user("io.path_not_allowed"));
        }

        let opened = if reveal {
            tauri_plugin_opener::reveal_item_in_dir(&target)
        } else {
            tauri_plugin_opener::open_path(&target, None::<&str>)
        };
        opened.map_err(AppError::unexpected)
    })
    .await
}

/// Games added to Gobbo, with the game folder and whether that folder exists. Games outside this list have no tabs.
#[tauri::command]
#[specta::specta]
pub async fn managed_games(data: State<'_, Data>) -> Result<Vec<ManagedGame>, AppError> {
    with_data(data, crate::context::managed::managed_games).await
}

/// Every game data folder with its size, active and removed, for the storage page.
#[tauri::command]
#[specta::specta]
pub async fn storage_usage(data: State<'_, Data>) -> Result<Vec<GameDataUsage>, AppError> {
    with_data(data, crate::context::managed::data_usage).await
}

/// Checks a folder for a game before the game is added. The result is the marker file the folder has, if the game names one.
#[tauri::command]
#[specta::specta]
pub async fn check_game_folder(
    game: String,
    game_root: String,
) -> Result<Option<String>, AppError> {
    blocking(move || crate::settings::managed::check_game_folder(&game, &game_root)).await
}
