//! Strings between the program and Win32: the NUL-terminated UTF-16 text the calls take, and the text the trace shows.

use std::ffi::{OsStr, OsString};
use std::iter::once;
use std::os::windows::ffi::{OsStrExt, OsStringExt};

use gobbo_overlay_core::fold_unit;

/// The text of a NUL-terminated UTF-16 string.
///
/// # Safety
/// `ptr` must be null or point to a NUL-terminated UTF-16 string.
pub(crate) unsafe fn text(ptr: *const u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    let mut len = 0;

    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }

        Some(
            OsString::from_wide(std::slice::from_raw_parts(ptr, len))
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// The text of a name the program passed, or `<none>` for a null name.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn display_name(name: *const u16) -> String {
    unsafe { text(name) }.unwrap_or_else(|| "<none>".to_string())
}

/// A string as a NUL-terminated UTF-16 buffer, for Win32.
pub(crate) fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(once(0)).collect()
}

/// ASCII case folding for one UTF-16 unit, for the syntax of a path: a drive letter, a keyword.
pub(crate) const fn fold_ascii(unit: u16) -> u16 {
    if unit >= 0x41 && unit <= 0x5A {
        unit + 32
    } else {
        unit
    }
}

/// The number of units before the terminator, or all of them when there is none.
pub(crate) fn nul_length(units: &[u16]) -> usize {
    units
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units.len())
}

/// Compares two UTF-16 names under the folding rule of `fold_unit`, as Windows compares names.
pub(crate) fn same_folded_wide(left: &[u16], right: &[u16]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| fold_unit(*left) == fold_unit(*right))
}
