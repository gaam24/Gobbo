//! Loader packages: the verified release of a mod loader. Installing one imports the release into the
//! library as one mod whose files keep their game paths and enables it, so the loader reaches the game through the
//! overlay like every other mod. Nothing is written into the game folder.
//!
//! The package is data: a loader preset (or a game) names the HTTPS archive, its SHA-256, and the folder its root stands
//! for. The code never branches on a loader id.

mod install;

#[cfg(test)]
mod tests;

use serde::Serialize;

use crate::context::{Data, Settings};
use crate::domain::library::{Library, ModVersion};
use crate::domain::pathkey::path_key;
use crate::games::{GameDef, Install};

pub use install::install;

/// What the setup UI needs to know about the loader package of a game.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct LoaderSetupView {
    pub name: String,
    /// The version the definition offers.
    pub version: String,
    /// The version of the copy in the library, or `None` when it has none (installed by hand, or not installed).
    pub installed_version: Option<String>,
    pub state: LoaderSetupState,
    /// The archive link.
    pub source: String,
    pub sha256: String,
    /// The expected download size in bytes, or `0` when the definition does not name one. A loader release is far
    /// below the 4 GiB that fit in the IPC type.
    pub size: u32,
    /// The deployment-root-relative folder the release's files are served in.
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LoaderSetupState {
    Missing,
    Installed,
    Update,
}

/// The loader package a game can install, in the form the setup dialog shows. `None` when the game names no package.
pub fn view(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
) -> Result<Option<LoaderSetupView>, crate::error::AppError> {
    let Some(install) = &game.install else {
        return Ok(None);
    };
    let library = crate::context::load_library_shared(&data.game(&game.id), game)?;
    let root = crate::context::target::deployment_root(data, game, settings)?;
    let copy = library_copy(&library, install);
    let manual = install.check.iter().all(|path| root.join(path).exists());
    let installed_version = copy.and_then(|copy| copy.info.version.clone());
    let state = match (copy, &installed_version) {
        (Some(_), Some(version)) if *version != install.version => LoaderSetupState::Update,
        (Some(_), _) => LoaderSetupState::Installed,
        (None, _) if manual => LoaderSetupState::Installed,
        (None, _) => LoaderSetupState::Missing,
    };

    Ok(Some(LoaderSetupView {
        name: install.name.clone(),
        version: install.version.clone(),
        installed_version,
        state,
        source: install.url.clone(),
        sha256: install.sha256.clone(),
        size: install
            .size
            .map_or(0, |size| u32::try_from(size).unwrap_or(u32::MAX)),
        target: install.to.clone(),
    }))
}

/// The library copy of the loader: a mod whose files include every check path. The definition's version wins when
/// several copies are there.
fn library_copy<'a>(library: &'a Library, install: &Install) -> Option<&'a ModVersion> {
    let checks: Vec<String> = install.check.iter().map(|path| path_key(path)).collect();
    let mut copies = library.versions.values().filter(|version| {
        checks
            .iter()
            .all(|check| version.files.iter().any(|file| path_key(file) == *check))
    });
    let first = copies.next()?;

    if first.info.version.as_deref() == Some(install.version.as_str()) {
        return Some(first);
    }

    Some(
        copies
            .find(|copy| copy.info.version.as_deref() == Some(install.version.as_str()))
            .unwrap_or(first),
    )
}
