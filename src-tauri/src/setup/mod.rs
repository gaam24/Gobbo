//! Loader packages: the verified release of a mod loader. Installing one imports the release into the
//! library as one mod whose files keep their game paths and enables it, so the loader reaches the game through the
//! overlay like every other mod. Nothing is written into the game folder.
//!
//! The package is data: a loader preset (or a game) names the HTTPS archive, its SHA-256, and the folder its root stands
//! for. The code never branches on a loader id.

mod install;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::context::{Data, Settings};
use crate::domain::library::{Library, ModVersion};
use crate::domain::pathkey::path_key;
use crate::domain::profiles::Profile;
use crate::games::{GameDef, Install};

pub use install::install;

/// What the setup UI needs to know about the loader package of a game.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct LoaderSetupView {
    pub name: String,
    /// The version the definition offers.
    pub version: String,
    /// The version of the enabled copy, or `None` when it has none (installed by hand, or not installed).
    pub installed_version: Option<String>,
    pub state: LoaderSetupState,
    /// The archive link.
    pub source: String,
    pub sha256: String,
    /// The expected download size in bytes, or `0` when the definition does not name one. A loader release is far
    /// below the 4 GiB that fit in the IPC type.
    pub size: u32,
    /// The deployment-root-relative folder the release's files are served in. Empty means the deployment root.
    pub target: String,
    /// Every copy of the loader in the library, the official release and builds from mod sites alike. The user picks
    /// which one the active profile uses.
    pub copies: Vec<LoaderCopyView>,
}

/// One copy of the loader in the library: a mod that ships every check file.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct LoaderCopyView {
    /// The library folder, which `install_loader` takes to pick this copy.
    pub folder: String,
    pub name: String,
    pub version: Option<String>,
    /// Unpacked from the exact release the definition names (same SHA-256).
    pub official: bool,
    /// Enabled in the active profile.
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LoaderSetupState {
    Missing,
    Installed,
    /// The enabled copy is an older official release. A build from a mod site is never reported as outdated: it is
    /// the user's choice, and the official release stays one click away.
    Update,
}

/// Which copy of the loader `install_loader` turns on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoaderChoice {
    /// The quick action: a copy already in the library (the official release first), else the official download.
    Auto,
    /// The release the definition names: its copy in the library, or the verified download.
    Official,
    /// One library copy, for example a build a mod site publishes for this game.
    Library { folder: String },
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
    let game_data = data.game(&game.id);
    let library = crate::context::load_library_shared(&game_data, game)?;
    let profiles = crate::context::data::load_profiles_shared(&game_data)?;
    let root = crate::context::target::deployment_root(data, game, settings)?;
    let copies = copies(&library, profiles.active(), install);
    let manual = install.check.iter().all(|path| root.join(path).exists());
    let active = copies.iter().find(|copy| copy.enabled);
    let installed_version = active.and_then(|copy| copy.version.clone());
    let state = match active {
        Some(copy) if is_older_official(&library, copy, install) => LoaderSetupState::Update,
        Some(_) => LoaderSetupState::Installed,
        None if manual => LoaderSetupState::Installed,
        None => LoaderSetupState::Missing,
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
        copies,
    }))
}

/// Whether a library version ships every check file of the package.
fn is_loader_copy(version: &ModVersion, install: &Install) -> bool {
    install.check.iter().all(|check| {
        let check = path_key(check);
        version.files.iter().any(|file| path_key(file) == check)
    })
}

/// Whether a version was unpacked from the exact release the definition names.
fn is_official(version: &ModVersion, install: &Install) -> bool {
    version
        .archive
        .as_deref()
        .is_some_and(|archive| archive.eq_ignore_ascii_case(&install.sha256))
}

/// An enabled copy that the official install put there for an older definition: no mod site is its source and its
/// version is not the one the definition names now. A copy from a mod site is the user's pick and is never "outdated".
fn is_older_official(library: &Library, copy: &LoaderCopyView, install: &Install) -> bool {
    if copy.official {
        return false;
    }

    let from_site = library
        .versions
        .get(&copy.folder)
        .and_then(|version| library.families.get(&version.family))
        .is_some_and(|family| !family.sources.is_empty());

    !from_site
        && copy.name == install.name
        && copy
            .version
            .as_deref()
            .is_some_and(|version| version != install.version)
}

/// The library copies of the loader, the enabled one first, then the official release, then the newest import.
fn copies(library: &Library, profile: &Profile, install: &Install) -> Vec<LoaderCopyView> {
    let enabled: BTreeSet<&str> = profile
        .mods
        .iter()
        .filter(|entry| entry.enabled)
        .map(|entry| entry.folder.as_str())
        .collect();
    let mut found: Vec<(&ModVersion, LoaderCopyView)> = library
        .versions
        .values()
        .filter(|version| is_loader_copy(version, install))
        .map(|version| {
            let view = LoaderCopyView {
                folder: version.folder.clone(),
                name: version.info.name.clone(),
                version: version.info.version.clone(),
                official: is_official(version, install),
                enabled: enabled.contains(version.folder.as_str()),
            };
            (version, view)
        })
        .collect();
    found.sort_by(|(a, a_view), (b, b_view)| {
        b_view
            .enabled
            .cmp(&a_view.enabled)
            .then(b_view.official.cmp(&a_view.official))
            .then(b.installed_at.cmp(&a.installed_at))
            .then(a.folder.cmp(&b.folder))
    });
    found.into_iter().map(|(_, view)| view).collect()
}
