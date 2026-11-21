//! The game catalog: the games in `games/`, their icons, and the scan of Steam and the other stores for them.

use tauri::ipc::Channel;

use crate::commands::blocking;
use crate::domain::pathkey::path_key;
use crate::error::AppError;
use crate::games::{self, detect, editor};

/// Every game from the bundled and user definition folders, with invalid user files reported. The frontend gets only
/// what the UI shows.
#[tauri::command]
#[specta::specta]
pub async fn list_games() -> Result<games::GameCatalog, AppError> {
    blocking(games::catalog).await
}

/// Writes a shareable zip with the definition and its icon.
#[tauri::command]
#[specta::specta]
pub async fn export_definition(game: String, path: String) -> Result<(), AppError> {
    blocking(move || games::export(&game, std::path::Path::new(&path))).await
}

/// The game icon from `games/<id>/icon.webp` (or `icon.png`) as a data URL. `None` when there is no icon.
#[tauri::command]
#[specta::specta]
pub async fn game_icon(game: String) -> Result<Option<String>, AppError> {
    blocking(move || games::icon(&game)).await
}

/// Opens the page a game definition names for a required file, for example the `BepInEx` releases page. The URL comes
/// from the definition, so the window cannot open an arbitrary address.
#[tauri::command]
#[specta::specta]
pub async fn open_requirement_page(game: String, path: String) -> Result<(), AppError> {
    blocking(move || {
        let definition = games::load(&game)?;
        let requirement = definition
            .requirements
            .iter()
            .find(|requirement| path_key(&requirement.path) == path_key(&path))
            .ok_or_else(|| AppError::user("games.requirement_unknown").with("path", &path))?;
        let url = requirement.url.as_deref().ok_or_else(|| {
            AppError::user("games.requirement_page_missing").with("name", &requirement.name)
        })?;
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(AppError::unexpected)
    })
    .await
}

/// Reads a game folder and suggests a name, id, programs, markers, loader and store ids for the editor. Read-only.
#[tauri::command]
#[specta::specta]
pub async fn inspect_game_folder(path: String) -> Result<editor::FolderInspection, AppError> {
    blocking(move || editor::inspect_folder(&path)).await
}

/// The loader presets the editor offers, including the user's own files under `definitions/loaders/`.
#[tauri::command]
#[specta::specta]
pub async fn loader_presets() -> Result<Vec<editor::LoaderPresetView>, AppError> {
    blocking(|| Ok(editor::loader_presets())).await
}

/// The definition text of a game, for the editor. A user definition wins over a bundled one with the same id.
#[tauri::command]
#[specta::specta]
pub async fn definition_source(id: String) -> Result<editor::DefinitionSource, AppError> {
    blocking(move || editor::source(&id)).await
}

/// Checks a draft without writing anything: parses it, merges its loader and validates the result.
#[tauri::command]
#[specta::specta]
pub async fn validate_definition(text: String) -> Result<editor::DefinitionCheck, AppError> {
    blocking(move || Ok(editor::check(&text))).await
}

/// Writes a valid draft under `<data>/definitions/games/<id>/`. A bundled id is refused; `overwrite` replaces a user
/// definition with the same id.
#[tauri::command]
#[specta::specta]
pub async fn save_definition(
    text: String,
    overwrite: bool,
) -> Result<editor::SavedDefinition, AppError> {
    blocking(move || editor::save(&text, overwrite)).await
}

/// Reads a definition package for the import dialog. A mod archive is reported as `games.package_empty`, so a dropped
/// mod can fall back to the normal import.
#[tauri::command]
#[specta::specta]
pub async fn inspect_definition_package(
    path: String,
) -> Result<editor::DefinitionPackagePreview, AppError> {
    blocking(move || editor::inspect_package(std::path::Path::new(&path))).await
}

/// Writes a valid definition package into the user definitions. An existing user game or loader needs `overwrite`.
#[tauri::command]
#[specta::specta]
pub async fn import_definition_package(
    path: String,
    overwrite: bool,
) -> Result<editor::ImportedDefinitionPackage, AppError> {
    blocking(move || editor::import_package(std::path::Path::new(&path), overwrite)).await
}

/// Looks for supported games in the Steam libraries and the other stores on this computer. Read-only; nothing changes.
#[tauri::command]
#[specta::specta]
pub async fn scan_games() -> Result<Vec<detect::GameScan>, AppError> {
    blocking(|| {
        let (definitions, _) = games::definitions()?;
        let definitions: Vec<_> = definitions.into_iter().map(|(game, _)| game).collect();
        Ok(detect::scan(
            &crate::games::paths::home_dir()?,
            &definitions,
        ))
    })
    .await
}

/// Compares the official catalog with the definitions on this computer: what changed, what is available and what needs
/// a newer app. Nothing is written; a network failure is a retryable error the window may ignore.
#[tauri::command]
#[specta::specta]
pub async fn catalog_check() -> Result<games::CatalogStatus, AppError> {
    blocking(games::check_catalog).await
}

/// Downloads the selected definitions from the official catalog into the data folder. Progress goes through the channel;
/// each id reports its own result so one bad definition does not stop the others.
#[tauri::command]
#[specta::specta]
pub async fn catalog_install(
    ids: Vec<String>,
    progress: Channel<games::CatalogInstallStep>,
) -> Result<Vec<games::CatalogInstallOutcome>, AppError> {
    blocking(move || {
        let mut send = crate::commands::channel_sink(progress);
        games::install_catalog(&ids, &mut send)
    })
    .await
}
