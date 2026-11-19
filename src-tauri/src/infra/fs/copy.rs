//! Copying, replacing, merging and hashing a tree. The modes share one walk and one staging rule.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::infra::io::{digest_file, hash_and_copy_reader};

use super::remove::remove_path;
use super::walk::{WalkErrors, walk};

/// What `copy_tree` does with a target that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMode {
    /// Replace the target whole. The new tree is staged first and the old target is restored on failure; on success the
    /// previous generation is removed, so a user folder is not left with a `.old` sibling.
    Replace,
    /// Replace the target and keep one previous generation as `<name>.old`. Used for a profile store, where the
    /// previous copy is the recovery point.
    ReplaceKeepPrevious,
    /// Keep existing target entries and create only the missing ones.
    FillMissing,
    /// Copy the source over the target: a same-named file is overwritten and a target entry the source does not name
    /// stays. A path that is a file on one side and a folder on the other stops the copy with a [`MergeConflict`].
    #[cfg_attr(
        not(windows),
        allow(
            dead_code,
            reason = "moving profile data back is a Windows overlay path; the tests still cover the merge"
        )
    )]
    Merge,
}

/// Whether a path is a file or a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::File => "file",
            Self::Folder => "folder",
        })
    }
}

/// A merge stopped because the source and the target disagree on whether a path is a file or a folder. Nothing is
/// deleted to make the two sides fit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot copy a {} over a {} at {}",
    source_kind,
    target_kind,
    path.display()
)]
pub struct MergeConflict {
    /// The path inside the target that has the other kind.
    pub path: PathBuf,
    /// What the source has at the path.
    pub source_kind: EntryKind,
    /// What the target has at the path.
    pub target_kind: EntryKind,
}

/// Why a tree copy failed.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// The file system refused a read or a write.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A merge would have to replace a file with a folder or a folder with a file.
    #[error(transparent)]
    Conflict(#[from] MergeConflict),
}

impl From<CopyError> for io::Error {
    fn from(error: CopyError) -> Self {
        match error {
            CopyError::Io(error) => error,
            CopyError::Conflict(conflict) => io::Error::new(io::ErrorKind::AlreadyExists, conflict),
        }
    }
}

/// Copies a file or a folder tree. A folder is walked with the shared walk, so every mode agrees on separators and on
/// following a symlinked folder.
///
/// `Replace` is transactional: the new tree is written next to the target, the old target is moved to `<name>.old`,
/// and only then does the new tree take its place. A failure leaves the old target untouched and keeps the previous
/// generation as `<name>.old`. `FillMissing` only creates what is absent, so it cannot lose an existing entry. `Merge`
/// overwrites same-named files and keeps every other target entry; a file/folder mismatch is a [`MergeConflict`].
pub fn copy_tree(source: &Path, target: &Path, mode: CopyMode) -> Result<(), CopyError> {
    match mode {
        CopyMode::FillMissing => copy_tree_fill(source, target),
        CopyMode::Replace => replace_tree(source, target, false),
        CopyMode::ReplaceKeepPrevious => replace_tree(source, target, true),
        CopyMode::Merge => merge_tree(source, target),
    }
}

/// Copies every file below `source` into `target` in one pass, hashing the bytes as they are written. Returns the path
/// relative to `source`, the bytes written and the SHA-256 of every file. The hash comes from the copy, so a source that
/// changes during the copy still matches its entry.
pub fn copy_tree_hashed<D: Digest>(
    source: &Path,
    target: &Path,
) -> io::Result<Vec<(String, u64, String)>> {
    fs::create_dir_all(target)?;
    let mut files = Vec::new();
    walk(source, WalkErrors::Fail, &mut |entry, relative, is_dir| {
        let to = target.join(relative);
        if is_dir {
            fs::create_dir_all(&to)?;
        } else {
            create_parent_dir(&to)?;
            let mut from = File::open(entry.path())?;
            let total = from.metadata()?.len();
            let (sha256, size) = hash_and_copy_reader::<D, _>(&mut from, &to, total, &mut |_| {})?;
            files.push((relative.to_string(), size, sha256));
        }
        Ok(())
    })?;
    Ok(files)
}

/// Hashes every file below `root` into one checksum: the `/`-separated relative paths and each file's own SHA-256 are
/// sorted by path and fed into a second SHA-256. A change of content, name or number of files changes the digest, while
/// the order in which the files are read does not. Returns the hex digest, the file count and the total byte count.
pub fn hash_tree(root: &Path) -> io::Result<(String, u32, u64)> {
    let mut files = BTreeMap::new();
    let mut bytes = 0u64;
    walk(root, WalkErrors::Fail, &mut |entry, relative, is_dir| {
        if is_dir {
            return Ok(());
        }
        let size = fs::metadata(entry.path())?.len();
        let digest = digest_file::<Sha256>(&entry.path(), &mut |_| {})?;
        files.insert(relative.to_string(), digest);
        bytes = bytes.saturating_add(size);
        Ok(())
    })?;
    let mut total = Sha256::new();
    for (path, hash) in &files {
        total.update(path.as_bytes());
        total.update([0]);
        total.update(hash.as_bytes());
        total.update([0]);
    }
    Ok((
        faster_hex::hex_string(&total.finalize()),
        files.len() as u32,
        bytes,
    ))
}

fn copy_tree_fill(source: &Path, target: &Path) -> Result<(), CopyError> {
    let meta = fs::metadata(source)?;
    if meta.is_dir() {
        copy_dir(source, target, CopyMode::FillMissing)?;
    } else if !target.exists() {
        create_parent_dir(target)?;
        fs::copy(source, target)?;
    }
    Ok(())
}

/// Creates the folder above `path`, when it has one, so a file can be written there.
fn create_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// A failed `Replace` copy cannot remove the old target: the new tree is staged first, then the old target becomes
/// `<name>.old`, and the staging tree is renamed into place. The old generation stays for the next read or a repair.
fn replace_tree(source: &Path, target: &Path, keep_previous: bool) -> Result<(), CopyError> {
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
fn merge_tree(source: &Path, target: &Path) -> Result<(), CopyError> {
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
        if had_target {
            let _ = fs::rename(&backup, target);
        }
        return Err(error);
    }
    if had_target && !keep_previous {
        let _ = remove_path(&backup);
    }
    Ok(())
}

/// Whether a copy in `mode` writes `to`: a replace always does, any other mode only fills a missing entry.
fn writes_entry(mode: CopyMode, to: &Path) -> bool {
    mode == CopyMode::Replace || !to.exists()
}

fn copy_dir(source: &Path, target: &Path, mode: CopyMode) -> io::Result<()> {
    fs::create_dir_all(target)?;
    walk(source, WalkErrors::Fail, &mut |entry, rel, is_dir| {
        let to = target.join(rel);
        if is_dir {
            if writes_entry(mode, &to) {
                fs::create_dir_all(&to)?;
            }
        } else if writes_entry(mode, &to) {
            #[cfg(test)]
            if injected_failure() {
                return Err(io::Error::other("injected copy failure"));
            }
            fs::copy(entry.path(), &to)?;
        }
        Ok(())
    })
}

#[cfg(test)]
thread_local! {
    /// A test-only countdown: the next N file copies succeed, then the next one fails. Tests use it to prove that a
    /// failed replace leaves the old target and its previous generation intact.
    static FAIL_AFTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Makes the next `files` copies inside a copied folder fail after that many successes. Test-only.
#[cfg(test)]
pub fn fail_after(files: usize) {
    FAIL_AFTER.with(|cell| cell.set(files));
}

#[cfg(test)]
fn injected_failure() -> bool {
    FAIL_AFTER.with(|cell| {
        let left = cell.get();
        if left == 0 {
            return false;
        }
        cell.set(left - 1);
        left == 1
    })
}

/// Copies `source` over `target`, or removes `target` when the source is gone and `remove_missing` is set.
pub fn sync_copy(source: &Path, target: &Path, remove_missing: bool) -> io::Result<()> {
    if source.exists() {
        copy_tree(source, target, CopyMode::Replace).map_err(io::Error::from)
    } else if remove_missing {
        remove_path(target)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::io::digest_file;
    use crate::test_support::scratch;
    use sha2::Sha256;

    #[test]
    fn copy_tree_replaces_or_fills_the_target() {
        let dir = scratch!("copy-tree");
        let source = dir.join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("a.txt"), "source-a").unwrap();
        fs::write(source.join("nested/b.txt"), "source-b").unwrap();

        let replaced = dir.join("replaced");
        fs::create_dir_all(&replaced).unwrap();
        fs::write(replaced.join("old.txt"), "old").unwrap();
        copy_tree(&source, &replaced, CopyMode::Replace).unwrap();
        assert!(!replaced.join("old.txt").exists());
        assert!(
            !dir.join("replaced.old").exists(),
            "a plain replace keeps no generation"
        );
        assert_eq!(
            fs::read_to_string(replaced.join("a.txt")).unwrap(),
            "source-a"
        );

        let filled = dir.join("filled");
        fs::create_dir_all(&filled).unwrap();
        fs::write(filled.join("a.txt"), "kept").unwrap();
        copy_tree(&source, &filled, CopyMode::FillMissing).unwrap();
        assert_eq!(fs::read_to_string(filled.join("a.txt")).unwrap(), "kept");
        assert_eq!(
            fs::read_to_string(filled.join("nested/b.txt")).unwrap(),
            "source-b"
        );
    }

    #[test]
    fn a_merge_overwrites_same_names_and_keeps_every_other_target_entry() {
        let dir = scratch!("copy-merge");
        let source = dir.join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("a.txt"), "profile-a").unwrap();
        fs::write(source.join("nested/b.txt"), "profile-b").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(target.join("nested")).unwrap();
        fs::write(target.join("a.txt"), "real-a").unwrap();
        fs::write(target.join("nested/kept.txt"), "real-kept").unwrap();
        fs::write(target.join("keep.txt"), "real-only").unwrap();

        copy_tree(&source, &target, CopyMode::Merge).unwrap();

        assert_eq!(
            fs::read_to_string(target.join("a.txt")).unwrap(),
            "profile-a"
        );
        assert_eq!(
            fs::read_to_string(target.join("nested/b.txt")).unwrap(),
            "profile-b"
        );
        assert_eq!(
            fs::read_to_string(target.join("nested/kept.txt")).unwrap(),
            "real-kept"
        );
        assert_eq!(
            fs::read_to_string(target.join("keep.txt")).unwrap(),
            "real-only",
            "a merge never deletes a target entry the source does not name"
        );
    }

    #[test]
    fn a_merge_creates_its_target_and_its_nested_folders() {
        let dir = scratch!("copy-merge-new");
        let source = dir.join("source");
        fs::create_dir_all(source.join("a/b")).unwrap();
        fs::write(source.join("a/b/file.txt"), "new").unwrap();

        copy_tree(&source, &dir.join("target"), CopyMode::Merge).unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("target/a/b/file.txt")).unwrap(),
            "new"
        );
    }

    /// Unwraps the conflict of a merge that must refuse a file/folder mismatch.
    fn merge_conflict(source: &Path, target: &Path) -> MergeConflict {
        match copy_tree(source, target, CopyMode::Merge).unwrap_err() {
            CopyError::Conflict(conflict) => conflict,
            CopyError::Io(error) => panic!("expected a conflict, got {error}"),
        }
    }

    #[test]
    fn a_merge_refuses_a_file_where_the_target_has_a_folder() {
        let dir = scratch!("copy-merge-file-folder");
        let source = dir.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("shared"), "source-file").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(target.join("shared")).unwrap();
        fs::write(target.join("shared/inside.txt"), "real-inside").unwrap();

        let conflict = merge_conflict(&source, &target);

        assert_eq!(conflict.path, target.join("shared"));
        assert_eq!(conflict.source_kind, EntryKind::File);
        assert_eq!(conflict.target_kind, EntryKind::Folder);
        assert!(target.join("shared").is_dir(), "the folder stays usable");
        assert_eq!(
            fs::read_to_string(target.join("shared/inside.txt")).unwrap(),
            "real-inside"
        );
        assert_eq!(
            fs::read_to_string(source.join("shared")).unwrap(),
            "source-file"
        );
    }

    #[test]
    fn a_merge_refuses_a_folder_where_the_target_has_a_file() {
        let dir = scratch!("copy-merge-folder-file");
        let source = dir.join("source");
        fs::create_dir_all(source.join("shared")).unwrap();
        fs::write(source.join("shared/inside.txt"), "profile-inside").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("shared"), "real-file").unwrap();

        let conflict = merge_conflict(&source, &target);

        assert_eq!(conflict.path, target.join("shared"));
        assert_eq!(conflict.source_kind, EntryKind::Folder);
        assert_eq!(conflict.target_kind, EntryKind::File);
        assert_eq!(
            fs::read_to_string(target.join("shared")).unwrap(),
            "real-file"
        );
    }

    #[test]
    fn a_failed_merge_stops_before_the_first_write_and_the_target_stays_usable() {
        let dir = scratch!("copy-merge-conflict-early");
        let source = dir.join("source");
        fs::create_dir_all(source.join("z/child")).unwrap();
        fs::write(source.join("a.txt"), "profile").unwrap();
        fs::write(source.join("z/child/file.txt"), "profile").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(target.join("z")).unwrap();
        fs::write(target.join("z/child"), "real-file").unwrap();
        fs::write(target.join("keep.txt"), "keep").unwrap();

        // The mismatch is deep in the tree; the check still runs before the first write.
        let conflict = merge_conflict(&source, &target);
        assert_eq!(conflict.path, target.join("z/child"));
        assert_eq!(conflict.source_kind, EntryKind::Folder);
        assert_eq!(conflict.target_kind, EntryKind::File);

        assert_eq!(
            fs::read_to_string(target.join("z/child")).unwrap(),
            "real-file"
        );
        assert!(!target.join("a.txt").exists());
        assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
    }

    #[test]
    fn a_merge_that_fails_midway_leaves_every_written_file_readable() {
        let dir = scratch!("copy-merge-fail");
        let source = dir.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.txt"), "profile-a").unwrap();
        fs::write(source.join("b.txt"), "profile-b").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep.txt"), "keep").unwrap();

        fail_after(2);
        let error = copy_tree(&source, &target, CopyMode::Merge).unwrap_err();
        fail_after(0);
        assert!(error.to_string().contains("injected"));
        assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
        // A merge is not staged: one file may be there already and the other not, but every written file is complete.
        let written: Vec<_> = ["a.txt", "b.txt"]
            .into_iter()
            .filter(|name| target.join(name).is_file())
            .map(|name| fs::read_to_string(target.join(name)).unwrap())
            .collect();
        assert_eq!(written.len(), 1, "only the file before the failure");
        assert!(
            written[0] == "profile-a" || written[0] == "profile-b",
            "the written file is complete"
        );

        copy_tree(&source, &target, CopyMode::Merge).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("b.txt")).unwrap(),
            "profile-b"
        );
    }

    #[test]
    fn a_tree_is_copied_and_hashed_in_one_pass() {
        let dir = scratch!("copy-tree-hashed");
        let source = dir.join("source");
        fs::create_dir_all(source.join("nested/empty")).unwrap();
        fs::write(source.join("a.txt"), "abc").unwrap();
        fs::write(source.join("nested/b.bin"), "payload").unwrap();
        let target = dir.join("target");

        let files = copy_tree_hashed::<Sha256>(&source, &target).unwrap();
        let mut names: Vec<_> = files
            .iter()
            .map(|(path, size, _)| (path.as_str(), *size))
            .collect();
        names.sort_unstable();
        assert_eq!(names, [("a.txt", 3), ("nested/b.bin", 7)]);

        for (relative, size, sha256) in &files {
            let copy = target.join(relative);
            assert_eq!(fs::metadata(&copy).unwrap().len(), *size);
            assert_eq!(&digest_file::<Sha256>(&copy, &mut |_| {}).unwrap(), sha256);
        }
        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "abc");
        assert!(target.join("nested/empty").is_dir());
    }

    #[test]
    fn a_staged_tree_replaces_the_target_and_keeps_one_generation() {
        let dir = scratch!("replace-staged");
        let staging = dir.join("staging");
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("new.txt"), "new").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("old.txt"), "old").unwrap();

        replace_with_staged(&staging, &target, true).unwrap();
        assert!(!staging.exists(), "the staged tree becomes the target");
        assert_eq!(fs::read_to_string(target.join("new.txt")).unwrap(), "new");
        assert!(!target.join("old.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.join("target.old/old.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn sync_copy_removes_the_target_when_the_source_is_gone() {
        let dir = scratch!("sync-copy");
        let source = dir.join("source");
        let target = dir.join("target");
        fs::write(&target, "old").unwrap();

        sync_copy(&source, &target, true).unwrap();
        assert!(!target.exists());

        fs::write(&target, "old").unwrap();
        sync_copy(&source, &target, false).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    }

    #[test]
    fn a_failed_replace_leaves_the_old_target_untouched() {
        let dir = scratch!("copy-fail");
        let source = dir.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.txt"), "new-a").unwrap();
        fs::write(source.join("b.txt"), "new-b").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("old.txt"), "old").unwrap();

        for countdown in 1..=2 {
            fail_after(countdown);
            let error = copy_tree(&source, &target, CopyMode::Replace).unwrap_err();
            fail_after(0);
            assert!(error.to_string().contains("injected"));
            assert_eq!(fs::read_to_string(target.join("old.txt")).unwrap(), "old");
            assert!(!target.join("a.txt").exists());
            assert!(!target.join("b.txt").exists());
            assert!(!dir.join("target.old").exists());
            let staging: Vec<_> = fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_name().to_string_lossy().contains("staging")
                        || entry.file_name().to_string_lossy().ends_with(".tmp")
                })
                .collect();
            assert!(staging.is_empty(), "the staging tree was not cleaned up");
        }

        copy_tree(&source, &target, CopyMode::Replace).unwrap();
        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new-a");
        assert_eq!(fs::read_to_string(target.join("b.txt")).unwrap(), "new-b");
    }

    #[test]
    fn a_successful_replace_keeps_one_previous_generation() {
        let dir = scratch!("copy-old");
        let source = dir.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("new.txt"), "first").unwrap();
        let target = dir.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("old.txt"), "old").unwrap();

        copy_tree(&source, &target, CopyMode::ReplaceKeepPrevious).unwrap();
        assert_eq!(fs::read_to_string(target.join("new.txt")).unwrap(), "first");
        assert!(!target.join("old.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.join("target.old/old.txt")).unwrap(),
            "old"
        );

        fs::write(source.join("new.txt"), "second").unwrap();
        copy_tree(&source, &target, CopyMode::ReplaceKeepPrevious).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("new.txt")).unwrap(),
            "second"
        );
        assert_eq!(
            fs::read_to_string(dir.join("target.old/new.txt")).unwrap(),
            "first"
        );
        let generations: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("target.old")
            })
            .collect();
        assert_eq!(
            generations.len(),
            1,
            "the previous generation must not accumulate"
        );
    }

    #[test]
    fn a_tree_checksum_changes_when_one_file_changes_without_changing_its_size() {
        let dir = scratch!("hash-tree");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/a.txt"), "one").unwrap();
        fs::write(dir.join("b.txt"), "two").unwrap();
        let first = hash_tree(&dir).unwrap();
        assert_eq!(first.1, 2);
        assert_eq!(first.2, 6);

        // Same length, different content: a size-only report would miss this.
        fs::write(dir.join("sub/a.txt"), "ONE").unwrap();
        let changed = hash_tree(&dir).unwrap();
        assert_ne!(changed.0, first.0);

        // A rename changes the checksum too, while a reread of the same bytes does not.
        fs::write(dir.join("sub/a.txt"), "one").unwrap();
        assert_eq!(hash_tree(&dir).unwrap().0, first.0);
        fs::rename(dir.join("sub/a.txt"), dir.join("sub/c.txt")).unwrap();
        assert_ne!(hash_tree(&dir).unwrap().0, first.0);
    }
}
