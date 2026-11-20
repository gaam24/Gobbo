//! `ReplaceFileW`: the replaced file and the backup are the upper copies, and the replacement is taken from the upper layer
//! (or copied up from a lower one). The game folder is not written to.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER,
};

use super::source::{full_name, source_path};
use super::{PreserveLastError, fail};
use crate::guard::enter_session;
use crate::paths::{make_write_target, place, unsafe_name};
use crate::text::{display_name, wide};
use crate::whiteouts::hide;

/// `ReplaceFileW`: the replaced file and the backup are the upper copies, and the replacement is taken from the upper
/// layer (or copied up from a lower one). The game folder is not written to.
///
/// # Safety
/// Called by the program with the arguments of `ReplaceFileW`.
pub(crate) unsafe extern "system" fn replace_file(
    replaced: *const u16,
    replacement: *const u16,
    backup: *const u16,
    flags: u32,
    exclude: *mut c_void,
    reserved: *mut c_void,
) -> i32 {
    if let Some(_guard) = enter_session()
        && let Some(result) =
            unsafe { replace_in_game(replaced, replacement, backup, flags, exclude, reserved) }
    {
        return result;
    }

    if crate::diag::settings().pass_writes {
        let replaced_name = unsafe { display_name(replaced) };
        let replacement_name = unsafe { display_name(replacement) };
        let backup_name = if backup.is_null() {
            "<none>".to_string()
        } else {
            unsafe { display_name(backup) }
        };
        crate::trace::log(&format!(
            "pass-through ReplaceFileW {replaced_name} <- {replacement_name} backup={backup_name} flags=0x{flags:x}"
        ));
    }

    let original = original!(replace_file, {
        fail(ERROR_INVALID_FUNCTION);
        0
    });
    let replaced_ok =
        unsafe { original.call(replaced, replacement, backup, flags, exclude, reserved) };
    let _last_error = PreserveLastError::capture();
    replaced_ok
}

/// The replacement, when one of the paths is inside the game folder. `None` means none is, and the call goes ahead.
///
/// # Safety
/// The pointers must be NUL-terminated UTF-16 strings, or null.
unsafe fn replace_in_game(
    replaced: *const u16,
    replacement: *const u16,
    backup: *const u16,
    flags: u32,
    exclude: *mut c_void,
    reserved: *mut c_void,
) -> Option<i32> {
    if unsafe { unsafe_name(replaced) }
        || unsafe { unsafe_name(replacement) }
        || unsafe { unsafe_name(backup) }
    {
        return Some(fail(ERROR_ACCESS_DENIED));
    }

    let replaced_place = unsafe { place(replaced) };
    let replacement_place = unsafe { place(replacement) };
    let backup_place = unsafe { place(backup) };

    if replaced_place.is_none() && replacement_place.is_none() && backup_place.is_none() {
        return None;
    }
    // A replacement may not cross two mounts: the copy-up and the whiteout would belong to different profile layers.
    let mut mount = None;

    for place in [&replaced_place, &replacement_place, &backup_place]
        .into_iter()
        .flatten()
    {
        if let Some(first) = mount {
            if !std::ptr::eq(first, place.mount) {
                return Some(fail(ERROR_ACCESS_DENIED));
            }
        } else {
            mount = Some(place.mount);
        }
    }

    let _mount = mount?;

    let (replaced_path, _) = match unsafe { source_path(replaced, replaced_place.as_ref()) } {
        Ok(found) => found,
        Err(code) => return Some(fail(code)),
    };
    let (replacement_path, replacement_below) =
        match unsafe { source_path(replacement, replacement_place.as_ref()) } {
            Ok(found) => found,
            Err(code) => return Some(fail(code)),
        };
    let backup_path = match &backup_place {
        Some(place) => match make_write_target(place.mount, &place.rel) {
            Ok(path) => Some(path),
            Err(failure) => return Some(fail(failure.win32())),
        },
        None if backup.is_null() => None,
        None => match unsafe { full_name(backup) } {
            Some(path) => Some(path),
            None => return Some(fail(ERROR_INVALID_PARAMETER)),
        },
    };

    let replaced_wide = wide(replaced_path.as_os_str());
    let replacement_wide = wide(replacement_path.as_os_str());
    let backup_wide = backup_path.as_ref().map(|path| wide(path.as_os_str()));
    let backup_ptr = backup_wide
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr());
    let original = original!(replace_file, Some(fail(ERROR_INVALID_FUNCTION)));
    let replaced_ok = unsafe {
        original.call(
            replaced_wide.as_ptr(),
            replacement_wide.as_ptr(),
            backup_ptr,
            flags,
            exclude,
            reserved,
        )
    };
    let _last_error = PreserveLastError::capture();

    if replaced_ok != 0
        && replacement_below
        && let Some(place) = &replacement_place
    {
        hide(place.mount, &place.rel);
    }

    Some(replaced_ok)
}
