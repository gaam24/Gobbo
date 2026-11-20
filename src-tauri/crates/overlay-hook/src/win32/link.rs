//! `CreateHardLinkW` and `CreateSymbolicLinkW`: a link into the game folder, or to a file of it, would make the game folder
//! change through a name the layers do not see. Such a link is refused.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION};

use super::{PreserveLastError, fail};
use crate::guard::enter_session;
use crate::paths::{place, unsafe_name};

/// `CreateHardLinkW`: a hard link into the game folder, or to a file of it, would make the game folder change through
/// a name the layers do not see. Such a link is refused.
///
/// # Safety
/// Called by the program with the arguments of `CreateHardLinkW`.
pub(crate) unsafe extern "system" fn create_hard_link(
    link: *const u16,
    existing: *const u16,
    security: *const c_void,
) -> i32 {
    if let Some(_guard) = enter_session()
        && unsafe { touches_game(link, existing) }
    {
        return fail(ERROR_ACCESS_DENIED);
    }

    let original = original!(create_hard_link, {
        fail(ERROR_INVALID_FUNCTION);
        0
    });
    let created = unsafe { original.call(link, existing, security) };
    let _last_error = PreserveLastError::capture();
    created
}

/// `CreateSymbolicLinkW`: as `CreateHardLinkW`. A symbolic link that leads into the game folder, or sits in it, is refused.
///
/// # Safety
/// Called by the program with the arguments of `CreateSymbolicLinkW`.
pub(crate) unsafe extern "system" fn create_symbolic_link(
    link: *const u16,
    target: *const u16,
    flags: u32,
) -> u8 {
    if let Some(_guard) = enter_session()
        && unsafe { touches_game(link, target) }
    {
        let _ = fail(ERROR_ACCESS_DENIED);
        return 0;
    }

    let original = original!(create_symbolic_link, {
        let _ = fail(ERROR_INVALID_FUNCTION);
        0
    });
    let created = unsafe { original.call(link, target, flags) };
    let _last_error = PreserveLastError::capture();
    created
}

/// Whether either of two names is inside the game folder.
///
/// # Safety
/// Both pointers must be NUL-terminated UTF-16 strings, or null.
unsafe fn touches_game(first: *const u16, second: *const u16) -> bool {
    (unsafe { unsafe_name(first) })
        || (unsafe { unsafe_name(second) })
        || unsafe { place(first) }.is_some()
        || unsafe { place(second) }.is_some()
}
