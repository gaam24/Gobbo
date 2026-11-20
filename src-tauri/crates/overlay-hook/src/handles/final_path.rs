//! `GetFinalPathNameByHandleW`: a handle taken from a layer answers with its game path, in the form the flags ask for.

use std::ffi::c_void;

use windows_sys::Win32::Storage::FileSystem::{VOLUME_NAME_DOS, VOLUME_NAME_NONE, VOLUME_NAME_NT};

use super::path::{
    VOLUME_NAME_MASK, guest_nt_for_path, guest_nt_for_path_units, map_layer_path_cached,
    map_layer_path_units, original_path, original_path_units, without_volume, without_volume_units,
};
use crate::guard::Guard;
use crate::win32::PreserveLastError;

/// `GetFinalPathNameByHandleW`: a handle to a file in a layer answers with its game path, in the form the flags ask for.
/// Any other handle, and the forms this does not translate, get the answer of Windows.
///
/// # Safety
/// Called by the program with the arguments of `GetFinalPathNameByHandleW`.
pub(crate) unsafe extern "system" fn final_path(
    handle: *mut c_void,
    buffer: *mut u16,
    size: u32,
    flags: u32,
) -> u32 {
    let _guard = Guard::enter();
    let volume = flags & VOLUME_NAME_MASK;
    // The common forms read the DOS path once and stay in UTF-16: a path outside the layers is written back as it is,
    // and a layer path is translated in a stack buffer. No `String`, no second system call. A path too long for the
    // stack, or the GUID form, takes the allocating path below.
    if matches!(volume, VOLUME_NAME_DOS | VOLUME_NAME_NONE | VOLUME_NAME_NT) {
        let mut actual = [0u16; 520];

        if let Some(length) = original_path_units(handle, VOLUME_NAME_DOS, &mut actual) {
            let last_error = PreserveLastError::capture();
            let mut mapped = [0u16; 520];

            if let Some(length) = map_layer_path_units(&actual[..length], &mut mapped) {
                let mut answer = [0u16; 520];
                let translated = match volume {
                    VOLUME_NAME_DOS => Some(&mapped[..length]),
                    VOLUME_NAME_NONE => {
                        without_volume_units(&mapped[..length], &mut answer).map(|n| &answer[..n])
                    }
                    VOLUME_NAME_NT => guest_nt_for_path_units(&mapped[..length], &mut answer)
                        .map(|n| &answer[..n]),
                    _ => None,
                };

                if let Some(path) = translated {
                    drop(last_error);
                    return write_units(buffer, size, path);
                }
            } else {
                let translated = match volume {
                    VOLUME_NAME_DOS => Some(&actual[..length]),
                    VOLUME_NAME_NONE => {
                        without_volume_units(&actual[..length], &mut mapped).map(|n| &mapped[..n])
                    }
                    _ => None,
                };

                if let Some(path) = translated {
                    drop(last_error);
                    return write_units(buffer, size, path);
                }
            }
        }
    }

    let Some(actual) = original_path(handle, flags) else {
        let original = original!(get_final_path, 0);
        // SAFETY: the caller's buffer holds `size` units, and the call writes at most that many.
        let length = unsafe { original.call(handle, buffer, size, flags) };
        let _last_error = PreserveLastError::capture();
        return length;
    };
    let path = translate(handle, &actual, flags).unwrap_or(actual);
    write_path(buffer, size, &path)
}

/// Writes a UTF-16 answer into the program's buffer with the convention of `GetFinalPathNameByHandleW`: the length
/// without the terminator, or the size needed when the buffer is too small.
fn write_units(buffer: *mut u16, size: u32, path: &[u16]) -> u32 {
    let needed = path.len() as u32 + 1;

    if size < needed {
        return needed;
    }
    // SAFETY: the caller's buffer holds `size` units, and `needed <= size` covers the text and its terminator.
    unsafe {
        std::ptr::copy_nonoverlapping(path.as_ptr(), buffer, path.len());
        *buffer.add(path.len()) = 0;
    }

    path.len() as u32
}

/// The game path for the handle, in the form `flags` asks for. `None` when the handle names a path outside the layers,
/// or the form is not translated (the GUID form), so the caller gets the answer of Windows.
fn translate(handle: *mut c_void, actual: &str, flags: u32) -> Option<String> {
    match flags & VOLUME_NAME_MASK {
        VOLUME_NAME_DOS => map_layer_path_cached(actual),
        VOLUME_NAME_NONE => {
            let dos = original_path(handle, VOLUME_NAME_DOS)?;
            let guest = map_layer_path_cached(&dos)?;
            Some(without_volume(&guest))
        }
        VOLUME_NAME_NT => {
            let dos = original_path(handle, VOLUME_NAME_DOS)?;
            let guest = map_layer_path_cached(&dos)?;
            guest_nt_for_path(&guest)
        }
        _ => None,
    }
}

/// Writes `path` to the caller's buffer with the convention of `GetFinalPathNameByHandleW`: the length without the
/// terminator, or the size needed when the buffer is too small.
fn write_path(buffer: *mut u16, size: u32, path: &str) -> u32 {
    let units: Vec<u16> = path.encode_utf16().collect();
    let needed = units.len() as u32 + 1;

    if size < needed {
        return needed;
    }
    // SAFETY: the caller's buffer holds `size` units, and `needed <= size` covers the text and its terminator.
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr(), buffer, units.len());
        *buffer.add(units.len()) = 0;
    }

    units.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_utf16_answer_reports_the_needed_size_and_terminates() {
        let text: Vec<u16> = "abcde".encode_utf16().collect();
        let mut buffer = [0u16; 8];
        assert_eq!(write_units(buffer.as_mut_ptr(), 4, &text), 6);
        assert_eq!(&buffer[..4], &[0, 0, 0, 0]);
        assert_eq!(write_units(buffer.as_mut_ptr(), 8, &text), 5);
        assert_eq!(&buffer[..6], &[97, 98, 99, 100, 101, 0]);
    }

    #[test]
    fn a_volume_is_removed_from_a_drive_and_a_server_path() {
        assert_eq!(without_volume(r"\\?\C:\Game\a.txt"), r"\Game\a.txt");
        assert_eq!(
            without_volume(r"\\?\UNC\server\share\a.txt"),
            r"\\server\share\a.txt"
        );
    }
}
