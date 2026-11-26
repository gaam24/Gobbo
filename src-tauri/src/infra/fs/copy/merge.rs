//! Replacing a target from a staged tree and merging one tree into another without deleting target entries.

use std::fs;
use std::io;
use std::path::Path;

use crate::infra::fs::remove::remove_path;
use crate::infra::fs::walk::{WalkErrors, walk};

use super::model::{CopyError, CopyMode, EntryKind, MergeConflict};
use super::sync::copy_dir;
#[cfg(test)]
use super::sync::injected_failure;
use super::tree::create_parent_dir;

/// A failed `Replace` copy cannot remove the old target: the new tree is staged first, then the old target becomes
/// `<name>.old`, and the staging tree is renamed into place. The old generation stays for the next read or a repair.
pub(super) fn replace_tree(
    source: &Path,
    target: &Path,
    keep_previous: bool,
) -> Result<(), CopyError> {
    // Read the source before touching the target. A missing or unreadable source must fail without a move.
    let meta = fs::metadata(source)?;
    // The staging tree next to the target. Its unique suffix lets two copies of the same path coexist until the rename.
    let staging = crate::infra::store::sibling_named(target, |name| {
        format!(".{name}.{}.tmp", crate::infra::store::unique_suffix())
    });
    let result = (|| -> Result<(), CopyError> {
        if meta.is_dir() {
            copy_dir(source, &staging, CopyMode::Replace)?;
        } else {
            create_parent_dir(&staging)?;
            fs::copy(source, &staging)?;
        }

        replace_with_staged(&staging, target, keep_previous)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = remove_path(&staging);
    }

    result
}

/// The kind of entry at `path`, or `None` when it does not exist.
fn entry_kind(path: &Path) -> io::Result<Option<EntryKind>> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(Some(EntryKind::Folder)),
        Ok(_) => Ok(Some(EntryKind::File)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The error for a source entry that has a different kind than the target entry at the same path.
fn conflict(path: &Path, source_kind: EntryKind, target_kind: EntryKind) -> CopyError {
    CopyError::Conflict(MergeConflict {
        path: path.to_path_buf(),
        source_kind,
        target_kind,
    })
}

/// Checks a merge before it writes: a file/folder mismatch anywhere in the tree stops it whole, so a conflict never
/// leaves the target half merged.
fn check_merge(source: &Path, target: &Path) -> Result<(), CopyError> {
    let mut found: Option<MergeConflict> = None;
    walk(source, WalkErrors::Fail, &mut |_, relative, is_dir| {
        if found.is_some() {
            return Ok(());
        }

        let to = target.join(relative);
        let wanted = if is_dir {
            EntryKind::Folder
        } else {
            EntryKind::File
        };

        if let Some(have) = entry_kind(&to)?
            && have != wanted
        {
            found = Some(MergeConflict {
                path: to,
                source_kind: wanted,
                target_kind: have,
            });
        }

        Ok(())
    })?;
    found.map_or(Ok(()), |conflict| Err(CopyError::Conflict(conflict)))
}

/// Copies one source file over the target, creating the folder above it. Used by the merge and its single-file case.
fn merge_file(source: &Path, target: &Path) -> Result<(), CopyError> {
    create_parent_dir(target)?;
    #[cfg(test)]
    if injected_failure() {
        return Err(io::Error::other("injected copy failure").into());
    }

    fs::copy(source, target)?;
    Ok(())
}

/// Copies every entry of a merge source over the target. The types were checked by `check_merge` first.
fn merge_entries(source: &Path, target: &Path) -> Result<(), CopyError> {
    walk(source, WalkErrors::Fail, &mut |entry, relative, is_dir| {
        let to = target.join(relative);

        if is_dir {
            fs::create_dir_all(&to)?;
        } else {
            merge_file(&entry.path(), &to)?;
        }

        Ok(())
    })?;
    Ok(())
}

/// `CopyMode::Merge`: copies the source over the target, overwriting same-named files and keeping every target entry
/// the source does not name. The type check runs before the first write.
pub(super) fn merge_tree(source: &Path, target: &Path) -> Result<(), CopyError> {
    let meta = fs::metadata(source)?;
    let wanted = if meta.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::File
    };

    if let Some(have) = entry_kind(target)?
        && have != wanted
    {
        return Err(conflict(target, wanted, have));
    }

    if meta.is_dir() {
        check_merge(source, target)?;
        fs::create_dir_all(target)?;
        merge_entries(source, target)
    } else {
        merge_file(source, target)
    }
}

/// Replaces `target` with an already staged tree: the target becomes `<name>.old`, the staged tree is renamed into
/// place, and the previous target is restored when that rename fails. With `keep_previous` false the previous
/// generation is removed after a success. The caller owns `staging`; a failure leaves it in place for the caller to
/// remove.
pub fn replace_with_staged(staging: &Path, target: &Path, keep_previous: bool) -> io::Result<()> {
    // The one previous generation of a store entry. It is replaced on the next successful copy, so it never accumulates.
    let backup = crate::infra::store::sibling_named(target, |name| format!("{name}.old"));
    let had_target = fs::symlink_metadata(target).is_ok();

    if had_target {
        remove_path(&backup)?;
        fs::rename(target, &backup)?;
    }

    if let Err(error) = fs::rename(staging, target) {
        if had_target && let Err(restore) = fs::rename(&backup, target) {
            return Err(io::Error::new(
                restore.kind(),
                format!(
                    "{error}; the previous copy could not be restored and is kept at {}: {restore}",
                    backup.display()
                ),
            ));
        }

        return Err(error);
    }

    if had_target && !keep_previous {
        let _ = remove_path(&backup);
    }

    Ok(())
}
