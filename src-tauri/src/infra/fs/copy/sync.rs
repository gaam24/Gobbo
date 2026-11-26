//! The one-pass directory copy and the test-injected write failure it shares with the merge.

use std::fs;
use std::io;
use std::path::Path;

use crate::infra::fs::remove::remove_path;
use crate::infra::fs::walk::{WalkErrors, walk};

use super::model::CopyMode;
use super::tree::copy_tree;

/// Whether a copy in `mode` writes `to`: a replace always does, any other mode only fills a missing entry.
fn writes_entry(mode: CopyMode, to: &Path) -> bool {
    mode == CopyMode::Replace || !to.exists()
}

pub(super) fn copy_dir(source: &Path, target: &Path, mode: CopyMode) -> io::Result<()> {
    fs::create_dir_all(target)?;
    walk(source, WalkErrors::Fail, &mut |entry, rel, is_dir| {
        let to = target.join(rel);

        if is_dir {
            if writes_entry(mode, &to) {
                fs::create_dir_all(&to)?;
            }
        } else if writes_entry(mode, &to) {
            #[cfg(test)]
            if injected_failure() {
                return Err(io::Error::other("injected copy failure"));
            }

            fs::copy(entry.path(), &to)?;
        }

        Ok(())
    })
}

#[cfg(test)]
thread_local! {
    /// A test-only countdown: the next N file copies succeed, then the next one fails. Tests use it to prove that a
    /// failed replace leaves the old target and its previous generation intact.
    static FAIL_AFTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Makes the next `files` copies inside a copied folder fail after that many successes. Test-only.
#[cfg(test)]
pub fn fail_after(files: usize) {
    FAIL_AFTER.with(|cell| cell.set(files));
}

#[cfg(test)]
pub(super) fn injected_failure() -> bool {
    FAIL_AFTER.with(|cell| {
        let left = cell.get();

        if left == 0 {
            return false;
        }

        cell.set(left - 1);
        left == 1
    })
}

/// Copies `source` over `target`, or removes `target` when the source is gone and `remove_missing` is set.
pub fn sync_copy(source: &Path, target: &Path, remove_missing: bool) -> io::Result<()> {
    if source.exists() {
        copy_tree(source, target, CopyMode::Replace).map_err(io::Error::from)
    } else if remove_missing {
        remove_path(target)
    } else {
        Ok(())
    }
}
