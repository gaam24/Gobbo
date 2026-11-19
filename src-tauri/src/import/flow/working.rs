//! The working folders of imports: the folder that holds an archive while it is read, the claim that removes the
//! reserved name when an import fails, and the cleanup of folders an earlier run left.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use crate::infra::store;

/// The working folders of imports are named with this prefix. A name with a dot is never a mod folder.
pub(super) const WORKING_PREFIX: &str = ".importing-";

/// The working folder of an import. It is removed when it goes out of scope, also when the import fails before it
/// claims an archive name.
pub(super) struct Working {
    pub(super) path: PathBuf,
}

impl Working {
    pub(super) fn create(path: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for Working {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// What an import holds on disk until it commits: the archive name it reserved and its working folder. When the import
/// ends without committing, both are removed.
pub(super) struct Claim {
    pub(super) archive: PathBuf,
    pub(super) working: Arc<Working>,
    pub(super) committed: bool,
}

impl Drop for Claim {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.archive);
        }
        store::release_archive(&self.archive);
    }
}

/// The name of the archive copy in the working folder. It keeps the source extension, because the archive reader picks
/// the format from it.
pub(super) fn working_copy_name(source: &std::path::Path) -> String {
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("zip");
    format!("archive.{ext}")
}

/// Removes the working folders that an earlier run left in the library. The folders of this run stay, and so does a
/// folder that another process created less than an hour ago: it may still be hashing its archive.
pub(super) fn remove_stale_working(library: &std::path::Path) {
    remove_working_older_than(library, std::time::Duration::from_hours(1));
}

/// The age rule of `remove_stale_working`, so a test does not have to wait an hour or change an old folder's time.
pub(super) fn remove_working_older_than(library: &std::path::Path, stale: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(library) else {
        return;
    };
    let own = format!("{WORKING_PREFIX}{}-", std::process::id());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(WORKING_PREFIX) || name.starts_with(&own) {
            continue;
        }
        if store::is_older_than(&entry, stale) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn an_earlier_runs_working_folder_is_removed_and_this_runs_is_kept() {
        let library = scratch!("stale-working");
        let stale = library.join(format!("{WORKING_PREFIX}1-abandoned"));
        let own = library.join(format!("{WORKING_PREFIX}{}-running", std::process::id()));
        let mod_folder = library.join("Mod");
        for dir in [&stale, &own, &mod_folder] {
            std::fs::create_dir_all(dir).unwrap();
        }

        remove_working_older_than(&library, std::time::Duration::ZERO);

        assert!(!stale.exists());
        assert!(own.is_dir());
        assert!(mod_folder.is_dir());

        // A folder another process could still be filling is left for the next run.
        let fresh = library.join(format!("{WORKING_PREFIX}2-running"));
        std::fs::create_dir_all(&fresh).unwrap();
        remove_stale_working(&library);
        assert!(fresh.is_dir());
    }
}
