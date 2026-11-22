//! The library index: reading and writing `library.json`, scanning the folders and keeping the families up to date.

use crate::error::{AppError, ErrorKind, io_at};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use super::lock::{HOLDS_INDEX, lock_index};
use crate::domain::library::{ArchiveLabel, Family, FolderCheck, Library, ModVersion};
use crate::domain::profiles::entry::ProfileMod;
use crate::domain::resolver::ModInput;
use crate::domain::source::{ProviderId, SourceRef};
use crate::games::GameDef;
use crate::infra::json;

mod scan;

/// Reads the name and version a provider gives an archive file name. It gets the provider ids of the versions that use
/// the archive, then the archive's own source ids. The caller supplies it (`providers::archive_name_from`), so the index
/// does not depend on the providers.
pub type ArchiveLabels<'a> = &'a dyn Fn(&[&str], &str) -> Option<ArchiveLabel>;

/// The error a stale list gets when it names a version the family does not have.
fn version_unknown(folder: &str) -> AppError {
    AppError::user("library.version_unknown").with("folder", folder)
}

impl Library {
    /// Reads the index. When it is missing (the first start), scans the folder and saves the scan, unless a change saves
    /// the index first. A file that does not parse is kept as `.json.broken` and rebuilt from the library folders; a
    /// file from a newer Gobbo is refused rather than overwritten.
    pub fn load(index: &Path, library_dir: &Path, game: &GameDef) -> Result<Self, AppError> {
        let mut library: Self = match json::read_json(index) {
            Ok(Some(library)) => library,
            Ok(None) => return Self::rebuild(index, library_dir, game),
            Err(json::ReadError::Io(e)) => return Err(io_at(index)(e)),
            Err(json::ReadError::Parse(error)) => {
                json::keep_broken(index, &error);
                return Self::rebuild(index, library_dir, game);
            }
        };

        if library.schema_version > crate::domain::STATE_SCHEMA {
            return Err(AppError::new("state.schema_newer")
                .with("found", library.schema_version)
                .with("supported", crate::domain::STATE_SCHEMA)
                .kind(ErrorKind::Permanent));
        }

        library.schema_version = crate::domain::STATE_SCHEMA;
        // A slot the definition no longer has goes to the one a rescan would pick, so an edited definition never
        // leaves a mod without a place. The slot names are compared through a set, so a large index does not scan the
        // definition's slot list again for every version.
        let slots: rustc_hash::FxHashSet<&str> = game
            .deploy
            .slots
            .iter()
            .map(|slot| slot.dir.as_str())
            .collect();
        for version in library.versions.values_mut() {
            if !version.slot.is_empty() && !slots.contains(version.slot.as_str()) {
                version.slot = game.slot_for_folder(&version.folder);
            }
        }

        library.fill_layouts(game);
        Ok(library)
    }

    /// Scans the library folders into a new index and saves it, unless a change saved one while the scan ran.
    fn rebuild(index: &Path, library_dir: &Path, game: &GameDef) -> Result<Self, AppError> {
        let mut library = Self::default();
        library.rescan(library_dir, game)?;
        // On a thread that holds the lock, a change is under way and the scan is saved as it is. Otherwise the scan
        // waits for the lock, and it is saved only if no change has saved the index by then.
        if HOLDS_INDEX.get() > 0 {
            library.save(index)?;
        } else {
            let _index = lock_index();

            if !index.exists() {
                library.save(index)?;
            }
        }

        Ok(library)
    }

    /// Writes the index. A change calls it while it holds `lock_index`.
    pub fn save(&self, index: &Path) -> Result<(), AppError> {
        json::write_json(index, self)
    }

    /// Drops the layout entries of versions that are gone. Deletions remove versions from the map; this keeps the
    /// per-version profiles from outliving their mods until the next load.
    pub(crate) fn prune_layouts(&mut self) {
        if self.layouts.len() == self.versions.len() {
            return;
        }

        let versions = &self.versions;
        self.layouts
            .retain(|folder, _| versions.contains_key(folder));
    }

    /// Data for the plan: files from the index, only for enabled versions. Does not touch the disk. The names borrow
    /// the index and the profile, so a plan of a thousand mods allocates no strings here.
    pub fn inputs<'a>(&'a self, mods: &'a [ProfileMod]) -> Vec<ModInput<'a>> {
        // A disabled entry needs no files. One empty list is shared by all of them instead of one allocation per
        // disabled mod.
        let empty_files: std::sync::Arc<[String]> = std::sync::Arc::from(Vec::new());
        mods.iter()
            .map(|entry| {
                let record = self.versions.get(&entry.folder);
                ModInput {
                    id: std::borrow::Cow::Borrowed(
                        record.map_or(entry.folder.as_str(), |record| record.family.as_str()),
                    ),
                    folder: std::borrow::Cow::Borrowed(&entry.folder),
                    slot: std::borrow::Cow::Borrowed(
                        record.map_or(entry.slot.as_str(), |record| record.slot.as_str()),
                    ),
                    display_name: std::borrow::Cow::Borrowed(
                        record
                            .map(|record| record.info.name.as_str())
                            .filter(|name| !name.is_empty())
                            .unwrap_or(&entry.folder),
                    ),
                    enabled: entry.enabled,
                    priority: entry.priority,
                    files: match record {
                        Some(record) if entry.enabled => std::sync::Arc::clone(&record.files),
                        _ => std::sync::Arc::clone(&empty_files),
                    },
                    layout: self.layouts.get(&entry.folder),
                }
            })
            .collect()
    }

    /// How the index and the folders in `library/` disagree. It depends on nothing else, so the context keeps it for
    /// as long as both stay the same (`Data::folder_check`).
    pub fn check_folders(&self, folders: &[String]) -> FolderCheck {
        let on_disk: rustc_hash::FxHashSet<String> = folders.iter().cloned().collect();
        FolderCheck {
            missing: self
                .versions
                .keys()
                .filter(|folder| !on_disk.contains(folder.as_str()))
                .cloned()
                .collect(),
            unindexed: folders
                .iter()
                .filter(|folder| !self.versions.contains_key(folder.as_str()))
                .cloned()
                .collect(),
            on_disk,
        }
    }

    /// The family `id`, for a change. A family the index does not hold is the error a stale list gets.
    pub fn family_mut(&mut self, id: &str) -> Result<&mut Family, AppError> {
        self.families
            .get_mut(id)
            .ok_or_else(|| super::not_in_library(id))
    }

    /// The version in `folder`. A folder the index does not hold is the error a stale list gets.
    pub fn version(&self, folder: &str) -> Result<&ModVersion, AppError> {
        self.versions
            .get(folder)
            .ok_or_else(|| super::not_in_library(folder))
    }

    /// The version in `folder`, when it belongs to `family`. The error a stale list gets.
    pub fn version_of_family(&self, folder: &str, family: &str) -> Result<&ModVersion, AppError> {
        self.versions
            .get(folder)
            .filter(|version| version.family == family)
            .ok_or_else(|| version_unknown(folder))
    }

    /// The mutable version in `folder`, when it belongs to `family`. The error a stale list gets.
    pub fn version_of_family_mut(
        &mut self,
        folder: &str,
        family: &str,
    ) -> Result<&mut ModVersion, AppError> {
        self.versions
            .get_mut(folder)
            .filter(|version| version.family == family)
            .ok_or_else(|| version_unknown(folder))
    }

    /// Drops the families no version refers to.
    pub fn retain_families_of_versions(&mut self) {
        let families: HashSet<&str> = self
            .versions
            .values()
            .map(|version| version.family.as_str())
            .collect();
        self.families.retain(|id, _| families.contains(id.as_str()));
    }

    /// Original name of the archive the version was unpacked from, borrowed from the index.
    pub fn archive_name(&self, folder: &str) -> Option<&str> {
        let sha256 = self.versions.get(folder)?.archive.as_ref()?;
        self.archives
            .get(sha256)
            .map(|archive| archive.original_name.as_str())
    }

    /// Gives each archive without a `named` label the label a provider reads from its file name. The providers that
    /// named the versions of an archive are asked first, then the archive's own source ids; `archive_names` falls back to
    /// every provider. An archive that no provider reads stays as it is. Returns whether any archive changed, so the
    /// caller saves the index only then.
    pub fn name_archives(&mut self, archive_names: ArchiveLabels<'_>) -> bool {
        let mut version_providers: rustc_hash::FxHashMap<&str, Vec<&str>> =
            rustc_hash::FxHashMap::default();
        for version in self.versions.values() {
            if let Some(sha256) = version.archive.as_deref() {
                version_providers
                    .entry(sha256)
                    .or_default()
                    .extend(version.parsed_sources.keys().map(ProviderId::as_str));
            }
        }

        let mut labels = Vec::new();

        for (sha256, archive) in &self.archives {
            if archive.named.is_some() {
                continue;
            }

            let mut providers = version_providers
                .get(sha256.as_str())
                .cloned()
                .unwrap_or_default();
            providers.extend(archive.source_files.keys().map(ProviderId::as_str));

            if let Some(label) = archive_names(&providers, &archive.original_name) {
                labels.push((sha256.clone(), label));
            }
        }

        let changed = !labels.is_empty();

        for (sha256, label) in labels {
            if let Some(archive) = self.archives.get_mut(&sha256) {
                archive.named = Some(label);
            }
        }

        changed
    }

    /// The name of the archive copy in `archives/`, by SHA-256 sum.
    pub fn archive_file_of(&self, sha256: &str) -> Option<String> {
        self.archives
            .get(sha256)
            .map(|archive| archive.file.clone())
    }

    /// Names of the copies of all archives in the index. A new copy cannot take any of them.
    pub fn archive_files(&self) -> BTreeSet<String> {
        self.archives
            .values()
            .map(|archive| archive.file.clone())
            .collect()
    }

    /// Forgets archives that no version uses any more. Returns the names of the copies that stay.
    pub fn forget_unused_archives(&mut self) -> BTreeSet<String> {
        let used: HashSet<&str> = self
            .versions
            .values()
            .filter_map(|found| found.archive.as_deref())
            .collect();
        self.archives
            .retain(|sha256, _| used.contains(sha256.as_str()));
        self.archive_files()
    }

    /// After a family gets a new id, the relations that named the old id name the new one.
    pub fn rename_family_references(&mut self, from: &str, to: &str) {
        for family in self.families.values_mut() {
            for id in family
                .requires
                .iter_mut()
                .chain(family.incompatible.iter_mut())
                .chain(family.overrides.iter_mut())
            {
                if id == from {
                    *id = to.to_string();
                }
            }
        }
    }

    /// The family an import belongs to. A known part keeps its family, a new part of a known page gets its own
    /// suffixed family, and the first part of a page keeps the plain page id. An import without a part never joins a
    /// family that has a known part; it goes to an unresolved family until the provider check names its part.
    ///
    /// This is the write path that assigns an imported archive to a family, creating `#part` and `#unresolved`
    /// families. It is not `crate::domain::library::LibraryView::find`, which resolves a *wanted* mod against the
    /// families that already exist.
    pub fn family_for_source(&self, source: &SourceRef, name: &str) -> String {
        let page = crate::context::library::page_id_of(source);

        if let Some(part) = &source.part_id
            && let Some(family) = self.families.values().find(|family| {
                family.sources.get(&source.provider).is_some_and(|known| {
                    known.mod_id == source.mod_id && known.part_id.as_deref() == Some(part.as_str())
                })
            })
        {
            return family.id.clone();
        }

        let page_known = self.families.values().any(|family| {
            family
                .sources
                .get(&source.provider)
                .is_some_and(|known| known.mod_id == source.mod_id)
        });

        match (&source.part_id, page_known) {
            (Some(part), true) => format!("{page}#{part}"),
            (Some(_) | None, false) => page,
            (None, true) => {
                let wanted = crate::domain::naming::slug(name);

                if let Some(family) = self.families.values().find(|family| {
                    family.sources.get(&source.provider).is_some_and(|known| {
                        known.mod_id == source.mod_id && known.part_id.is_none()
                    })
                }) {
                    // A version that never had a parsed Nexus name came from the local library and got its page id
                    // typed by the user; a later import of that page belongs to it. A version parsed from a Nexus
                    // name joins only a family with the same name, so two manually downloaded addons stay apart.
                    let parsed = self.versions.values().any(|version| {
                        version.family == family.id
                            && version
                                .parsed_sources
                                .get(&source.provider)
                                .is_some_and(|id| id == &source.mod_id)
                    });

                    if !parsed || crate::domain::naming::slug(&family.name) == wanted {
                        return family.id.clone();
                    }
                }

                let key = source
                    .file_id
                    .as_deref()
                    .map_or_else(|| wanted, str::to_string);
                format!("{page}#unresolved:{key}")
            }
        }
    }

    /// Adds a family for a new version when it is missing. The name comes from `info.json` or the folder.
    fn ensure_family(&mut self, version: &ModVersion) {
        self.families
            .entry(version.family.clone())
            .or_insert_with(|| Family::of(version));
    }
}
