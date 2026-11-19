//! The source of a move or a replacement: the full path of a name outside the game folder, and the upper copy of a file that
//! a move or a replacement takes.

use std::path::PathBuf;

use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;

use crate::paths::{absolute, copy_up, in_lower, missing, shown};
use crate::state::Mount;
use crate::text::text;

/// The full path of a name the program gave, outside the game folder too. `None` for a null, empty or unresolvable
/// name; the caller must fail the call instead of guessing a path.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(super) unsafe fn full_name(name: *const u16) -> Option<PathBuf> {
    let text = unsafe { text(name) }?;
    Some(PathBuf::from(absolute(&text)?))
}

/// The file a game-relative path moves or replaces from: its upper copy, copied up first when only a lower layer has the
/// file. The flag tells whether a lower entry stays behind after the operation (it has to be hidden then).
/// The path of a move or replace source: the layer path, or the full name when it is outside every mount.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(super) unsafe fn source_path(
    name: *const u16,
    place: Option<&crate::paths::Place>,
) -> Result<(PathBuf, bool), u32> {
    match place {
        Some(place) => game_source(place.mount, &place.rel),
        None => match unsafe { full_name(name) } {
            Some(path) => Ok((path, false)),
            None => Err(ERROR_INVALID_PARAMETER),
        },
    }
}

/// The file a game-relative path moves or replaces from: its upper copy, copied up first when only a lower layer has the
/// file. The flag tells whether a lower entry stays behind after the operation (it has to be hidden then).
pub(super) fn game_source(mount: &Mount, rel: &str) -> Result<(PathBuf, bool), u32> {
    let Some((_, path, attributes)) = shown(mount, rel).map_err(|failure| failure.win32())? else {
        return Err(missing(mount, rel)
            .map_err(|failure| failure.win32())?
            .win32());
    };
    let below = in_lower(mount, rel).map_err(|failure| failure.win32())?;
    if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        // A folder of the game is not moved or replaced: its lower part would stay where it was.
        if below {
            return Err(ERROR_NOT_SUPPORTED);
        }
        return Ok((path, false));
    }
    let upper = mount.write_target(rel);
    copy_up(mount, rel, &upper, true).map_err(|failure| failure.win32())?;
    Ok((upper, below))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_or_empty_name_has_no_full_path() {
        assert_eq!(unsafe { full_name(std::ptr::null()) }, None);
        assert_eq!(unsafe { full_name([0u16].as_ptr()) }, None);
    }
}
