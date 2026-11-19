//! Scanning the library folders: the explicit rescan, the deployment refresh and the record a scan keeps from the
//! index.

use std::collections::BTreeMap;
use std::path::Path;

use crate::domain::profiles::entry::ProfileMod;
use crate::domain::resolver::LayoutRules;
use crate::error::AppError;
use crate::games::GameDef;

use super::super::folders::{mod_folders, scan_files, scan_mod, version_from_import};
use crate::domain::library::{Library, ModVersion};

impl Library {
    /// Scans the whole library folder. New versions are added, removed ones are dropped, and the family data stays.
    pub fn rescan(&mut self, library_dir: &Path, game: &GameDef) -> Result<(), AppError> {
        let folders = mod_folders(library_dir)?;
        let refs: Vec<&str> = folders.iter().map(String::as_str).collect();
        // An explicit rescan reads `info.json` again; the deployment refresh below keeps the known record.
        let scanned = scan_folders(library_dir, &refs)?;
        let mut versions = BTreeMap::new();
        for (folder, (files, size_bytes)) in folders.into_iter().zip(scanned) {
            let mut found = version_from_import(
                &library_dir.join(&folder),
                &folder,
                &game.slot_for_folder(&folder),
                files,
                size_bytes,
            );
            self.keep_known(&mut found, game);
            self.ensure_family(&found);
            versions.insert(folder, found);
        }
        self.versions = versions;
        self.retain_families_of_versions();
        self.fill_layouts(game);
        Ok(())
    }

    pub(super) fn fill_layouts(&mut self, game: &GameDef) {
        let rules = LayoutRules::new(&game.deploy);
        self.layouts.clear();
        self.layouts.reserve(self.versions.len());
        for (folder, version) in &self.versions {
            self.layouts
                .insert(folder.clone(), rules.profile(&version.files));
        }
    }

    /// Scans the enabled versions of the profile, so the deployment sees the state of the disk. Disabled ones stay as
    /// they are. A version the index knows keeps its record: only its files, size and an invalid slot are updated.
    pub fn refresh(
        &mut self,
        library_dir: &Path,
        game: &GameDef,
        mods: &[ProfileMod],
    ) -> Result<(), AppError> {
        let folders: Vec<&str> = mods
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.folder.as_str())
            .collect();
        for folder in &folders {
            if !library_dir.join(folder).is_dir() {
                return Err(crate::context::library::not_in_library(folder));
            }
        }

        let scanned = scan_folders(library_dir, &folders)?;
        let rules = LayoutRules::new(&game.deploy);
        // Merge in the original order, so the error of the first scanned folder is the one returned and every known
        // record is updated in place.
        for (folder, (files, size_bytes)) in folders.into_iter().zip(scanned) {
            if let Some(found) = self.versions.get_mut(folder) {
                // The record stays as it is: only what the disk says can change, so its info, edits, source ids,
                // archive and FOMOD choices are not cloned back over it.
                found.files = files.into();
                found.size_bytes = size_bytes;
                if !found.slot.is_empty() && game.deploy.slot(&found.slot).is_none() {
                    found.slot = game.slot_for_folder(folder);
                }
                let layout = rules.profile(&found.files);
                self.layouts.insert(folder.to_string(), layout);
                continue;
            }
            let mut found = version_from_import(
                &library_dir.join(folder),
                folder,
                &game.slot_for_folder(folder),
                files,
                size_bytes,
            );
            self.keep_known(&mut found, game);
            self.ensure_family(&found);
            let layout = rules.profile(&found.files);
            self.layouts.insert(folder.to_string(), layout);
            self.versions.insert(folder.to_string(), found);
        }
        Ok(())
    }

    /// Scans one version folder again and keeps what the index knows about it. A folder the index does not know is added.
    pub fn rescan_version(
        &mut self,
        library_dir: &Path,
        game: &GameDef,
        folder: &str,
    ) -> Result<(), AppError> {
        let mut found = scan_mod(library_dir, folder, game)?;
        self.keep_known(&mut found, game);
        self.ensure_family(&found);
        let rules = LayoutRules::new(&game.deploy);
        self.layouts
            .insert(folder.to_string(), rules.profile(&found.files));
        self.versions.insert(folder.to_string(), found);
        Ok(())
    }

    /// Keeps what the index knows about a scanned version and the disk does not show: its family, slot, archive and
    /// the user's edits. A slot the game no longer has is replaced by the one the scan found.
    fn keep_known(&self, found: &mut ModVersion, game: &GameDef) {
        let Some(old) = self.versions.get(&found.folder) else {
            return;
        };
        found.family.clone_from(&old.family);
        if old.slot.is_empty() || game.deploy.slot(&old.slot).is_some() {
            found.slot.clone_from(&old.slot);
        }
        found.archive.clone_from(&old.archive);
        // A rescan reads the folder again but does not rehash it. Keeping the baseline is what lets a verification
        // notice files that were changed by hand after the import.
        found.integrity.clone_from(&old.integrity);
        found.parsed_sources = old.parsed_sources.clone();
        found.parsed_parts = old.parsed_parts.clone();
        found.installed_at = old.installed_at;
        found.fomod.clone_from(&old.fomod);
        found.fomod_installer = old.fomod_installer;
        found.edit.clone_from(&old.edit);
        // A fresh name that is only the folder echo means `info.json` has no name of its own; keep what the import
        // decided then, not just the name but the rest of the info too.
        if found.info.name == found.folder {
            found.info.clone_from(&old.info);
        }
        // The label is read once, at import, and kept with the archive. Only a version a provider named takes it.
        let label = (!found.parsed_sources.is_empty())
            .then_some(self.archive_label_of(found))
            .flatten();
        let fresh_version = found.info.version.clone();
        let (name, fill_version) = crate::domain::naming::imported(
            label,
            None, // a rescan has no provider answer; the stored version preserves the import decision
            Some(found.info.name.as_str()),
            fresh_version.as_deref(),
            found.folder.as_str(),
        );
        found.info.name = name;
        // Keep the version the import stored whenever it has one; fill only what is missing, with the label before
        // `info.json`.
        found.info.version = old.info.version.clone().or(fill_version);
        found.edit.apply(&mut found.info);
    }
}

/// One folder's files and their sizes.
fn scan_one(library_dir: &Path, folder: &str) -> Result<(Vec<String>, u64), AppError> {
    scan_files(library_dir, folder)
}

/// Scans the folders in parallel and returns their files and sizes in the original order. The work is mostly metadata
/// and directory reads, so a small number of workers helps without contending for the disk. Both a full rescan and the
/// deployment refresh use this one implementation; only the rescan reads `info.json` afterwards.
pub(super) fn scan_folders(
    library_dir: &Path,
    folders: &[&str],
) -> Result<Vec<(Vec<String>, u64)>, AppError> {
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(8)
        .min(folders.len());
    if workers <= 1 {
        return folders
            .iter()
            .map(|folder| scan_one(library_dir, folder))
            .collect();
    }
    let chunk = folders.len().div_ceil(workers);
    let scanned = std::thread::scope(|scope| -> Result<Vec<_>, AppError> {
        let handles: Vec<_> = folders
            .chunks(chunk)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|folder| scan_one(library_dir, folder))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut scanned = Vec::with_capacity(folders.len());
        for handle in handles {
            let mut part = handle
                .join()
                .map_err(|_| AppError::unexpected("a library scan worker panicked"))?;
            scanned.append(&mut part);
        }
        Ok(scanned)
    })?;
    scanned.into_iter().collect()
}
