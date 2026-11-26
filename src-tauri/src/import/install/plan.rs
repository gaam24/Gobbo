//! Decides where each entry of an archive goes, and the game's top-level folders used by the wrapper rules.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::domain::library::{SkipReason, SkippedFile};
use crate::domain::pathkey::path_key_into;
use crate::games::GameDef;
use crate::infra::archive::Archive;

use super::errors::InstallError;
use super::layout::{LayoutPlan, UnitIndex, attach_mirrored};
use super::loader::{loader_layout, places_every_check};
use super::model::{Placed, Plan};
use super::naming::{refuse_unwritable, split_name};

/// Archive folders that are documentation or variants, not game files. They are skipped before the layout rules, so a
/// full-root game does not put a README folder into the game.
fn is_archive_noise(name: &str) -> bool {
    let first = name.split('/').next().unwrap_or(name).trim_end_matches('/');
    [
        "__macosx",
        ".git",
        ".svn",
        "docs",
        "documentation",
        "readme",
        "source",
        "src",
        "optional",
    ]
    .iter()
    .any(|folder| first.eq_ignore_ascii_case(folder))
}

/// The top-level folders of the game folder, sorted. A wrapper around one of them is stripped from an archive path.
/// A missing or unreadable folder gives an empty list; the version and same-name wrapper rules still apply.
pub fn top_level_folders(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

/// Decides where each entry of the archive goes. Nothing is written. `base_name` names the files that belong to the
/// archive as a whole, and `allowed` limits the entries to the ones the FOMOD wizard chose.
pub fn plan(
    archive: &Archive,
    game: &GameDef,
    base_name: &str,
    allowed: Option<&BTreeSet<usize>>,
) -> Result<Plan, InstallError> {
    plan_with_roots(archive, game, base_name, allowed, None, &[])
}

/// `plan` with the game's own top-level folders, so a wrapper around `bin/` or `r6/` is stripped and a folder with a
/// game name is not a wrapper. Empty when the game folder is not known, for example in a unit test.
///
/// `virtual_paths` maps a selected archive entry to the game path its FOMOD pattern's `destination` asks for. It is
/// used for every layout decision in place of the archive name; the entry's real index is kept, so extraction still
/// reads the right bytes. `None` is a plain archive, where the archive path is the game path.
pub fn plan_with_roots<'a>(
    archive: &Archive,
    game: &'a GameDef,
    base_name: &str,
    allowed: Option<&BTreeSet<usize>>,
    virtual_paths: Option<&BTreeMap<usize, String>>,
    base_roots: &'a [String],
) -> Result<Plan, InstallError> {
    let deploy = &game.deploy;

    // The file entries this import considers, split into parts once.
    let mut files: Vec<(usize, Vec<&str>)> = Vec::with_capacity(archive.entries().len());
    let mut skipped_files = Vec::new();

    for (index, entry) in archive.entries().iter().enumerate() {
        if entry.is_dir {
            continue;
        }

        let name = entry.name.as_str();
        // A root `info.xml` is always installer metadata. The whole `fomod/` folder is metadata only for a plain
        // archive: with a FOMOD selection the `allowed` set already holds just the files whose source pattern matched,
        // and a real mod can keep its source files under `fomod/`. Compare without allocating a lowercased copy of
        // every entry name.
        let metadata = name.eq_ignore_ascii_case("info.xml")
            || (allowed.is_none()
                && name
                    .get(.."fomod/".len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fomod/")));
        if metadata {
            skipped_files.push(SkippedFile {
                path: entry.name.clone(),
                reason: SkipReason::Metadata,
            });
            continue;
        }

        if is_archive_noise(name) {
            skipped_files.push(SkippedFile {
                path: entry.name.clone(),
                reason: SkipReason::Noise,
            });
            continue;
        }

        if allowed.is_some_and(|allowed| !allowed.contains(&index)) {
            skipped_files.push(SkippedFile {
                path: entry.name.clone(),
                reason: SkipReason::NotSelected,
            });
            continue;
        }
        // Layout decisions use the FOMOD destination when the wizard supplied one. The archive name is still the
        // source of truth for the metadata/noise/choice checks above and for `SkippedFile` paths below.
        let layout_name = virtual_paths
            .and_then(|paths| paths.get(&index))
            .map_or(name, String::as_str);
        files.push((index, split_name(layout_name)?));
    }

    // A loader release keeps the game's own rules when they already put every check file at its game path, as a
    // `BepInEx` pack with plugins does: its plugins still go to their slot. Otherwise the release is one mod whose files
    // keep their game paths, wherever the archive keeps the release (UE4SS below the game's program folder).
    let loader = game.install.as_ref().filter(|_| {
        game.is_loader_archive(archive.entries().iter().map(|entry| entry.name.as_str()))
    });
    let before = skipped_files.len();

    let layout = LayoutPlan::new(deploy, base_roots);
    let wrapper = layout.wrapper_depth(&files);

    for (_, parts) in &mut files {
        let strip = wrapper.min(parts.len().saturating_sub(1));
        parts.drain(..strip);
        layout.rewrite_aliases(parts);
    }

    let units = layout.unit_roots(&files);
    let unit_index = UnitIndex::new(&units);

    let mut placed = Vec::new();
    let mut skipped = 0;

    for (index, parts) in &files {
        if let Some((group, slot, rel)) = layout.classify(parts, &unit_index, &units, base_name)? {
            placed.push(Placed {
                index: *index,
                group,
                slot,
                rel,
            });
        } else {
            skipped += 1;
            skipped_files.push(SkippedFile {
                path: archive.entries()[*index].name.clone(),
                reason: SkipReason::NoRule,
            });
        }
    }

    if let Some(install) = loader
        && !places_every_check(install, &placed)
    {
        let loader_files = files.clone();
        let mut loader_skipped = skipped_files[..before].to_vec();

        if let Some(placed) = loader_layout(install, &loader_files, base_name, &mut loader_skipped)
        {
            refuse_unwritable(&placed, archive)?;
            let skipped = loader_skipped.len() - before;
            return Ok(Plan {
                placed,
                enable_files: BTreeMap::new(),
                skipped,
                skipped_files: loader_skipped,
            });
        }
    }

    if placed.is_empty() {
        return Err(InstallError::UnknownLayout);
    }
    refuse_unwritable(&placed, archive)?;

    attach_mirrored(&mut placed);
    // A slot's `enable_file` is written once per mod, unless the archive already ships it.
    let mut enable_files: BTreeMap<String, String> = BTreeMap::new();

    for item in &placed {
        if enable_files.contains_key(&item.group) {
            continue;
        }

        if let Some(file) = deploy
            .slots
            .iter()
            .find(|slot| slot.dir == item.slot)
            .and_then(|slot| slot.enable_file.as_ref())
        {
            enable_files.insert(item.group.clone(), file.clone());
        }
    }

    let mut file_key = String::new();
    let mut rel_key = String::new();
    for item in &placed {
        if enable_files.get(&item.group).is_some_and(|file| {
            file_key.clear();
            rel_key.clear();
            let file_range = path_key_into(file, &mut file_key);
            let rel_range = path_key_into(&item.rel, &mut rel_key);
            file_key[file_range] == rel_key[rel_range]
        }) {
            enable_files.remove(&item.group);
        }
    }

    Ok(Plan {
        placed,
        enable_files,
        skipped,
        skipped_files,
    })
}
