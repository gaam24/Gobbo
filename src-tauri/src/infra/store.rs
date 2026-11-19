//! Archive copies and the state files around them: free names, reservations, atomic writes and stale temp cleanup.
//! JSON reads and writes live in `infra::json`, file copies and hashes in `infra::io`, and the tail reader in
//! `infra::text`.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::{AppError, FileError, io_at};

/// Name of the folder in the game folder where archive copies live. A copy has the name of the original file.
pub const ARCHIVES_DIR: &str = "archives";

/// A suffix that no other call in this process returns: the process id and a counter, for example `4312-7`.
pub fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// A temporary file next to `path`. Its name is unique, so two writes of one file never share it.
fn temp_path(path: &Path) -> PathBuf {
    sibling_named(path, |name| format!("{name}.{}.tmp", unique_suffix()))
}

/// A path next to `path` whose file name `build` makes from the original file name, for example `<name>.old`. A path
/// without a file name gets an empty one.
pub fn sibling_named(path: &Path, build: impl FnOnce(&str) -> String) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(build(&name))
}

/// Removes the `*.tmp` files that an interrupted `write_atomic` left in `dir`. Only files old enough that no running
/// write can still own them are removed. Other files, and subfolders, are left alone. Errors are ignored: a file that
/// another process still holds is not a reason to fail the start.
pub fn remove_temp_files(dir: &Path) {
    let stale = std::time::Duration::from_hours(1);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "tmp") || !path.is_file() {
            continue;
        }
        if is_older_than(&entry, stale) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Whether a directory entry is older than `stale`.
pub fn is_older_than(entry: &fs::DirEntry, stale: std::time::Duration) -> bool {
    entry
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| std::time::SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= stale)
}

/// Writes a file through a temporary file and a rename, so an interrupted write does not spoil the old file.
pub fn write_atomic(path: &Path, content: &[u8]) -> Result<(), FileError> {
    write_temp_atomic(
        path,
        |file, tmp| file.write_all(content).map_err(|e| FileError::new(tmp, e)),
        FileError::new,
    )
}

/// Writes a file through a unique temporary file next to `path`, syncs it, renames it over `path` and syncs the parent
/// folder. The data reaches the device before the rename and the folder entry after it, so a power loss cannot leave a
/// renamed but empty file. A failed write removes the temporary file and leaves the old file in place. `write` fills the
/// file, and `error` builds the caller's error type for a failed step, named by the path that step used.
pub(super) fn write_temp_atomic<E>(
    path: &Path,
    write: impl FnOnce(&mut fs::File, &Path) -> Result<(), E>,
    error: impl Fn(&Path, io::Error) -> E,
) -> Result<(), E> {
    let tmp = temp_path(path);
    let result = (|| -> Result<(), E> {
        let mut file = fs::File::create(&tmp).map_err(|e| error(&tmp, e))?;
        write(&mut file, &tmp)?;
        file.sync_all().map_err(|e| error(&tmp, e))?;
        drop(file);
        rename_replacing(&tmp, path).map_err(|e| error(path, e))?;
        sync_parent(path).map_err(|e| error(path, e))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Flushes the folder entry after a rename, so the new name survives a power loss. Windows has no portable directory
/// fsync; the file itself was synced before the rename and NTFS journals the rename.
#[cfg_attr(
    not(unix),
    allow(
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "Windows keeps the Result signature the Unix path needs, so callers stay the same on both systems"
    )
)]
fn sync_parent(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Atomic replaces are serialized. Windows can refuse two `MoveFileEx` calls that replace the same file at once, and
/// a retry loop without a lock lets the writers collide again in lockstep. The lock covers only the rename.
#[cfg(windows)]
static RENAME_LOCK: Mutex<()> = Mutex::new(());

/// Replaces `to` with `from`, removing `from` when the replace fails.
pub fn replace_file_clean(from: &Path, to: &Path) -> Result<(), AppError> {
    if let Err(error) = rename_replacing(from, to) {
        let _ = fs::remove_file(from);
        return Err(io_at(to)(error));
    }
    Ok(())
}

/// Replaces `to` with `from` atomically, retrying a rename that Windows refused because another process briefly held
/// the destination (a virus scanner or a test reader). The temporary file has a unique name, so a retry cannot mix
/// two writers.
fn rename_replacing(from: &Path, to: &Path) -> io::Result<()> {
    // Unix replaces atomically; the retry exists for Windows, where a virus scanner can briefly hold the destination.
    #[cfg(not(windows))]
    return fs::rename(from, to);

    #[cfg(windows)]
    {
        let mut last = None;
        // The backoff is exponential: a scanner may hold the destination for a few milliseconds. A genuine permission
        // error only pays this delay once. The lock is taken only around the rename, never across the sleep.
        for attempt in 0..8 {
            let result = {
                let _guard = RENAME_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
                fs::rename(from, to)
            };
            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last = Some(error);
                    if attempt < 7 {
                        std::thread::sleep(std::time::Duration::from_millis(1 << attempt));
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| io::Error::other("could not replace the file")))
    }
}

/// Name of an archive copy in `archives/`: the original file name. When another archive already has it,
/// we add the start of the SHA-256 sum, for example `Mod 1.2.0 (3f9a1c2b).zip`.
pub fn free_archive_name(
    archives: &Path,
    original: &str,
    sha256: &str,
    taken: &BTreeSet<String>,
) -> String {
    let safe = crate::domain::naming::safe_limited_file_name(original);
    let (stem, ext) = crate::domain::archive::split_name(&safe);
    let short = sha256.get(..8).unwrap_or(sha256);
    // The suffixed name is checked too: a second collision would otherwise return the same name again, and the
    // `create_new` after it would fail. The counter is deterministic, so two runs agree on the name.
    let name = crate::domain::naming::first_free(
        |number| match number {
            1 => safe.clone(),
            2 => format!("{stem} ({short}){ext}"),
            _ => format!("{stem} ({short}-{}){ext}", number - 1),
        },
        |name| !taken.contains(name) && !archives.join(name).exists(),
    );
    crate::domain::naming::safe_limited_file_name(&name)
}

/// Archive copies that an import has named but not yet moved into `archives/`, by path. A cleanup keeps them: their
/// names are taken, and the copies are about to arrive.
static RESERVED: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

fn reservations() -> MutexGuard<'static, BTreeSet<PathBuf>> {
    RESERVED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holds the name of an archive copy for an import, until `release_archive`.
pub fn reserve_archive(path: &Path) {
    reservations().insert(path.to_path_buf());
}

/// Gives back a name that `reserve_archive` held.
pub fn release_archive(path: &Path) {
    reservations().remove(path);
}

/// Removes from `archives/` everything except the copies in `keep` (file names) and the names that imports hold:
/// unused archives, orphaned files and interrupted `.tmp` files.
pub fn clean_archives(archives: &Path, keep: &BTreeSet<String>) -> Result<(), AppError> {
    let entries = match fs::read_dir(archives) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_at(archives)(e)),
    };
    let held: BTreeSet<String> = reservations()
        .iter()
        .filter(|path| path.parent() == Some(archives))
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    for entry in entries {
        let entry = entry.map_err(io_at(archives))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep.contains(&name) && !held.contains(&name) && path.is_file() {
            fs::remove_file(&path).map_err(io_at(&path))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_failed_write_keeps_the_old_file_and_leaves_no_tmp() {
        let dir = scratch!("failed-write");
        let path = dir.join("value.bin");
        fs::write(&path, "old").unwrap();

        let result = write_temp_atomic(
            &path,
            |file, _| {
                file.write_all(b"new")?;
                Err(io::Error::other("the disk is full"))
            },
            |_, error| error,
        );
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(crate::test_support::file_names(&dir), ["value.bin"]);
    }

    #[test]
    fn only_old_temp_files_are_removed() {
        use std::time::{Duration, SystemTime};

        let dir = scratch!("stale-temps");
        let old = dir.join("library.json.1-0.tmp");
        let fresh = dir.join("library.json.2-0.tmp");
        let other = dir.join("library.json");
        fs::write(&old, "old").unwrap();
        fs::write(&fresh, "fresh").unwrap();
        fs::write(&other, "{}").unwrap();
        let two_hours = SystemTime::now() - Duration::from_hours(2);
        fs::OpenOptions::new()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(two_hours)
            .unwrap();

        remove_temp_files(&dir);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(other.exists());
    }

    #[test]
    fn a_name_already_taken_gets_the_start_of_the_digest() {
        let dir = scratch!("names");
        let archives = dir.join(ARCHIVES_DIR);
        fs::create_dir_all(&archives).unwrap();
        fs::write(archives.join("Mod.zip"), "other").unwrap();
        let digest = "3f9a1c2b".repeat(8);

        assert_eq!(
            free_archive_name(&archives, "Mod.zip", &digest, &BTreeSet::new()),
            "Mod (3f9a1c2b).zip"
        );
        assert_eq!(
            free_archive_name(
                &archives,
                "New.zip",
                &digest,
                &BTreeSet::from(["New.zip".to_string()])
            ),
            "New (3f9a1c2b).zip"
        );
        assert_eq!(
            free_archive_name(&archives, "Free.zip", &digest, &BTreeSet::new()),
            "Free.zip"
        );
    }

    #[test]
    fn clean_archives_keeps_only_the_listed_copies() {
        let dir = scratch!("clean");
        let archives = dir.join(ARCHIVES_DIR);
        fs::create_dir_all(&archives).unwrap();
        for name in ["keep.zip", "unused.zip", "orphan.zip", "half.zip.tmp"] {
            fs::write(archives.join(name), "zip bytes").unwrap();
        }

        clean_archives(&archives, &BTreeSet::from(["keep.zip".to_string()])).unwrap();
        let left: Vec<String> = fs::read_dir(&archives)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["keep.zip"]);
    }

    #[test]
    fn clean_archives_keeps_a_reserved_name_until_it_is_released() {
        let dir = scratch!("reserved");
        let archives = dir.join(ARCHIVES_DIR);
        fs::create_dir_all(&archives).unwrap();
        let held = archives.join("held.zip");
        fs::write(&held, "").unwrap();

        reserve_archive(&held);
        clean_archives(&archives, &BTreeSet::new()).unwrap();
        assert!(held.is_file());

        release_archive(&held);
        clean_archives(&archives, &BTreeSet::new()).unwrap();
        assert!(!held.exists());
    }
}
