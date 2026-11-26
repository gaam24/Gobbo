//! Unpacks the portable archive into a staging folder. The archive wraps its files in one top folder, which is dropped;
//! the data folder is never taken from an archive, so an update cannot change a user's library or settings.

use std::fs;
use std::io::{self, Cursor};
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, io_at};
use crate::infra::storage::is_portable_data_dir;

/// The most entries a portable update archive may list. A release lists far fewer; a list this large is a mistake or a
/// tampered archive, and it is refused before anything is written to the staging folder.
const MAX_UPDATE_ENTRIES: usize = 100_000;

/// The most bytes the entries of a portable update archive may expand to. A release is far smaller; a larger archive is
/// refused before it can fill the disk.
const MAX_UPDATE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

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

/// Writes the program files of `bytes` under `dest`, without the top folder. An entry that would leave `dest`, a file
/// outside the top folder, or an archive over the entry or byte cap refuses the whole archive before any file is written.
pub fn extract(bytes: &[u8], dest: &Path) -> Result<(), AppError> {
    extract_limited(bytes, dest, MAX_UPDATE_ENTRIES, MAX_UPDATE_BYTES)
}

/// Writes the program files under the given caps. The limits are parameters so a test can reach both checks with a
/// small archive instead of a 100,000-entry or 8 GiB fixture.
fn extract_limited(
    bytes: &[u8],
    dest: &Path,
    count_limit: usize,
    byte_limit: u64,
) -> Result<(), AppError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;
    let count = archive.len();

    // Refuse before allocating a size list for a header that claims a huge number of entries.
    if count > count_limit {
        return Err(too_many_entries(count, count_limit));
    }

    let mut sizes = Vec::with_capacity(count);
    for index in 0..count {
        sizes.push(archive.by_index(index).map_err(invalid)?.size());
    }
    limits_ok(count, &sizes, count_limit, byte_limit)?;

    let mut copied = 0u64;
    for index in 0..count {
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
        let written = io::copy(&mut file, &mut out).map_err(io_at(&target))?;
        copied = add_copied(copied, written, byte_limit)?;
    }

    Ok(())
}

/// Checks the declared entry count and the sum of the declared uncompressed sizes. The sum saturates, so a header that
/// declares very large sizes cannot wrap into a small total.
fn limits_ok(
    count: usize,
    sizes: &[u64],
    count_limit: usize,
    byte_limit: u64,
) -> Result<(), AppError> {
    if count > count_limit {
        return Err(too_many_entries(count, count_limit));
    }

    let bytes = sizes
        .iter()
        .fold(0u64, |total, &size| total.saturating_add(size));
    if bytes > byte_limit {
        return Err(too_large(bytes, byte_limit));
    }

    Ok(())
}

/// Adds the bytes `io::copy` actually wrote to the running total, refusing when the cap is crossed. The central
/// directory is not trusted on its own for the copied bytes; the sum saturates so a wrong count cannot wrap.
fn add_copied(total: u64, written: u64, limit: u64) -> Result<u64, AppError> {
    let total = total.saturating_add(written);
    if total > limit {
        return Err(too_large(total, limit));
    }
    Ok(total)
}

fn too_many_entries(count: usize, limit: usize) -> AppError {
    AppError::new("update.archive_too_many_entries")
        .with("count", count)
        .with("limit", limit)
}

fn too_large(bytes: u64, limit: u64) -> AppError {
    AppError::new("update.archive_too_large")
        .with("bytes", bytes)
        .with("limit", limit)
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

    #[test]
    fn small_archives_pass_the_caps() {
        assert!(limits_ok(2, &[4, 6], 2, 10).is_ok());
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = archive(&[("top/a.txt", "ok")]);
        extract_limited(&bytes, dir.path(), 1, 2).expect("extract");
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).expect("file"),
            "ok"
        );
    }

    #[test]
    fn declared_sizes_over_the_caps_are_refused_and_sum_saturates() {
        assert_eq!(
            limits_ok(3, &[1, 2], 2, 10).expect_err("count").code,
            "update.archive_too_many_entries"
        );
        assert_eq!(
            limits_ok(1, &[11], 2, 10).expect_err("bytes").code,
            "update.archive_too_large"
        );

        let saturated = limits_ok(2, &[u64::MAX, 1], MAX_UPDATE_ENTRIES, MAX_UPDATE_BYTES)
            .expect_err("saturated");
        assert_eq!(saturated.code, "update.archive_too_large");
        assert_eq!(
            saturated.params.get("bytes").map(String::as_str),
            Some("18446744073709551615")
        );
    }

    #[test]
    fn an_archive_over_the_byte_cap_is_refused_before_writing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = archive(&[("top/a.txt", "too big")]);
        let error = extract_limited(&bytes, dir.path(), 10, 3).expect_err("refused");
        assert_eq!(error.code, "update.archive_too_large");
        assert_eq!(error.params.get("bytes").map(String::as_str), Some("7"));
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn an_archive_over_the_entry_cap_is_refused_before_writing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = archive(&[("top/a.txt", "a"), ("top/b.txt", "b"), ("top/c.txt", "c")]);
        let error = extract_limited(&bytes, dir.path(), 2, MAX_UPDATE_BYTES).expect_err("refused");
        assert_eq!(error.code, "update.archive_too_many_entries");
        assert_eq!(error.params.get("count").map(String::as_str), Some("3"));
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn copied_bytes_stop_at_the_cap_and_cannot_wrap() {
        assert_eq!(add_copied(2, 1, 3).expect("within"), 3);
        assert_eq!(
            add_copied(3, 1, 3).expect_err("over").code,
            "update.archive_too_large"
        );
        assert_eq!(
            add_copied(u64::MAX, 1, MAX_UPDATE_BYTES)
                .expect_err("saturated")
                .params
                .get("bytes")
                .map(String::as_str),
            Some("18446744073709551615")
        );
    }
}
