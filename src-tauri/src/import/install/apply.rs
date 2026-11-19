#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
#[cfg(test)]
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use rustc_hash::FxHashSet;

use crate::error::FileError;
use crate::infra::archive::Archive;
use crate::infra::io::Reporter;

use super::{
    Import, InstallError, Installed, Naming, Placed, Plan, Staged, StagedMod, check_limits,
    free_folder_name, wanted_name,
};

// A test-only countdown for a failure part way through extraction. Production code has no fault hook.
#[cfg(test)]
thread_local! {
    static FAIL_AFTER: Cell<Option<(usize, io::ErrorKind)>> = const { Cell::new(None) };
}

/// Fails before the next `files` extracted entries, with an error of `kind`. `fail_after(0, _)` clears an earlier fault.
#[cfg(test)]
pub fn fail_after(files: usize, kind: io::ErrorKind) {
    FAIL_AFTER.with(|cell| cell.set((files > 0).then_some((files, kind))));
}

#[cfg(test)]
fn injected_failure() -> Option<io::ErrorKind> {
    FAIL_AFTER.with(|cell| {
        let (left, kind) = cell.get()?;
        if left == 1 {
            cell.set(None);
        } else {
            cell.set(Some((left - 1, kind)));
        }
        Some(kind)
    })
}

/// Extracts the planned entries into `dir`: mod `i`, in the order of the group names, goes to `dir/<i>`. `progress`
/// receives the percentage of extracted bytes. When this fails, `dir` may hold part of the mods; the caller removes it.
pub fn extract(
    archive: &Archive,
    plan: &Plan,
    dir: &Path,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(u32),
) -> Result<Staged, InstallError> {
    // A folder takes the slot of its files in a slot; a folder of mirrored files only has none.
    let mut groups: BTreeMap<&str, &str> = BTreeMap::new();
    for item in &plan.placed {
        let slot = groups
            .entry(item.group.as_str())
            .or_insert(item.slot.as_str());
        if slot.is_empty() {
            *slot = item.slot.as_str();
        }
    }
    let position: BTreeMap<&str, usize> = groups
        .keys()
        .enumerate()
        .map(|(at, group)| (*group, at))
        .collect();
    let mut mods: Vec<StagedMod> = groups
        .iter()
        .map(|(group, slot)| StagedMod {
            group: (*group).to_string(),
            slot: (*slot).to_string(),
            files: 0,
            paths: Vec::new(),
            size_bytes: 0,
            integrity: None,
        })
        .collect();
    let group_dirs: Vec<PathBuf> = (0..mods.len()).map(|at| dir.join(at.to_string())).collect();
    let mut known_dirs: FxHashSet<PathBuf> = FxHashSet::default();

    let total: u64 = plan
        .placed
        .iter()
        .map(|item| archive.entries()[item.index].size)
        .sum();
    check_limits(plan.placed.len(), total)?;
    let mut reporter = Reporter::new(total, progress);
    let wanted: BTreeSet<usize> = plan.placed.iter().map(|item| item.index).collect();
    // Indexed by the archive entry, not by the plan: a visit asks for each planned entry once, and a map lookup per
    // entry was logarithmic in the number of entries.
    let mut by_index: Vec<Option<&Placed>> = vec![None; archive.entries().len()];
    let mut group_at: Vec<Option<usize>> = vec![None; archive.entries().len()];
    for item in &plan.placed {
        by_index[item.index] = Some(item);
        group_at[item.index] = position.get(item.group.as_str()).copied();
    }
    archive.visit::<InstallError>(&wanted, |index, data| {
        if crate::infra::tasks::cancelled(cancel) {
            return Err(InstallError::Cancelled);
        }
        let item = by_index[index].ok_or(InstallError::UnknownLayout)?;
        let at = group_at[index].ok_or(InstallError::UnknownLayout)?;
        let dst = group_dirs[at].join(&item.rel);
        #[cfg(test)]
        if let Some(kind) = injected_failure() {
            return Err(FileError::new(&dst, io::Error::from(kind)).into());
        }
        if let Some(parent) = dst.parent()
            && !known_dirs.contains(parent)
        {
            fs::create_dir_all(parent).map_err(|e| FileError::new(parent, e))?;
            known_dirs.insert(parent.to_path_buf());
        }
        let mut out = File::create(&dst).map_err(|e| FileError::new(&dst, e))?;
        match reporter.copy_cancellable(data, &mut out, cancel) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                return Err(InstallError::Cancelled);
            }
            Err(error) => return Err(FileError::new(&dst, error).into()),
        }
        mods[at].files += 1;
        mods[at].paths.push(item.rel.clone());
        mods[at].size_bytes = mods[at]
            .size_bytes
            .saturating_add(archive.entries()[index].size);
        Ok(())
    })?;
    reporter.finish();
    // The files are still in the working folder, outside the index lock. Hash them here so the stored checksum is
    // taken at import; a later "verify" only has to hash the installed copy again and compare.
    for (at, staged_mod) in mods.iter_mut().enumerate() {
        if crate::infra::tasks::cancelled(cancel) {
            return Err(InstallError::Cancelled);
        }
        let path = &group_dirs[at];
        let (digest, files, bytes) =
            crate::infra::fs::hash_tree(path).map_err(|error| FileError::new(path, error))?;
        staged_mod.integrity = Some(crate::domain::library::Integrity {
            digest,
            files,
            bytes,
        });
    }
    Ok(Staged {
        dir: dir.to_path_buf(),
        mods,
        skipped: plan.skipped,
        skipped_files: plan.skipped_files.clone(),
    })
}

/// Moves the extracted mods into `library`. Each takes the first free folder name of its group, or `naming.only` for a
/// reinstall. When a move fails, the folders moved so far are removed again. The names are chosen here, so the caller
/// holds whatever lock keeps other imports from choosing the same names.
pub fn place(
    staged: &mut Staged,
    library: &Path,
    naming: Naming<'_>,
) -> Result<Import, InstallError> {
    let mut named: Vec<(String, usize)> = Vec::with_capacity(staged.mods.len());
    for (at, staged_mod) in staged.mods.iter().enumerate() {
        let wanted = wanted_name(naming.base_name, staged.mods.len(), &staged_mod.group);
        let name = match naming.only {
            Some(folder) => folder.to_string(),
            None => free_folder_name(library, &wanted, naming.digest)?,
        };
        named.push((name, at));
    }
    // The library lists its folders by name, so the mods are moved and reported in that order.
    named.sort();

    let mut import = Import {
        skipped: staged.skipped,
        skipped_files: std::mem::take(&mut staged.skipped_files),
        ..Import::default()
    };
    for (name, at) in named {
        let target = library.join(&name);
        if let Err(error) = fs::rename(staged.dir.join(at.to_string()), &target) {
            for moved in &import.mods {
                let _ = fs::remove_dir_all(library.join(&moved.folder));
            }
            return Err(FileError::new(&target, error).into());
        }
        let mut paths = std::mem::take(&mut staged.mods[at].paths);
        paths.sort_unstable();
        import.mods.push(Installed {
            folder: name,
            slot: staged.mods[at].slot.clone(),
            files: staged.mods[at].files,
            paths,
            size_bytes: staged.mods[at].size_bytes,
            integrity: staged.mods[at].integrity.clone(),
        });
    }
    Ok(import)
}
