//! Opening a pack and reading what it holds: the manifest, the mods that are missing, and the preview the window shows.

use super::model::ARCHIVES;
use super::model::MANIFEST;
use super::model::Manifest;
use super::model::PackInfo;
use super::model::PackMissing;
use super::model::PackPreview;
use super::model::PackProfile;
use super::model::PackUpdate;
use super::model::SUPPORTED_FORMATS;
use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::domain::library::LibraryView;
use crate::domain::pack::PackMod;
use crate::domain::profiles::Profile;
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::error::io_at;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;

/// A manifest is state, not an archive: a huge `pack.json` is a broken or hostile pack, not a slow read.
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct Opened {
    pub(super) manifest: Manifest,
    pub(super) zip: zip::ZipArchive<fs::File>,
    pub(super) archives: BTreeMap<String, String>,
}

pub(super) fn open_pack(game_id: &str, path: &Path) -> Result<Opened, AppError> {
    let file = fs::File::open(path).map_err(io_at(path))?;
    let mut zip = zip::ZipArchive::new(file).map_err(pack_error)?;
    let manifest: Manifest = {
        let entry = zip
            .by_name(MANIFEST)
            .map_err(|_| AppError::user("pack.no_manifest"))?;
        if entry.size() > MAX_MANIFEST_BYTES {
            return Err(AppError::user("pack.invalid").with("detail", "the manifest is too large"));
        }

        let mut text = String::new();
        entry
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(io_at(path))?;
        if text.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(AppError::user("pack.invalid").with("detail", "the manifest is too large"));
        }

        serde_json::from_str(&text).map_err(|e| AppError::user("pack.invalid").with("detail", e))?
    };

    if !SUPPORTED_FORMATS.contains(&manifest.format) {
        return Err(AppError::user("pack.format").with("format", manifest.format));
    }

    if manifest.game != game_id {
        return Err(AppError::user("pack.other_game").with("game", &manifest.game));
    }

    let archives = (0..zip.len())
        .filter_map(|index| {
            let name = zip.name_for_index(index)?.ok()?.to_string();
            let (sha256, file) = name.strip_prefix(ARCHIVES)?.split_once('/')?;
            let plain = !file.is_empty() && !file.contains('/');
            plain.then(|| (sha256.to_string(), name.clone()))
        })
        .collect();
    Ok(Opened {
        manifest,
        zip,
        archives,
    })
}

pub(super) fn missing(entry: &PackMod) -> PackMissing {
    PackMissing {
        name: entry.name.clone(),
        version: entry.version.clone(),
        source: entry.archive_source(),
        archive: entry.archive.as_ref().map(|archive| archive.name.clone()),
    }
}

/// The profile an earlier version of this pack made from its profile `name`.
pub(super) fn made_from<'a>(
    profiles: &'a Profiles,
    info: &PackInfo,
    name: &str,
) -> Option<&'a Profile> {
    profiles.profiles.iter().find(|profile| {
        profile
            .pack
            .as_ref()
            .is_some_and(|origin| origin.id == info.id && origin.profile == name)
    })
}

/// Reads a pack and says what an import would do. Nothing changes.
pub fn inspect_pack(root: &Data, game_id: &str, path: &Path) -> Result<PackPreview, AppError> {
    let Loaded {
        library, profiles, ..
    } = load_game(root, game_id)?;
    let view = LibraryView::new(&library);
    let opened = open_pack(game_id, path)?;
    let manifest = &opened.manifest;

    let mut seen = BTreeSet::new();
    let (mut total, mut have, mut bundled) = (0, 0, 0);
    let mut missing_mods = Vec::new();
    let mut updates = Vec::new();

    for profile in &manifest.profiles {
        for entry in &profile.mods {
            if !seen.insert(entry.family.as_str()) {
                continue;
            }

            total += 1;

            if view.find_entry(entry).is_some() {
                have += 1;
            } else if entry
                .archive
                .as_ref()
                .is_some_and(|archive| opened.archives.contains_key(&archive.sha256))
            {
                bundled += 1;
            } else {
                missing_mods.push(missing(entry));
            }
        }

        if let Some(existing) = made_from(&profiles, &manifest.info, &profile.name) {
            updates.push(pack_update(&view, existing, profile));
        }
    }

    Ok(PackPreview {
        info: manifest.info.clone(),
        profiles: manifest
            .profiles
            .iter()
            .map(|profile| profile.name.clone())
            .collect(),
        total,
        have,
        bundled,
        missing: missing_mods,
        updates,
    })
}

/// How a profile changes when the pack's version of it replaces it.
pub(super) fn pack_update(
    view: &LibraryView,
    existing: &Profile,
    profile: &PackProfile,
) -> PackUpdate {
    let resolved: Vec<(&PackMod, Option<(String, String)>)> = profile
        .mods
        .iter()
        .map(|entry| (entry, view.find_entry(entry)))
        .collect();
    let name_of = |id: &str| {
        view.family_name(id)
            .map_or_else(|| id.to_string(), str::to_string)
    };
    // The existing list is indexed once. A scan of it for every pack mod made this quadratic.
    let mut existing_by_family: rustc_hash::FxHashMap<&str, &crate::domain::profiles::ListedMod> =
        rustc_hash::FxHashMap::default();
    for entry in &existing.mods {
        existing_by_family.entry(entry.id.as_str()).or_insert(entry);
    }

    let mut update = PackUpdate {
        profile: existing.name.clone(),
        from: existing
            .pack
            .as_ref()
            .map(|origin| origin.version.clone())
            .unwrap_or_default(),
        added: Vec::new(),
        removed: Vec::new(),
        updated: Vec::new(),
    };

    for (entry, found) in &resolved {
        let old = found
            .as_ref()
            .and_then(|(family, _)| existing_by_family.get(family.as_str()).copied());
        match (found, old) {
            (Some((_, folder)), Some(old)) if old.folder != *folder => {
                update.updated.push(entry.name.clone());
            }
            (_, None) => update.added.push(entry.name.clone()),
            _ => {}
        }
    }

    let kept: BTreeSet<&str> = resolved
        .iter()
        .filter_map(|(_, found)| found.as_ref().map(|(family, _)| family.as_str()))
        .collect();
    update.removed = existing
        .mods
        .iter()
        .filter(|entry| !kept.contains(entry.id.as_str()))
        .map(|entry| name_of(&entry.id))
        .collect();
    update
}

/// A library and pack entries that resolve to it, for `benches/backend.rs`. Compiled only with the `bench` feature.
#[cfg(feature = "bench")]
#[doc(hidden)]
pub mod bench {
    use super::{LibraryView, PackMod};
    use crate::domain::library::{Family, Library, ModInfo, ModInfoEdit, ModVersion};
    use crate::domain::source::{ProviderId, SourceMod};
    use std::sync::Arc;

    pub struct PackFixture {
        library: Library,
        entries: Vec<PackMod>,
    }

    impl PackFixture {
        /// `families` families with `versions` installed versions each, and `entries` pack mods that name them by
        /// Nexus id, as a pack from another library does.
        pub fn new(families: usize, versions: usize, entries: usize) -> Self {
            let mut library = Library::default();

            for family in 0..families {
                let id = format!("nexus:{family}");
                let nexus = u32::try_from(family).unwrap_or(u32::MAX) + 1;

                for version in 0..versions {
                    let folder = format!("mod{family}_{version}");
                    library.versions.insert(
                        folder.clone(),
                        ModVersion {
                            folder,
                            family: id.clone(),
                            slot: "Mods".into(),
                            files: Arc::from(Vec::new()),
                            size_bytes: 0,
                            info: ModInfo {
                                name: format!("Mod {family}"),
                                version: Some(format!("1.{version}")),
                                ..ModInfo::default()
                            },
                            edit: ModInfoEdit::default(),
                            archive: None,
                            parsed_sources: std::collections::BTreeMap::from([(
                                ProviderId::new("nexus"),
                                nexus.to_string(),
                            )]),
                            parsed_parts: std::collections::BTreeMap::new(),
                            installed_at: version as u32,
                            fomod: Vec::new(),
                            fomod_installer: false,
                            integrity: None,
                        },
                    );
                }

                library.families.insert(
                    id.clone(),
                    Family {
                        id,
                        name: format!("Mod {family}"),
                        ..Family::default()
                    },
                );
            }

            let entries = (0..entries)
                .map(|entry| {
                    let family = entry % families;
                    PackMod {
                        name: format!("Mod {family}"),
                        version: None,
                        family: format!("nexus:{family}"),
                        enabled: true,
                        priority: None,
                        sources: std::collections::BTreeMap::from([(
                            ProviderId::new("nexus"),
                            SourceMod {
                                mod_id: (u32::try_from(family).unwrap_or(u32::MAX) + 1).to_string(),
                                part_id: None,
                                part_name: None,
                                part_category: None,
                                meta: None,
                            },
                        )]),
                        source_files: std::collections::BTreeMap::new(),
                        source_parts: std::collections::BTreeMap::new(),
                        part_name: None,
                        archive: None,
                        fomod: Vec::new(),
                        fomod_installer: false,
                        tags: Vec::new(),
                        instructions: String::new(),
                        requires: Vec::new(),
                        requirements: Vec::new(),
                        source_dlcs: std::collections::BTreeMap::new(),
                        notes: String::new(),
                        incompatible: Vec::new(),
                        overrides: Vec::new(),
                        steps: None,
                    }
                })
                .collect();
            Self { library, entries }
        }

        /// Resolves every entry, as an import of the pack does.
        pub fn resolve(&self) -> usize {
            let view = LibraryView::new(&self.library);
            self.entries
                .iter()
                .filter(|entry| view.find_entry(entry).is_some())
                .count()
        }
    }
}

/// Imports a pack: the archives inside it that the library lacks go in first, each with its FOMOD choices. Then every
/// profile of the pack is made, or updated when an earlier version of the pack made it.
pub(super) fn pack_error(error: zip::result::ZipError) -> AppError {
    AppError::new("pack.failed")
        .with("detail", error)
        .kind(ErrorKind::Permanent)
}
