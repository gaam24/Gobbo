//! Replaces the program files of a portable copy. Every old file is moved aside before any new file is written, so a
//! failure part way puts the old copy back. The moved-aside files stay until the next start, when the old program no
//! longer runs, and `cleanup_leftovers` removes them.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, io_at};
use crate::update::archive::is_program_file;

/// The suffix of a file moved aside by an update. Only files with it are removed at start.
pub const OLD_SUFFIX: &str = ".gobbo-old";
/// The folder inside the program folder where an update unpacks before it swaps.
pub const STAGING_DIR: &str = ".gobbo-update";

/// The path of a file moved aside: the same name with `OLD_SUFFIX` appended.
pub fn aside(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(OLD_SUFFIX);
    PathBuf::from(name)
}

/// Copies every program file of `staged` over the one in `program_dir`. Returns how many files were replaced. A failure
/// removes the new files written so far and moves the old ones back.
pub fn replace_program_files(program_dir: &Path, staged: &Path) -> Result<usize, AppError> {
    let files: Vec<PathBuf> = relative_files(staged)?
        .into_iter()
        .filter(|relative| is_program_file(relative))
        .collect();
    let mut moved: Vec<PathBuf> = Vec::new();
    let mut written: Vec<PathBuf> = Vec::new();
    let result = swap(program_dir, staged, &files, &mut moved, &mut written);
    match result {
        Ok(()) => Ok(files.len()),
        Err(error) => {
            for path in &written {
                let _ = fs::remove_file(path);
            }
            for path in &moved {
                let _ = fs::rename(aside(path), path);
            }
            Err(error)
        }
    }
}

fn swap(
    program_dir: &Path,
    staged: &Path,
    files: &[PathBuf],
    moved: &mut Vec<PathBuf>,
    written: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    for relative in files {
        let target = program_dir.join(relative);
        if target.exists() {
            let old = aside(&target);
            if old.exists() {
                fs::remove_file(&old).map_err(io_at(&old))?;
            }
            fs::rename(&target, &old).map_err(io_at(&target))?;
            moved.push(target);
        }
    }
    for relative in files {
        let target = program_dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }
        fs::copy(staged.join(relative), &target).map_err(io_at(&target))?;
        written.push(target);
    }
    Ok(())
}

/// Every file under `root`, as a path relative to it.
fn relative_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(io_at(&dir))? {
            let path = entry.map_err(io_at(&dir))?.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                found.push(relative.to_path_buf());
            }
        }
    }
    Ok(found)
}

/// Removes what an update left: the unpacking folder and the files moved aside. The data folder is never searched.
pub fn cleanup_leftovers(program_dir: &Path) {
    let _ = fs::remove_dir_all(program_dir.join(STAGING_DIR));
    let Ok(entries) = fs::read_dir(program_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "data") {
            continue;
        }
        if path.is_dir() {
            cleanup_under(&path);
        } else if is_leftover(&path) {
            let _ = fs::remove_file(&path);
        }
    }
}

fn cleanup_under(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup_under(&path);
        } else if is_leftover(&path) {
            let _ = fs::remove_file(&path);
        }
    }
}

fn is_leftover(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(OLD_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, text).expect("write");
    }

    #[test]
    fn new_program_files_replace_old_ones_and_the_data_folder_stays() {
        let dir = tempfile::tempdir().expect("tempdir");
        let program = dir.path().join("program");
        let staged = dir.path().join("staged");
        write(&program.join("gobbo"), "old");
        write(&program.join("games/a/game.toml"), "old definition");
        write(&program.join("data/library.json"), "user library");
        write(&staged.join("gobbo"), "new");
        write(&staged.join("games/a/game.toml"), "new definition");
        write(&staged.join("games/b/game.toml"), "added definition");

        let replaced = replace_program_files(&program, &staged).expect("swap");

        assert_eq!(replaced, 3);
        assert_eq!(
            fs::read_to_string(program.join("gobbo")).expect("exe"),
            "new"
        );
        assert_eq!(
            fs::read_to_string(program.join("games/a/game.toml")).expect("def"),
            "new definition"
        );
        assert_eq!(
            fs::read_to_string(program.join("games/b/game.toml")).expect("added"),
            "added definition"
        );
        assert_eq!(
            fs::read_to_string(aside(&program.join("gobbo"))).expect("old copy"),
            "old"
        );
        assert_eq!(
            fs::read_to_string(program.join("data/library.json")).expect("data"),
            "user library"
        );
    }

    #[test]
    fn a_failure_part_way_puts_the_old_copy_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let program = dir.path().join("program");
        let staged = dir.path().join("staged");
        write(&program.join("gobbo"), "old");
        write(&staged.join("gobbo"), "new");
        // `blocked` is a file in the program folder, so the copy of `blocked/inner` cannot create its folder.
        write(&program.join("blocked"), "a file, not a folder");
        write(&staged.join("blocked/inner"), "new inner");

        replace_program_files(&program, &staged).expect_err("the copy fails");
        assert_eq!(
            fs::read_to_string(program.join("gobbo")).expect("restored"),
            "old"
        );
        assert!(!aside(&program.join("gobbo")).exists());
        assert_eq!(
            fs::read_to_string(program.join("blocked")).expect("untouched"),
            "a file, not a folder"
        );
    }

    #[test]
    fn cleanup_removes_the_leftovers_and_keeps_the_data_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let program = dir.path();
        write(&program.join("gobbo.exe.gobbo-old"), "old");
        write(&program.join("games/a/game.toml.gobbo-old"), "old");
        write(&program.join("games/a/game.toml"), "current");
        write(&program.join(STAGING_DIR).join("gobbo"), "unpacked");
        write(&program.join("data/keep.gobbo-old"), "user file");

        cleanup_leftovers(program);

        assert!(!program.join("gobbo.exe.gobbo-old").exists());
        assert!(!program.join("games/a/game.toml.gobbo-old").exists());
        assert!(!program.join(STAGING_DIR).exists());
        assert_eq!(
            fs::read_to_string(program.join("games/a/game.toml")).expect("kept"),
            "current"
        );
        assert!(program.join("data/keep.gobbo-old").exists());
    }
}
