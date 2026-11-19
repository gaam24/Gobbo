//! JSON state files: one read policy and one write policy, shared by every saved value.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};

use crate::error::{AppError, io_at};

use super::store::{sibling_named, write_temp_atomic};

/// What can go wrong when a state file is read. The file missing is not an error: `read_json` reports it as `None`.
#[derive(Debug)]
pub enum ReadError {
    Io(std::io::Error),
    Parse(serde_json::Error),
}

/// Reads and parses one JSON state file. `Ok(None)` means the file does not exist. A read or parse failure is left to
/// the caller, because the state files do not all treat a damaged file the same way.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, ReadError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ReadError::Io(error)),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(ReadError::Parse)
}

/// Reads a JSON file, or `None` when it is missing, unreadable or does not parse. For files whose content is only a
/// cache or a diagnostic, where the caller has a useful default.
pub fn read_json_or_none<T: DeserializeOwned>(path: &Path) -> Option<T> {
    read_json(path).ok().flatten()
}

/// Reads a JSON file for settings. A missing file gives the default. A file that does not parse is kept: it is renamed to
/// `<name>.json.broken`, so the next save cannot overwrite what the user may still want to recover, and the default is
/// used with a line on stderr.
pub fn read_json_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    match read_json(path) {
        Ok(Some(value)) => value,
        Ok(None) | Err(ReadError::Io(_)) => T::default(),
        Err(ReadError::Parse(error)) => {
            keep_broken(path, &error);
            T::default()
        }
    }
}

/// Keeps a file that did not parse as `<name>.json.broken` and reports it.
pub fn keep_broken(path: &Path, error: &impl std::fmt::Display) {
    let broken = broken_path(path);
    let _ = fs::rename(path, &broken);
    crate::infra::log::warn(format!(
        "{} could not be read ({error}); it was kept as {}",
        path.display(),
        broken.display()
    ));
}

/// A name next to `path` for a file that did not parse. The first broken copy keeps the familiar `.json.broken`
/// name; later ones get a suffix, so a second broken file cannot overwrite the first.
pub fn broken_path(path: &Path) -> PathBuf {
    let first = path.with_extension("json.broken");
    if !first.exists() {
        return first;
    }
    sibling_named(path, |name| {
        format!("{name}.broken-{}", super::store::unique_suffix())
    })
}

/// Saves a value as compact JSON, through a temporary file and a rename, so an interrupted write does not spoil the old
/// file. Compact JSON is both faster to write and smaller to read than the pretty form; the files are state, not
/// hand-edited documents.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_at(parent))?;
    }
    write_temp_atomic(
        path,
        |file, tmp| {
            // `serde_json` writes many small pieces; an unbuffered `File` turned a 100k-path index into tens of
            // thousands of write syscalls. The buffer makes the save one pass through the file.
            let mut file = BufWriter::with_capacity(64 * 1024, file);
            serde_json::to_writer(&mut file, value).map_err(AppError::unexpected)?;
            file.flush().map_err(io_at(tmp))
        },
        |path, error| io_at(path)(error),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Sample {
        value: u32,
    }

    #[test]
    fn settings_are_read_or_defaulted_and_a_broken_file_is_kept_aside() {
        let dir = scratch!("read-json");
        let path = dir.join("settings.json");

        // A missing file is the default, and nothing is created.
        assert_eq!(read_json_or_default::<Sample>(&path), Sample::default());
        assert!(!path.exists());

        write_json(&path, &Sample { value: 7 }).unwrap();
        assert_eq!(read_json_or_default::<Sample>(&path), Sample { value: 7 });

        // A broken file is not overwritten by the next save: it is kept as `.json.broken`.
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(read_json_or_default::<Sample>(&path), Sample::default());
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(dir.join("settings.json.broken")).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn a_second_broken_file_does_not_overwrite_the_first_backup() {
        let dir = scratch!("read-json-twice");
        let path = dir.join("settings.json");
        fs::write(&path, "{ first").unwrap();
        assert_eq!(read_json_or_default::<Sample>(&path), Sample::default());
        fs::write(&path, "{ second").unwrap();
        assert_eq!(read_json_or_default::<Sample>(&path), Sample::default());
        assert_eq!(
            fs::read_to_string(dir.join("settings.json.broken")).unwrap(),
            "{ first"
        );
        let backups: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("settings.json.broken-"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(
            fs::read_to_string(dir.join(&backups[0])).unwrap(),
            "{ second"
        );
    }

    #[test]
    fn json_is_replaced_whole_and_no_tmp_file_is_left() {
        let dir = scratch!("json");
        let path = dir.join("data.json");
        write_json(&path, &vec![1, 2]).unwrap();
        write_json(&path, &vec![3]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().trim(), "[3]");
        assert!(!dir.join("data.json.tmp").exists());
    }

    struct NotSerializable;

    impl serde::Serialize for NotSerializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("a value that cannot be saved"))
        }
    }

    #[test]
    fn a_failed_json_write_keeps_the_old_file_and_leaves_no_tmp() {
        let dir = scratch!("json-failed");
        let path = dir.join("data.json");
        write_json(&path, &vec![1]).unwrap();

        assert!(write_json(&path, &NotSerializable).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap().trim(), "[1]");
        assert_eq!(crate::test_support::file_names(&dir), ["data.json"]);
    }

    #[test]
    fn a_missing_file_is_none_and_a_damaged_one_is_a_parse_error() {
        let dir = scratch!("read-json-result");
        let path = dir.join("data.json");
        assert!(matches!(read_json::<Sample>(&path), Ok(None)));
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            read_json::<Sample>(&path),
            Err(ReadError::Parse(_))
        ));
        write_json(&path, &Sample { value: 3 }).unwrap();
        assert_eq!(
            read_json::<Sample>(&path).unwrap(),
            Some(Sample { value: 3 })
        );
    }

    #[test]
    fn saves_of_one_file_from_threads_all_succeed() {
        let dir = scratch!("threads");
        let path = dir.join("value.json");
        std::thread::scope(|scope| {
            for number in 0..8_u32 {
                let path = &path;
                scope.spawn(move || {
                    for step in 0..25_u32 {
                        write_json(path, &[number, step]).unwrap();
                    }
                });
            }
        });
        let text = fs::read_to_string(&path).unwrap();
        assert!(serde_json::from_str::<Vec<u32>>(&text).is_ok(), "{text}");
    }
}
