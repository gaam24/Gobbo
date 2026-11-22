//! Unpacks the portable archive into a staging folder. The archive wraps its files in one top folder, which is dropped;
//! the data folder is never taken from an archive, so an update cannot change a user's library or settings.

use std::fs;
use std::io::{self, Cursor};
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, io_at};
use crate::infra::storage::is_portable_data_dir;

/// Whether an archive entry (below the top folder) is a program file. The data folder is the user's, not the program's.
pub fn is_program_file(relative: &Path) -> bool {
    match relative.components().next() {
        Some(Component::Normal(first)) => !first.to_str().is_some_and(is_portable_data_dir),
        _ => false,
    }
}

/// The path of an entry below the archive's top folder. `None` when the entry has no such folder, such as a file at the
/// root of the archive.
fn below_top_folder(entry: &Path) -> Option<PathBuf> {
    let mut parts = entry.components();

    match parts.next()? {
        Component::Normal(_) => {}
        _ => return None,
    }

    let rest: PathBuf = parts.collect();

    if rest.as_os_str().is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// Writes the program files of `bytes` under `dest`, without the top folder. An entry that would leave `dest`, or a
/// file outside the top folder, refuses the whole archive.
pub fn extract(bytes: &[u8], dest: &Path) -> Result<(), AppError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(invalid)?;
        let name = file
            .enclosed_name()
            .ok_or_else(|| invalid("an entry leaves the archive folder"))?;
        if file.is_dir() {
            continue;
        }

        let rest = below_top_folder(&name)
            .ok_or_else(|| invalid("a file is not inside the top folder"))?;
        if !is_program_file(&rest) {
            continue;
        }

        let target = dest.join(&rest);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }

        let mut out = fs::File::create(&target).map_err(io_at(&target))?;
        io::copy(&mut file, &mut out).map_err(io_at(&target))?;
    }

    Ok(())
}

fn invalid(detail: impl ToString) -> AppError {
    AppError::user("update.archive_invalid").with("detail", detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);

            for (name, body) in entries {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .expect("start");
                writer.write_all(body.as_bytes()).expect("write");
            }

            writer.finish().expect("finish");
        }

        buffer.into_inner()
    }

    #[test]
    fn the_top_folder_is_dropped_and_the_data_folder_is_skipped() {
        assert_eq!(
            below_top_folder(Path::new("gobbo-0.1.0/gobbo.exe")),
            Some(PathBuf::from("gobbo.exe"))
        );
        assert_eq!(
            below_top_folder(Path::new("gobbo-0.1.0/games/a/game.toml")),
            Some(PathBuf::from("games/a/game.toml"))
        );
        assert_eq!(below_top_folder(Path::new("gobbo.exe")), None);
        assert!(is_program_file(Path::new("games/a/game.toml")));
        assert!(is_program_file(Path::new("gobbo.exe")));
        assert!(!is_program_file(Path::new("data/library.json")));
        assert!(!is_program_file(Path::new("../escape")));
    }

    #[test]
    fn extract_writes_program_files_and_ignores_the_data_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = archive(&[
            ("gobbo-0.1.0-linux-x64-portable/gobbo", "new program"),
            (
                "gobbo-0.1.0-linux-x64-portable/games/a/game.toml",
                "definition",
            ),
            (
                "gobbo-0.1.0-linux-x64-portable/data/library.json",
                "from the archive",
            ),
        ]);
        extract(&bytes, dir.path()).expect("extract");
        assert_eq!(
            fs::read_to_string(dir.path().join("gobbo")).expect("program"),
            "new program"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("games/a/game.toml")).expect("definition"),
            "definition"
        );
        assert!(!dir.path().join("data").exists());
    }

    #[test]
    fn an_entry_that_escapes_the_folder_refuses_the_archive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = archive(&[("top/../../escape.txt", "bad")]);
        let error = extract(&bytes, dir.path()).expect_err("refused");
        assert_eq!(error.code, "update.archive_invalid");
        assert!(
            !dir.path()
                .parent()
                .expect("parent")
                .join("escape.txt")
                .exists()
        );
    }

    #[test]
    fn a_file_at_the_root_refuses_the_archive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = archive(&[("loose.txt", "no folder")]);
        assert_eq!(
            extract(&bytes, dir.path()).expect_err("refused").code,
            "update.archive_invalid"
        );
    }
}
