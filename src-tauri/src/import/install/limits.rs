//! The expansion limits of one import: a mod is not a disk image, and a very large one is a mistake or a bomb.

use super::errors::InstallError;

/// An archive may not write more than this in one import. A mod is not a disk image; a larger file is a mistake or a
/// decompression bomb, and the user gets a clear error instead of a full disk.
const MAX_EXTRACT_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// The most files one import may write.
const MAX_EXTRACT_FILES: usize = 100_000;

/// The shared expansion limit: a mod is not a disk image, and a very large one is a mistake or a bomb.
pub(super) const fn check_limits(files: usize, total: u64) -> Result<(), InstallError> {
    if files > MAX_EXTRACT_FILES {
        return Err(InstallError::TooManyFiles {
            limit: MAX_EXTRACT_FILES,
        });
    }

    if total > MAX_EXTRACT_BYTES {
        return Err(InstallError::TooLarge {
            limit: MAX_EXTRACT_BYTES,
        });
    }

    Ok(())
}
