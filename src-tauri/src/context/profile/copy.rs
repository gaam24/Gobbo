//! The copy store shared by the legacy `profile_data` list and the `user_data` sets. Both keep a profile's files in
//! `state/<profile>/...` and copy them to and from the real paths around a launch.

use std::path::Path;

use crate::error::{AppError, io_at};
use crate::infra::fs::{CopyMode, copy_tree, sync_copy};

/// Copies `source` over `target`. A source that is missing leaves the stored copy alone: a folder that is temporarily
/// unavailable, or a game that deleted its saves, must not delete the profile's only copy. The store keeps one previous
/// generation, so a bad copy can be recovered.
pub(super) fn capture(source: &Path, target: &Path) -> Result<(), AppError> {
    if source.exists() {
        copy_tree(source, target, CopyMode::ReplaceKeepPrevious)
            .map_err(|error| io_at(target)(error.into()))
    } else {
        Ok(())
    }
}

/// Copies `source` back to `target`, or removes `target` when the source is gone and `remove_missing` is set.
pub(super) fn restore(source: &Path, target: &Path, remove_missing: bool) -> Result<(), AppError> {
    sync_copy(source, target, remove_missing).map_err(io_at(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn a_missing_source_keeps_the_stored_copy() {
        let dir = scratch!("profile-copy-missing");
        let store = dir.join("store");
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("save.dat"), "saved").unwrap();

        capture(&dir.join("missing"), &store).unwrap();

        assert_eq!(fs::read_to_string(store.join("save.dat")).unwrap(), "saved");
    }
}
