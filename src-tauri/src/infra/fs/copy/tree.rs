//! Copying a tree in one pass, optionally hashing the bytes as they are written.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::infra::fs::walk::{WalkErrors, walk};
use crate::infra::io::{digest_file, hash_and_copy_reader};

use super::merge::{merge_tree, replace_tree};
use super::model::{CopyError, CopyMode};
use super::sync::copy_dir;

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
pub(super) fn create_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(())
}
