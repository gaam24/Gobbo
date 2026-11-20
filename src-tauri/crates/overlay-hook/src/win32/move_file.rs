//! `MoveFileExW` and `MoveFileWithProgressW`, which rename and move files. `MoveFileW` and `std::fs::rename` end in one of
//! them. A file of a lower layer is copied up and the copy is moved, and the lower entry it leaves is hidden. A folder of
//! a lower layer is not moved: the call fails, because its lower part would stay where it was.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER,
    ERROR_NOT_SUPPORTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_DELAY_UNTIL_REBOOT, MOVEFILE_REPLACE_EXISTING,
};

use super::source::{full_name, source_path};
use super::{PreserveLastError, fail};
use crate::guard::enter_session;
use crate::paths::{in_lower, make_write_target, name_taken, place, shown, unsafe_name};
use crate::text::{display_name, wide};
use crate::trace::trace_call;
use crate::whiteouts::hide;

/// The shared body of `MoveFileExW` and `MoveFileWithProgressW`: the in-game move, the pass-through log, and the
/// original call.
///
/// # Safety
/// `from` and `to` must be NUL-terminated UTF-16 strings, or null.
unsafe fn move_file_common(
    operation: &'static str,
    from: *const u16,
    to: *const u16,
    flags: u32,
    mover: impl FnOnce(*const u16, *const u16) -> i32,
    original: impl FnOnce() -> i32,
) -> i32 {
    unsafe { trace_call(operation, from) };

    if let Some(_guard) = enter_session()
        && let Some(result) = unsafe { move_in_game(from, to, flags, mover) }
    {
        return result;
    }

    if crate::diag::settings().pass_writes {
        let from = unsafe { display_name(from) };
        let to = unsafe { display_name(to) };
        crate::trace::log(&format!(
            "pass-through {operation} {from} -> {to} flags=0x{flags:x}"
        ));
    }

    let moved = original();
    let _last_error = PreserveLastError::capture();
    moved
}

/// `MoveFileExW`, which renames and moves files and is what `MoveFileW` and `std::fs::rename` end in. A file of a lower
/// layer is copied up and the copy is moved, and the lower entry it leaves is hidden. A folder of a lower layer is not
/// moved: the call fails, because its lower part would stay where it was.
///
/// # Safety
/// Called by the program with the arguments of `MoveFileExW`.
pub(crate) unsafe extern "system" fn move_file_ex(
    from: *const u16,
    to: *const u16,
    flags: u32,
) -> i32 {
    unsafe {
        move_file_common(
            "MoveFileExW",
            from,
            to,
            flags,
            |source, dest| {
                original!(move_file_ex, {
                    fail(ERROR_INVALID_FUNCTION);
                    0
                })
                .call(source, dest, flags)
            },
            || {
                let original = original!(move_file_ex, {
                    fail(ERROR_INVALID_FUNCTION);
                    0
                });
                original.call(from, to, flags)
            },
        )
    }
}

/// `MoveFileWithProgressW`: `MoveFileW` ends here, and it does not go through `MoveFileExW`. A rename inside the game
/// folder goes through the layers as `MoveFileExW` does. The progress callback goes on to the system with the move, so a
/// move to another volume reports its progress as it does without the hook.
///
/// # Safety
/// Called by the program with the arguments of `MoveFileWithProgressW`.
pub(crate) unsafe extern "system" fn move_file_with_progress(
    from: *const u16,
    to: *const u16,
    progress: *const c_void,
    data: *const c_void,
    flags: u32,
) -> i32 {
    unsafe {
        move_file_common(
            "MoveFileWithProgressW",
            from,
            to,
            flags,
            |source, dest| {
                original!(move_file_with_progress, {
                    fail(ERROR_INVALID_FUNCTION);
                    0
                })
                .call(source, dest, progress, data, flags)
            },
            || {
                let original = original!(move_file_with_progress, {
                    fail(ERROR_INVALID_FUNCTION);
                    0
                });
                original.call(from, to, progress, data, flags)
            },
        )
    }
}

/// The error a move inside the game folder is refused with, before anything is copied or hidden.
fn move_refusal(flags: u32) -> Option<u32> {
    (flags & MOVEFILE_DELAY_UNTIL_REBOOT != 0).then_some(ERROR_NOT_SUPPORTED)
}

/// The move, when one of the two paths is inside the game folder. `None` means neither is, and the call goes ahead.
///
/// # Safety
/// `from` and `to` must be NUL-terminated UTF-16 strings, or null.
unsafe fn move_in_game(
    from: *const u16,
    to: *const u16,
    flags: u32,
    mover: impl FnOnce(*const u16, *const u16) -> i32,
) -> Option<i32> {
    if unsafe { unsafe_name(from) } || unsafe { unsafe_name(to) } {
        return Some(fail(ERROR_ACCESS_DENIED));
    }

    let from_place = unsafe { place(from) };
    let to_place = unsafe { place(to) };

    if from_place.is_none() && to_place.is_none() {
        return None;
    }

    if let (Some(source), Some(target)) = (&from_place, &to_place)
        && !std::ptr::eq(source.mount, target.mount)
    {
        // A move between two mounts would copy between two profile layers. Refuse rather than guess.
        return Some(fail(ERROR_ACCESS_DENIED));
    }

    if let Some(code) = move_refusal(flags) {
        // A move at reboot is stored in the registry for a real path; a layer path would be gone by then.
        return Some(fail(code));
    }

    let (source, below) = match unsafe { source_path(from, from_place.as_ref()) } {
        Ok(found) => found,
        Err(code) => return Some(fail(code)),
    };

    let dest = match &to_place {
        Some(place) => {
            // The same entry under another spelling (a case change) is not an existing target.
            let same_entry = from_place
                .as_ref()
                .is_some_and(|source| gobbo_overlay_core::same_folded(&source.rel, &place.rel));
            let replace = flags & MOVEFILE_REPLACE_EXISTING != 0;
            let target_shown = match shown(place.mount, &place.rel) {
                Ok(shown) => shown.is_some(),
                Err(failure) => return Some(fail(failure.win32())),
            };

            if name_taken(same_entry, replace, || target_shown) {
                return Some(fail(ERROR_ALREADY_EXISTS));
            }

            match make_write_target(place.mount, &place.rel) {
                Ok(path) => path,
                Err(failure) => return Some(fail(failure.win32())),
            }
        }
        None => match unsafe { full_name(to) } {
            Some(path) => path,
            None => return Some(fail(ERROR_INVALID_PARAMETER)),
        },
    };

    // A move that replaces a file of a lower layer replaces the file for the program: the lower file is gone, so it is
    // hidden once the move succeeds. Without the whiteout it would stay in the listing under the replacement's name, and
    // come back when the replacement is deleted.
    let replaces_below = match &to_place {
        Some(place) if flags & MOVEFILE_REPLACE_EXISTING != 0 => {
            in_lower(place.mount, &place.rel).unwrap_or(false)
        }
        _ => false,
    };
    let source_wide = wide(source.as_os_str());
    let dest_wide = wide(dest.as_os_str());
    let moved = mover(source_wide.as_ptr(), dest_wide.as_ptr());
    let _last_error = PreserveLastError::capture();

    if moved != 0
        && below
        && let Some(place) = &from_place
    {
        hide(place.mount, &place.rel);
    }

    if moved != 0
        && replaces_below
        && let Some(place) = &to_place
    {
        hide(place.mount, &place.rel);
    }

    Some(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_at_reboot_inside_the_game_is_refused() {
        assert_eq!(
            move_refusal(MOVEFILE_DELAY_UNTIL_REBOOT),
            Some(ERROR_NOT_SUPPORTED)
        );
        assert_eq!(move_refusal(MOVEFILE_REPLACE_EXISTING), None);
        assert_eq!(move_refusal(0), None);
    }
}
