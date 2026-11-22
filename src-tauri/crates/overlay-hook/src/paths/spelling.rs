//! The exact-name lookups of the layers: the entry a name has in one folder, as the disk spells it, and the search pattern
//! they use. A listing takes the spelling of an entry that a lower layer also has from here, so a program sees the name
//! Windows keeps. The lookups are system calls: the caller is inside the hook, with its guard held (see `Guard`).

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null;

use gobbo_overlay_core::{join, wide};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    FindClose, FindExInfoStandard, FindExSearchNameMatch, FindFirstFileExW, WIN32_FIND_DATAW,
};

use crate::text::same_folded_wide;
use crate::win32::find_data_name;

/// The entry `name` of the folder `rel` under `base`, as the disk spells it. `None` when the folder has no such entry, or
/// the name is not the one found (the search matches the name under the folding rule, so this only guards the match).
pub(crate) fn find_exact(base: &Path, rel: &str, name: &[u16]) -> Option<WIN32_FIND_DATAW> {
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    crate::stat_spelling();
    crate::stat_io();
    let handle = with_pattern_units(base, rel, name, |pattern| {
        // SAFETY: the pattern is NUL-terminated, and `data` is writable for the standard information level.
        unsafe {
            FindFirstFileExW(
                pattern,
                FindExInfoStandard,
                (&raw mut data).cast::<c_void>(),
                FindExSearchNameMatch,
                null(),
                0,
            )
        }
    });

    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: the handle came from the search above and is closed once.
    unsafe { FindClose(handle) };
    same_folded_wide(find_data_name(&data), name).then_some(data)
}

/// Calls `call` with the search pattern `base\rel\mask`. The common pattern is assembled on the stack; a pattern too long
/// for the stack falls back to the allocating path form. The closure keeps the fallback buffer alive until the system
/// call returns.
pub(crate) fn with_pattern<R>(
    base: &Path,
    rel: &str,
    mask: &str,
    call: impl FnOnce(*const u16) -> R,
) -> R {
    with_pattern_impl(base, rel, mask.encode_utf16(), || mask.to_string(), call)
}

/// `with_pattern` for a mask that is already UTF-16, as a directory name is. No `String` is built for the common path.
fn with_pattern_units<R>(
    base: &Path,
    rel: &str,
    mask: &[u16],
    call: impl FnOnce(*const u16) -> R,
) -> R {
    with_pattern_impl(
        base,
        rel,
        mask.iter().copied(),
        || String::from_utf16_lossy(mask),
        call,
    )
}

/// The shared body of `with_pattern` and `with_pattern_units`: `mask` is the pattern as units, and `overflow` builds
/// the allocating form of the mask when the stack buffer is too small.
fn with_pattern_impl<R>(
    base: &Path,
    rel: &str,
    mask: impl IntoIterator<Item = u16>,
    overflow: impl FnOnce() -> String,
    call: impl FnOnce(*const u16) -> R,
) -> R {
    let mut units = [0u16; 520];
    let mut length = 0;
    let mut overflowed = false;
    'build: {
        for unit in base.as_os_str().encode_wide() {
            if length >= units.len() - 4 {
                overflowed = true;
                break 'build;
            }

            units[length] = unit;
            length += 1;
        }

        if !rel.is_empty() {
            if length > 0 && units[length - 1] != u16::from(b'\\') {
                if length >= units.len() - 3 {
                    overflowed = true;
                    break 'build;
                }

                units[length] = u16::from(b'\\');
                length += 1;
            }

            for unit in rel.encode_utf16() {
                if length >= units.len() - 3 {
                    overflowed = true;
                    break 'build;
                }

                units[length] = unit;
                length += 1;
            }
        }

        if length > 0 && units[length - 1] != u16::from(b'\\') {
            if length >= units.len() - 2 {
                overflowed = true;
                break 'build;
            }

            units[length] = u16::from(b'\\');
            length += 1;
        }

        for unit in mask {
            if length >= units.len() - 1 {
                overflowed = true;
                break 'build;
            }

            units[length] = unit;
            length += 1;
        }
    }

    if overflowed {
        let pattern = wide(join(base, rel).join(overflow()).as_os_str());
        return call(pattern.as_ptr());
    }

    units[length] = 0;
    call(units.as_ptr())
}
