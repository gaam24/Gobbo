//! The shared tree walk and the listings built on it.

use std::collections::BTreeMap;
use std::fs::{self, DirEntry};
use std::io;
use std::path::Path;

/// What a walk does with an entry or a folder it cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkErrors {
    /// Stop the walk and give the error back. The caller adds its own path to the message.
    Fail,
    /// Skip the entry or the folder. The game folder may lose a file while the game writes it; that is not a failure
    /// of the snapshot.
    Skip,
}

/// Calls `visit` for every entry below `dir`, depth first, with the path relative to `dir` and `/` as the separator.
/// The visitor gets the directory entry and whether it is a folder; the walk recurses into a folder after the visit.
pub fn walk(
    dir: &Path,
    errors: WalkErrors,
    visit: &mut impl FnMut(&DirEntry, &str, bool) -> io::Result<()>,
) -> io::Result<()> {
    let mut relative = String::new();
    walk_inner(dir, &mut relative, errors, visit)
}

/// The relative path is one buffer reused by the whole walk. A large game folder used to allocate a new `String`
/// (and a `format!`) for every entry.
fn walk_inner(
    dir: &Path,
    relative: &mut String,
    errors: WalkErrors,
    visit: &mut impl FnMut(&DirEntry, &str, bool) -> io::Result<()>,
) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => return skip_or_fail(errors, error),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                skip_or_fail(errors, error)?;
                continue;
            }
        };
        let base = relative.len();

        if base > 0 {
            relative.push('/');
        }

        relative.push_str(&entry.file_name().to_string_lossy());
        // `file_type` comes from the directory entry; a symlink or an unknown type needs the path check.
        let is_dir = match entry.file_type() {
            Ok(kind) if !kind.is_symlink() => kind.is_dir(),
            _ => entry.path().is_dir(),
        };

        if let Err(error) = visit(&entry, relative.as_str(), is_dir) {
            skip_or_fail(errors, error)?;
        }

        if is_dir && let Err(error) = walk_inner(&entry.path(), relative, errors, visit) {
            skip_or_fail(errors, error)?;
        }

        relative.truncate(base);
    }

    Ok(())
}

/// Paths of all files below `dir`, relative to it, with `/` as the separator, sorted. Only the names are read: a caller
/// that does not need the sizes skips one metadata call per file.
pub fn list_files(dir: &Path) -> io::Result<Vec<String>> {
    let mut files = Vec::new();
    walk(dir, WalkErrors::Fail, &mut |_, relative, is_dir| {
        if !is_dir {
            files.push(relative.to_string());
        }

        Ok(())
    })?;
    files.sort_unstable();
    Ok(files)
}

/// The total size of the files below `dir`. A folder or file that cannot be read is skipped, so a game folder that is
/// changing while its game runs still gives a usable number.
pub fn folder_size(dir: &Path) -> u64 {
    let mut bytes = 0_u64;
    let _ = walk(dir, WalkErrors::Skip, &mut |entry, _, is_dir| {
        if !is_dir {
            bytes = bytes.saturating_add(entry.metadata().map_or(0, |meta| meta.len()));
        }

        Ok(())
    });
    bytes
}

/// Paths of all files below `dir` and their total size, in one walk, sorted.
pub fn list_files_with_sizes(dir: &Path) -> io::Result<(Vec<String>, u64)> {
    let mut files = Vec::new();
    let mut bytes = 0_u64;
    walk(dir, WalkErrors::Fail, &mut |entry, relative, is_dir| {
        if !is_dir {
            bytes = bytes.saturating_add(entry.metadata().map_or(0, |meta| meta.len()));
            files.push(relative.to_string());
        }

        Ok(())
    })?;
    files.sort_unstable();
    Ok((files, bytes))
}

/// The files of a folder and their sizes, keyed by path relative to it. One walk gives both the names and the sizes,
/// so a caller comparing sizes does not stat every file again.
pub fn file_sizes(dir: &Path) -> io::Result<BTreeMap<String, u64>> {
    let mut files = BTreeMap::new();
    walk(dir, WalkErrors::Fail, &mut |entry, relative, is_dir| {
        if !is_dir {
            files.insert(
                relative.to_string(),
                entry.metadata().map_or(0, |meta| meta.len()),
            );
        }

        Ok(())
    })?;
    Ok(files)
}

fn skip_or_fail(errors: WalkErrors, error: io::Error) -> io::Result<()> {
    match errors {
        WalkErrors::Fail => Err(error),
        WalkErrors::Skip => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::collections::BTreeSet;

    #[test]
    fn a_walk_visits_folders_and_files_with_slashes() {
        let dir = scratch!("walk");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/c.txt"), "x").unwrap();
        fs::write(dir.join("top.txt"), "x").unwrap();

        let mut seen = BTreeSet::new();
        walk(&dir, WalkErrors::Fail, &mut |_, rel, is_dir| {
            seen.insert((rel.to_string(), is_dir));
            Ok(())
        })
        .unwrap();

        assert_eq!(
            seen,
            BTreeSet::from([
                ("a".to_string(), true),
                ("a/b".to_string(), true),
                ("a/b/c.txt".to_string(), false),
                ("top.txt".to_string(), false),
            ])
        );
    }

    #[test]
    fn a_missing_folder_fails_or_is_skipped() {
        let missing = scratch!("walk-missing").join("gone");
        assert!(walk(&missing, WalkErrors::Fail, &mut |_, _, _| Ok(())).is_err());
        assert!(walk(&missing, WalkErrors::Skip, &mut |_, _, _| Ok(())).is_ok());
    }

    #[test]
    fn a_folder_size_adds_the_files_below_it() {
        let dir = scratch!("folder-size");
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("top.txt"), b"1234").unwrap();
        fs::write(dir.join("nested/file.txt"), b"123456").unwrap();

        assert_eq!(folder_size(&dir), 10);
        assert_eq!(folder_size(&dir.join("missing")), 0);
    }

    #[test]
    fn a_names_only_listing_matches_the_sized_one() {
        let dir = scratch!("list-file-names");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("top.txt"), b"1234").unwrap();
        fs::write(dir.join("a/b/c.txt"), b"x").unwrap();

        let names = list_files(&dir).unwrap();
        assert_eq!(names, ["a/b/c.txt".to_string(), "top.txt".to_string()]);
        let (sized, bytes) = list_files_with_sizes(&dir).unwrap();
        assert_eq!(names, sized);
        assert_eq!(bytes, 5);
        let per_file = file_sizes(&dir).unwrap();
        assert_eq!(per_file.keys().cloned().collect::<Vec<_>>(), sized);
        assert_eq!(per_file["top.txt"], 4);
        assert_eq!(per_file["a/b/c.txt"], 1);

        // A name whose metadata fails is still a name: the names-only walk never asks for its size.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("missing"), dir.join("broken")).unwrap();
            let names = list_files(&dir).unwrap();
            assert!(names.contains(&"broken".to_string()));
            assert_eq!(names, list_files_with_sizes(&dir).unwrap().0);
            assert!(file_sizes(&dir).unwrap().contains_key("broken"));
        }
    }
}
