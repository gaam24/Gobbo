//! Placing one file in the overlay layer: a hardlink to the library, or a copy when the two places cannot share a file.
//! The game folder is never a destination; the layer is Gobbo's own folder in the profile state.

use std::fs;
use std::io;
use std::path::Path;

use crate::deploy::error::DeployError;
use crate::error::FileError;

/// Puts the file in `dst` as a hardlink to `src`. A hardlink that fails because the two places cannot share a file
/// (another volume, a file system without hard links) falls back to a copy. Every other failure is an error: a full
/// disk, a permission problem or an existing target must not be hidden by a copy.
pub(super) fn place(src: &Path, dst: &Path) -> Result<(), DeployError> {
    match fs::hard_link(src, dst) {
        Ok(()) => Ok(()),
        Err(error) if hardlink_unsupported(&error) => fs::copy(src, dst)
            .map(|_| ())
            .map_err(|error| FileError::new(dst, error).into()),
        Err(error) => Err(FileError::new(dst, error).into()),
    }
}

/// Whether a failed hard link means the two places cannot share a file: another volume, or a file system without hard
/// links. Only then a copy is the right result.
fn hardlink_unsupported(error: &io::Error) -> bool {
    #[cfg(windows)]
    const CODES: [i32; 2] = [
        17, // ERROR_NOT_SAME_DEVICE
        1,  // ERROR_INVALID_FUNCTION: the file system has no hard links
    ];
    #[cfg(unix)]
    const CODES: [i32; 2] = [
        18, // EXDEV: another file system
        95, // EOPNOTSUPP: the file system has no hard links
    ];
    #[cfg(not(any(windows, unix)))]
    const CODES: [i32; 0] = [];
    error
        .raw_os_error()
        .is_some_and(|code| CODES.contains(&code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, write};

    #[test]
    fn a_failed_hard_link_for_another_reason_is_an_error_not_a_copy() {
        // The target exists, so the link fails for a reason a copy would hide: the target stays as it was.
        let base = scratch!("place-error");
        let src = base.join("src.txt");
        let dst = base.join("dst.txt");
        write(&src, "from the library");
        write(&dst, "already there");

        let error = place(&src, &dst).unwrap_err();
        assert!(matches!(error, DeployError::Io { .. }), "{error:?}");
        assert_eq!(fs::read_to_string(&dst).unwrap(), "already there");
    }

    #[test]
    fn only_a_missing_shared_volume_or_hard_links_fall_back_to_a_copy() {
        #[cfg(windows)]
        assert!(hardlink_unsupported(&io::Error::from_raw_os_error(17)));
        #[cfg(unix)]
        assert!(hardlink_unsupported(&io::Error::from_raw_os_error(18)));
        assert!(!hardlink_unsupported(&io::Error::other("no code")));
    }
}
