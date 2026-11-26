//! Importing an r2modman or Gale profile: its `mods.yml` names Thunderstore packages and versions. The import matches
//! them against the library, lists the packages that are missing, and creates a normal Gobbo profile with the matched
//! mods on or off as the file says. Config files are not copied; the preview says whether the folder has them.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::context::Data;
use crate::context::load_library;
use crate::domain::library::view::FindQuery;
use crate::domain::library::{Library, LibraryView};
use crate::domain::profiles::Row;
use crate::domain::source::ProviderId;
use crate::error::{AppError, io_at};
use crate::games;
use crate::profiles::actions;
use crate::providers::thunderstore;
use crate::providers::thunderstore::community_profile::{CommunityMod, parse_mods};
use crate::status::views::Status;

/// The largest `mods.yml` the import reads. A profile file is a few kilobytes; a larger one is not a profile.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Where a package stands in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CommunityMatch {
    /// The exact version is installed.
    Matched,
    /// The package is installed, but the profile file names another version. The library's default version is selected.
    OtherVersion,
    /// No family of the library has this Thunderstore package.
    Missing,
}

/// One package of a community profile, with how it stands in the library and the version a profile would select.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CommunityProfileMod {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub state: CommunityMatch,
    pub family: Option<String>,
    /// The version folder a normal profile would select.
    pub folder: Option<String>,
    /// The version that folder holds, when the library knows it.
    pub selected_version: Option<String>,
}

/// What a community profile file holds before it is imported, with its packages and config folder.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CommunityProfilePreview {
    /// The name taken from the profile folder, before the user edits it.
    pub profile: String,
    pub mods: Vec<CommunityProfileMod>,
    pub missing: u32,
    pub other_versions: u32,
    /// The profile folder has the loader's config mirror, or the bare `config` folder. Those files are not part of the
    /// import.
    pub has_configs: bool,
}

/// The profile made from a community profile file and the packages that could not be matched.
#[derive(Debug, Serialize, specta::Type)]
pub struct CommunityProfileImport {
    pub status: Status,
    pub profile: String,
    pub mods: u32,
    pub missing: Vec<String>,
    pub other_versions: Vec<String>,
}

/// Reads a profile folder or its `mods.yml` file and matches every package against the game's library.
pub fn inspect(
    root: &Data,
    game_id: &str,
    path: &str,
) -> Result<CommunityProfilePreview, AppError> {
    let game = games::load(game_id)?;
    let config_mirror = game
        .resolved_loader
        .as_ref()
        .and_then(|loader| loader.config_mirror.as_deref());
    let (profile, mods, has_configs) = read_profile(Path::new(path), config_mirror)?;
    let library = load_library(&root.game(game_id), &game)?;
    let resolved = resolve(&library, &mods);
    Ok(preview_of(profile, resolved, has_configs))
}

/// Creates a new profile from the file. The previous profile stays untouched; the new one becomes active.
pub fn import(
    root: &Data,
    game_id: &str,
    path: &str,
    name: &str,
) -> Result<CommunityProfileImport, AppError> {
    let preview = inspect(root, game_id, path)?;
    let name = if name.trim().is_empty() {
        preview.profile.trim()
    } else {
        name.trim()
    };
    let rows: Vec<Row> = preview
        .mods
        .iter()
        .filter_map(|mod_view| {
            Some(Row {
                folder: mod_view.folder.clone()?,
                enabled: mod_view.enabled,
                separator: None,
            })
        })
        .collect();
    if rows.is_empty() {
        return Err(AppError::user("profiles.community.no_mods"));
    }

    let profile = crate::context::edit_profiles(root, game_id, |_, game, profiles| {
        let id = profiles.create_unique(name)?;
        profiles.profile_mut(&id)?.user_data = game.user_data_defaults();
        profiles.switch(&id)?;
        Ok(id)
    })?;

    if let Err(error) = actions::save_profile(root, game_id, &rows) {
        // The profile was created but has no list; remove it rather than leave an empty one behind.
        if let Err(cleanup) = actions::delete_profile(root, game_id, &profile) {
            crate::infra::log::warn(format!(
                "could not remove the empty profile {profile} after a failed save: {cleanup}"
            ));
        }

        return Err(error);
    }

    let missing = preview
        .mods
        .iter()
        .filter(|mod_view| mod_view.state == CommunityMatch::Missing)
        .map(|mod_view| mod_view.name.clone())
        .collect();
    let other_versions = preview
        .mods
        .iter()
        .filter(|mod_view| mod_view.state == CommunityMatch::OtherVersion)
        .map(|mod_view| mod_view.name.clone())
        .collect();
    Ok(CommunityProfileImport {
        status: crate::status::status(root, game_id)?,
        profile,
        mods: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        missing,
        other_versions,
    })
}

fn read_profile(
    path: &Path,
    config_mirror: Option<&str>,
) -> Result<(String, Vec<CommunityMod>, bool), AppError> {
    let (dir, file, profile) = if path.is_dir() {
        let mods = ["mods.yml", "mods.yaml"]
            .iter()
            .map(|name| path.join(name))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| AppError::user("profiles.community.file_missing"))?;
        let name = path.file_name().map_or_else(
            || "Imported profile".to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        (path.to_path_buf(), mods, name)
    } else {
        let name = path.parent().and_then(Path::file_name).map_or_else(
            || "Imported profile".to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        (
            path.parent().unwrap_or(path).to_path_buf(),
            path.to_path_buf(),
            name,
        )
    };
    let size = fs::metadata(&file).map_err(io_at(&file))?.len();

    if size > MAX_BYTES {
        return Err(AppError::user("profiles.community.too_large").with("path", file.display()));
    }

    let text = fs::read_to_string(&file).map_err(io_at(&file))?;
    let mods = parse_mods(&text);

    if mods.is_empty() {
        return Err(AppError::user("profiles.community.empty").with("path", file.display()));
    }

    // The loader owns the nested folder; the bare `config` folder is the convention an unpacked profile may use with
    // any loader.
    let has_configs = config_mirror.is_some_and(|folder| dir.join(folder).is_dir())
        || dir.join("config").is_dir();
    Ok((profile, mods, has_configs))
}

/// Matches the file's packages against the library. `item.name` is the Thunderstore package slug (`owner-name`), the
/// provider's mod id, not a display name: the shared matcher compares it with the family's provider id, and both the id
/// and the version go through the app's one case fold. A package in another version keeps the family's default version.
fn resolve(library: &Library, mods: &[CommunityMod]) -> Vec<CommunityProfileMod> {
    let provider = ProviderId::new(thunderstore::ID);
    let view = LibraryView::new(library);
    mods.iter()
        .map(|item| {
            let found = view.find_with(
                FindQuery::folded(&provider, &item.name, &item.version),
                None,
            );
            let Some(found) = found else {
                return CommunityProfileMod {
                    name: item.name.clone(),
                    version: item.version.clone(),
                    enabled: item.enabled,
                    state: CommunityMatch::Missing,
                    family: None,
                    folder: None,
                    selected_version: None,
                };
            };
            let selected = found.version.or_else(|| view.default_version(found.family));
            let state = if found.version.is_some() {
                CommunityMatch::Matched
            } else {
                CommunityMatch::OtherVersion
            };
            CommunityProfileMod {
                name: item.name.clone(),
                version: item.version.clone(),
                enabled: item.enabled,
                state,
                family: Some(found.family.to_string()),
                folder: selected.map(|version| version.folder.clone()),
                selected_version: selected.and_then(|version| version.info.version.clone()),
            }
        })
        .collect()
}

fn preview_of(
    profile: String,
    mods: Vec<CommunityProfileMod>,
    has_configs: bool,
) -> CommunityProfilePreview {
    let missing = mods
        .iter()
        .filter(|item| item.state == CommunityMatch::Missing)
        .count();
    let other_versions = mods
        .iter()
        .filter(|item| item.state == CommunityMatch::OtherVersion)
        .count();
    CommunityProfilePreview {
        profile,
        mods,
        missing: u32::try_from(missing).unwrap_or(u32::MAX),
        other_versions: u32::try_from(other_versions).unwrap_or(u32::MAX),
        has_configs,
    }
}
