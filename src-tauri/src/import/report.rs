//! Comparing a mod's installed folder with the archive copy it came from.

use std::collections::{BTreeMap, BTreeSet};

use super::archive_base_name;
use super::model::ArchiveReport;
use super::wizard::fomod_allowed;
use crate::context::{Data, load_library, load_settings};
use crate::domain::library::ModVersion;
use crate::error::{AppError, ErrorKind, io_at};
use crate::games;
use crate::import::install::{self, Plan};
use crate::infra::archive::Archive;
use crate::infra::fs::file_sizes;

/// Compares the archive copy with the installed folder, using the current game rules and the version's FOMOD choices.
/// Nothing is written.
pub fn report(root: &Data, game_id: &str, folder: &str) -> Result<ArchiveReport, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let record = library.version(folder)?.clone();
    let digest = record
        .archive
        .clone()
        .ok_or_else(|| archive_error(folder, "library.no_archive"))?;
    let file = library
        .archive_file_of(&digest)
        .ok_or_else(|| archive_error(folder, "library.archive_missing"))?;
    let path = data.archives().join(&file);

    if !path.is_file() {
        return Err(archive_error(folder, "library.archive_missing"));
    }

    let archive =
        Archive::open(&path).map_err(|error| AppError::from(install::InstallError::from(error)))?;
    let selection = match record.installer_choices() {
        Some(choices) => Some(fomod_allowed(&archive, choices)?),
        None => None,
    };
    let settings = load_settings(&data);
    let roots = crate::context::target::deployment_root(&data, &game, &settings)
        .ok()
        .map(|root| install::top_level_folders(&root))
        .unwrap_or_default();
    // The report uses the same FOMOD destinations as the import, so the paths it compares are the ones installed.
    let plan = install::plan_with_roots(
        &archive,
        &game,
        &archive_base_name(&file),
        selection.as_ref().map(|selection| &selection.allowed),
        selection.as_ref().map(|selection| &selection.virtual_paths),
        &roots,
    )?;
    let expected = expected_files(&plan, &record, &archive);
    let library_folder = data.library().join(folder);
    // A folder that is gone still has a useful report: every planned file is missing. One walk gives both the names
    // and the sizes; a report used to stat every installed file a second time to find the changed ones.
    let sizes = if library_folder.is_dir() {
        file_sizes(&library_folder).map_err(io_at(&library_folder))?
    } else {
        BTreeMap::new()
    };
    let actual_set: BTreeSet<&str> = sizes.keys().map(String::as_str).collect();
    let missing = expected
        .keys()
        .filter(|path| !actual_set.contains(path.as_str()))
        .cloned()
        .collect();
    let extra = sizes
        .keys()
        .filter(|path| !expected.contains_key(path.as_str()))
        .cloned()
        .collect();
    let changed = expected
        .iter()
        .filter(|(path, size)| {
            sizes
                .get(path.as_str())
                .is_some_and(|actual| actual != *size)
        })
        .map(|(path, _)| path.clone())
        .collect();
    Ok(ArchiveReport {
        folder: folder.to_string(),
        archive: file,
        skipped: plan.skipped_files.clone(),
        skipped_total: plan.skipped as u32,
        missing,
        extra,
        changed,
        expected: u32::try_from(expected.len()).unwrap_or(u32::MAX),
        installed: u32::try_from(sizes.len()).unwrap_or(u32::MAX),
    })
}

/// The files the plan places in the version's own folder, by library-relative path and archive size. A multi-mod archive
/// is matched to the folder by the files it already has; the group name breaks a tie.
fn expected_files(plan: &Plan, record: &ModVersion, archive: &Archive) -> BTreeMap<String, u64> {
    let mut groups: BTreeMap<&str, Vec<&install::Placed>> = BTreeMap::new();

    for item in &plan.placed {
        groups.entry(item.group.as_str()).or_default().push(item);
    }

    let present: BTreeSet<&str> = record.files.iter().map(String::as_str).collect();
    let folder = record.folder.to_lowercase();
    let chosen = groups
        .iter()
        .max_by_key(|(group, items)| {
            let overlap = items
                .iter()
                .filter(|item| present.contains(item.rel.as_str()))
                .count();
            let name = usize::from(folder.contains(&group.to_lowercase()));
            overlap.saturating_mul(2).saturating_add(name)
        })
        .map(|(group, items)| (*group, items.as_slice()));
    let Some((group, items)) = chosen else {
        return BTreeMap::new();
    };
    let mut expected: BTreeMap<String, u64> = items
        .iter()
        .map(|item| (item.rel.clone(), archive.entries()[item.index].size))
        .collect();

    if let Some(file) = plan.enable_files.get(group) {
        // The importer writes an empty file today; keep this in step if a slot ever needs content.
        expected.entry(file.clone()).or_insert(0);
    }

    expected
}

/// A missing archive or record: a user action, since the file can be restored or the mod imported again.
fn archive_error(folder: &str, code: &str) -> AppError {
    AppError::new(code)
        .with("folder", folder)
        .kind(ErrorKind::UserAction)
}
