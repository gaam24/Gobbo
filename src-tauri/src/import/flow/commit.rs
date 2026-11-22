//! Committing an import: place the extracted mods in the library, add their records and clean up a failed save.

use std::collections::BTreeMap;
use std::path::Path;

use super::prepare::{Copy, Prepared};
use crate::context::Data;
use crate::domain::library::{Archive, Family, Library, ModVersion};
use crate::domain::naming;
use crate::domain::source::SourceRef;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::import::ALREADY_IN_LIBRARY;
use crate::import::install::{self, Naming};

/// What a committed import added, for the view and for the rollback of a failed save.
pub(super) struct Committed {
    pub(super) folders: Vec<String>,
    pub(super) files: u32,
    pub(super) skipped: u32,
    pub(super) skipped_files: Vec<crate::domain::library::SkippedFile>,
    pub(super) mods: Vec<crate::import::model::ImportedModView>,
}

/// Places a prepared import into the library. The caller holds the index lock; the library is saved once by the caller.
pub(super) fn commit_prepared(
    data: &Data,
    game: &GameDef,
    library: &mut Library,
    prepared: &mut Prepared,
) -> Result<Committed, AppError> {
    let library_dir = data.library();
    let naming = Naming {
        base_name: &prepared.base_name,
        digest: &prepared.copy.digest,
        only: None,
    };
    let installed =
        install::place(&mut prepared.staged, &library_dir, naming).map_err(AppError::from)?;
    let folders: Vec<String> = installed.mods.iter().map(|m| m.folder.clone()).collect();
    let files = installed.mods.iter().map(|m| m.files as u32).sum();
    let skipped = installed.skipped as u32;
    let skipped_files = installed.skipped_files;
    // The extractor already knows every file and its uncompressed size. Build the index records from that, so the new
    // folders are not walked a second time; only `info.json` is read. The paths move into the records instead of being
    // cloned once more.
    let records: Vec<ModVersion> = installed
        .mods
        .into_iter()
        .map(|mod_installed| {
            let paths = mod_installed.paths;
            let integrity = mod_installed.integrity;
            let mut record = crate::context::library::version_from_import(
                &library_dir.join(&mod_installed.folder),
                &mod_installed.folder,
                &mod_installed.slot,
                paths,
                mod_installed.size_bytes,
            );
            record.integrity = integrity;
            record
        })
        .collect();
    if let Err(error) = std::fs::rename(&prepared.copy.copy, &prepared.claim.archive)
        .map_err(io_at(&prepared.claim.archive))
    {
        remove_folders(&library_dir, &folders);
        return Err(error);
    }

    let mods = add_records(library, game, prepared, records, &skipped_files);
    Ok(Committed {
        folders,
        files,
        skipped,
        skipped_files,
        mods,
    })
}

/// A map with one provider and its value, or an empty map when the source or the value is missing.
fn one_source<T>(
    source: Option<&SourceRef>,
    value: impl FnOnce(&SourceRef) -> Option<T>,
) -> BTreeMap<crate::domain::source::ProviderId, T> {
    source
        .and_then(|source| {
            value(source).map(|value| BTreeMap::from([(source.provider.clone(), value)]))
        })
        .unwrap_or_default()
}

/// Adds the extracted versions and their archive to the index, and answers what it added for the window.
fn add_records(
    library: &mut Library,
    game: &GameDef,
    prepared: &Prepared,
    records: Vec<ModVersion>,
    skipped: &[crate::domain::library::SkippedFile],
) -> Vec<crate::import::model::ImportedModView> {
    let mut imported = Vec::with_capacity(records.len());
    let source = prepared.copy.source.clone();
    let named_by: Vec<&str> = source
        .iter()
        .map(|source| source.provider.as_str())
        .collect();
    // The providers read the file name once; the archive keeps the label, so no later step parses it again.
    let named = crate::providers::archive_name_from(&named_by, &prepared.copy.original);
    let single_mod = records.len() == 1;
    let family_source = source.clone().filter(|_| single_mod);
    // A provider that names the exact file decides its version, whether or not `info.json` carries one. The archive
    // keeps the label whatever the answer is; only a single-mod archive may be renamed by it.
    let api_version = family_source
        .as_ref()
        .and_then(|source| {
            let file_id = source.file_id.as_deref()?;
            crate::providers::provider(source.provider.as_str())?.version_of_file(file_id)
        })
        .map(str::to_string);
    let label = single_mod.then_some(named.as_ref()).flatten();

    for mut record in records {
        record.archive = Some(prepared.copy.digest.clone());
        record.parsed_sources =
            one_source(family_source.as_ref(), |source| Some(source.mod_id.clone()));
        record.parsed_parts = one_source(family_source.as_ref(), |source| source.part_id.clone());
        record.fomod.clone_from(&prepared.choices);
        record.fomod_installer = prepared.installer;
        // One rule for every import path. `record.info.name` is the `info.json` name or the folder echo, so passing
        // the folder as the stem lets the rule tell an echo from a real name.
        let (name, version) = naming::imported(
            label,
            api_version.as_deref(),
            Some(record.info.name.as_str()),
            record.info.version.as_deref(),
            record.folder.as_str(),
        );
        record.info.name = name;
        record.info.version = version;
        record.family = family_source.as_ref().map_or_else(
            || crate::context::library::family_id_from(None, &record.info.name),
            |source| library.family_for_source(source, &record.info.name),
        );
        let mut family = Family::of(&record);

        if let Some(source) = &family_source
            && let Some(known) = family.sources.get_mut(&source.provider)
        {
            known.mod_id.clone_from(&source.mod_id);
            known.set_part_fields(
                source.part_id.as_deref(),
                source.part_name.as_deref(),
                source.part_category.as_deref(),
            );
        }

        if let Some(known) = library.families.get_mut(&family.id) {
            if let Some(source) = &family_source
                && let Some(entry) = known.sources.get_mut(&source.provider)
            {
                entry.mod_id.clone_from(&source.mod_id);
                entry.fill_missing_part(
                    source.part_id.as_deref(),
                    source.part_name.as_deref(),
                    source.part_category.as_deref(),
                );
            }
        } else {
            library.families.insert(family.id.clone(), family);
        }

        let folder = record.folder.clone();
        imported.push(crate::import::model::ImportedModView {
            family: record.family.clone(),
            sha256: prepared.copy.digest.clone(),
            folder: folder.clone(),
            name: record.info.name.clone(),
            version: record.info.version.clone(),
        });
        let layout = crate::domain::resolver::layout_profile(&game.deploy, &record.files);
        library.versions.insert(folder.clone(), record);
        library.layouts.insert(folder, layout);
    }

    let (skipped_files, skipped_total) = crate::import::skipped::archive_skipped(skipped);
    library.archives.insert(
        prepared.copy.digest.clone(),
        Archive {
            original_name: prepared.copy.original.clone(),
            file: prepared.file.clone(),
            named,
            source_files: one_source(source.as_ref(), |source| source.file_id.clone()),
            source_parts: one_source(source.as_ref(), |source| source.part_id.clone()),
            skipped: skipped_files,
            skipped_total,
        },
    );
    imported
}

pub(super) fn forget_download(data: &Data, copy: &Copy) {
    if let Some(download) = &copy.download {
        crate::infra::sidecar::forget_download(&data.downloads(), download);
    }
}

/// The error of an archive the library already holds, in `folder`.
pub fn already_in_library(folder: &str) -> AppError {
    AppError::user(ALREADY_IN_LIBRARY).with("folder", folder)
}

/// Removes the folders that an import has just unpacked. It touches nothing else.
pub fn remove_folders(library: &Path, folders: &[String]) {
    for folder in folders {
        let _ = std::fs::remove_dir_all(library.join(folder));
    }
}
