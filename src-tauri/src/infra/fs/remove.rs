//! Removing files and folders when a path may already be gone.

use std::fs;
use std::io;
use std::path::Path;

/// Removes a file, or does nothing when it is already gone.
pub fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    ignore_not_found(fs::remove_file(path))
}

/// Removes a folder and everything below it, or does nothing when it is already gone.
pub fn remove_dir_if_exists(path: &Path) -> io::Result<()> {
    ignore_not_found(fs::remove_dir_all(path))
}

/// Treats a missing path as already removed.
pub fn ignore_not_found(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// Removes a file or a folder, whichever the path is, and treats a missing path as already removed. A symlink is
/// removed itself, not the file or folder it points at.
pub fn remove_path(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn remove_helpers_treat_a_missing_path_as_removed() {
        let dir = scratch!("remove-helpers");
        let file = dir.join("file.txt");
        let folder = dir.join("folder");
        fs::write(&file, "x").unwrap();
        fs::create_dir_all(folder.join("inner")).unwrap();

        remove_file_if_exists(&file).unwrap();
        remove_dir_if_exists(&folder).unwrap();
        remove_path(&dir.join("missing")).unwrap();
        assert!(!file.exists());
        assert!(!folder.exists());
        assert!(!dir.join("missing").exists());
    }
}
