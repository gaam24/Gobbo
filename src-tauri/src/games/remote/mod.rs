//! The official catalog: read `catalog.json` from GitHub, compare it with the definitions on this computer and install
//! the ones the user picks. The manifest is fetched afresh each time; nothing here caches it or writes to the process-wide
//! definition folders except the installed files themselves.
//!
//! `manifest.rs` fetches, parses and gates the file; `install.rs` downloads, validates and places the definitions.

mod install;
mod manifest;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use crate::error::{AppError, io_at};
use crate::infra::store;

/// The address of the generated catalog manifest.
const CATALOG_URL: &str = "https://raw.githubusercontent.com/gaam24/Gobbo/main/catalog.json";
/// The address catalog entries join their relative `file` and `icon` paths onto.
const BASE_URL: &str = "https://raw.githubusercontent.com/gaam24/Gobbo/main/";
/// The file under `<data>/catalog` that names the app version which last wrote the official copies.
const VERSION_FILE: &str = "app-version";

/// The part of a manifest entry the Games page draws. `min_gobbo` is the higher floor of the game and its loader, so a
/// blocked entry can say which app version it needs.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CatalogEntryView {
    /// The game id the window sends back to install.
    pub id: String,
    /// The display name of the game.
    pub name: String,
    /// The loader preset id, when the game names one.
    pub loader: Option<String>,
    /// Whether a real installation was tested.
    pub verified: bool,
    /// The oldest Gobbo release that serves this game and its loader correctly.
    pub min_gobbo: Option<String>,
}

/// The catalog compared with the local definitions. User-owned ids appear in none of the lists: their files are theirs
/// to change, and the catalog never offers to overwrite them.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CatalogStatus {
    /// Local definitions whose checksum differs from the catalog.
    pub changed: Vec<CatalogEntryView>,
    /// Ids that exist in no local definition folder.
    pub available: Vec<CatalogEntryView>,
    /// Ids that need a newer app: a newer schema, or a `min_gobbo` above this version on the game or its loader.
    pub blocked: Vec<CatalogEntryView>,
}

/// The steps of one catalog install, in order, as the window sees them.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "step", rename_all = "lowercase")]
pub enum CatalogInstallStep {
    /// A definition, icon or loader file is downloading. `percent` is unknown when the server sends no length.
    Downloading {
        /// The share of the file received, from 0 to 100.
        percent: Option<u8>,
    },
    /// The files passed validation and are being moved into place.
    Installing,
}

/// The result of installing one id. One bad id does not stop the others, so each carries its own answer.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CatalogInstallOutcome {
    /// The id the window asked for.
    pub id: String,
    /// Whether the definition was installed, and if not, why.
    pub result: CatalogInstallResult,
    /// A note about a skipped part, such as a loader the manifest does not list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Whether one id was installed or why it was refused.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CatalogInstallResult {
    /// The files were placed in the catalog folders.
    Installed,
    /// The id was refused; the error has a stable code the window translates.
    Failed {
        /// Why this id was not installed.
        error: AppError,
    },
}

/// The definition folders an install reads and writes. Held together so a test can pass scratch folders instead of the
/// process-wide data root, and so the loader precedence is named in one place.
pub(super) struct Folders {
    /// `<data>/catalog/games`.
    pub(super) catalog_games: Option<PathBuf>,
    /// `<data>/definitions/loaders`.
    pub(super) user_loaders: Option<PathBuf>,
    /// `<data>/catalog/loaders`.
    pub(super) catalog_loaders: Option<PathBuf>,
    /// The bundled `loaders/` folder.
    pub(super) bundled_loaders: PathBuf,
}

/// Compares the official catalog with the definitions on this computer. Nothing is written; a network failure is a
/// retryable error the window may show or ignore.
pub fn check_catalog() -> Result<CatalogStatus, AppError> {
    check_from(
        CATALOG_URL,
        super::catalog_games_dir().as_deref(),
        super::user_games_dir().as_deref(),
        super::games_dir(),
    )
}

/// The check against explicit folders and a manifest address. A caller that owns the folders reads the same precedence
/// without depending on the process-wide data root.
pub(super) fn check_from(
    manifest_url: &str,
    catalog_games: Option<&Path>,
    user_games: Option<&Path>,
    bundled_games: &Path,
) -> Result<CatalogStatus, AppError> {
    let catalog = manifest::fetch_catalog(manifest_url)?;
    manifest::classify(&catalog, catalog_games, user_games, bundled_games)
}

/// Downloads the selected ids from the official catalog into the data folder, reporting progress on `progress`.
pub fn install_catalog(
    ids: &[String],
    progress: &mut dyn FnMut(CatalogInstallStep),
) -> Result<Vec<CatalogInstallOutcome>, AppError> {
    install_from(
        CATALOG_URL,
        BASE_URL,
        &Folders {
            catalog_games: super::catalog_games_dir(),
            user_loaders: super::user_loaders_dir(),
            catalog_loaders: super::catalog_loaders_dir(),
            bundled_loaders: super::loaders_dir().to_path_buf(),
        },
        ids,
        progress,
    )
}

/// The install against explicit folders and addresses, for a caller that owns the definition folders.
pub(super) fn install_from(
    manifest_url: &str,
    base_url: &str,
    folders: &Folders,
    ids: &[String],
    progress: &mut dyn FnMut(CatalogInstallStep),
) -> Result<Vec<CatalogInstallOutcome>, AppError> {
    install::install(manifest_url, base_url, folders, ids, progress)
}

/// Records which app version wrote the official copies, so the next start after an update can drop the stale ones.
pub(super) fn mark_app_version(catalog_games: &Path) -> Result<(), AppError> {
    let root = catalog_games.parent().unwrap_or(catalog_games);
    store::write_atomic(
        &root.join(VERSION_FILE),
        env!("CARGO_PKG_VERSION").as_bytes(),
    )?;
    Ok(())
}

/// Drops the official copies that shadow a bundled file once the app version changed. A new release ships definitions
/// at least as new as the ones it was built with, and a downloaded copy would otherwise hide them forever. A copy with
/// no bundled counterpart stays; the next catalog check offers whatever the repository has moved on to since.
pub fn drop_stale_catalog() {
    let (Some(games), Some(loaders)) = (super::catalog_games_dir(), super::catalog_loaders_dir())
    else {
        return;
    };

    match drop_stale_from(
        &games,
        &loaders,
        super::games_dir(),
        super::loaders_dir(),
        env!("CARGO_PKG_VERSION"),
    ) {
        Ok(0) => {}
        Ok(count) => crate::infra::log::info(format!(
            "dropped {count} downloaded definitions that the updated app ships itself"
        )),
        Err(error) => crate::infra::log::warn(format!(
            "could not drop stale downloaded definitions: {error}"
        )),
    }
}

/// The drop against explicit folders and an explicit app version. Returns how many games and loaders were removed.
pub(super) fn drop_stale_from(
    catalog_games: &Path,
    catalog_loaders: &Path,
    bundled_games: &Path,
    bundled_loaders: &Path,
    version: &str,
) -> Result<usize, AppError> {
    let marker = catalog_games
        .parent()
        .unwrap_or(catalog_games)
        .join(VERSION_FILE);
    let written_by = match std::fs::read_to_string(&marker) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(io_at(&marker)(error)),
    };

    if written_by.trim() == version {
        return Ok(0);
    }

    let mut dropped = 0;

    for name in entry_names(catalog_games)? {
        if bundled_games.join(&name).join("game.toml").is_file() {
            let dir = catalog_games.join(&name);
            std::fs::remove_dir_all(&dir).map_err(io_at(&dir))?;
            dropped += 1;
        }
    }

    for name in entry_names(catalog_loaders)? {
        if bundled_loaders.join(&name).is_file() {
            let file = catalog_loaders.join(&name);
            std::fs::remove_file(&file).map_err(io_at(&file))?;
            dropped += 1;
        }
    }

    store::write_atomic(&marker, version.as_bytes())?;
    Ok(dropped)
}

/// The entry names in `dir`, or none when the folder does not exist yet.
fn entry_names(dir: &Path) -> Result<Vec<std::ffi::OsString>, AppError> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|entry| entry.map(|entry| entry.file_name()).map_err(io_at(dir)))
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(io_at(dir)(error)),
    }
}
