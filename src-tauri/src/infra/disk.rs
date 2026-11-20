//! The space on the drive that holds a path, for the preflight report. One place asks sysinfo, so a platform never
//! needs its own disk code.

use std::path::{Path, PathBuf};

use sysinfo::Disks;

/// Free and total space in kilobytes. `u64` stays inside the backend; the IPC type uses `u32` like the other sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub free_kb: u64,
    pub total_kb: u64,
}

/// The space on the longest mount point that contains `path`. A path that does not exist yet uses its closest existing
/// parent, so a mods folder created by the first apply still gets a real answer.
pub fn space_for(path: &Path) -> Option<Space> {
    let existing = existing_ancestor(path)?;
    let target = existing.canonicalize().unwrap_or_else(|_| existing.clone());
    let disks = Disks::new_with_refreshed_list();
    let disk = disks
        .list()
        .iter()
        .filter(|disk| {
            let mount = disk.mount_point();
            target.starts_with(mount)
                || mount
                    .canonicalize()
                    .is_ok_and(|mount| target.starts_with(mount))
        })
        .max_by_key(|disk| disk.mount_point().as_os_str().len())?;
    Some(Space {
        free_kb: disk.available_space() / 1024,
        total_kb: disk.total_space() / 1024,
    })
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path;

    loop {
        if current.exists() {
            return Some(current.to_path_buf());
        }

        current = current.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_folder_has_a_disk_with_some_total_space() {
        let base = crate::test_support::scratch!("disk-space");
        let space = space_for(&base).expect("a disk for the scratch folder");
        assert!(space.total_kb > 0);
        assert!(space.free_kb <= space.total_kb);
    }

    #[test]
    fn a_missing_path_uses_its_existing_parent() {
        let base = crate::test_support::scratch!("disk-parent");
        let missing = base.join("not-created").join("deeper");
        assert!(space_for(&missing).is_some());
    }
}
