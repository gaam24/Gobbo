//! `GetMappedFileNameW`: a mapped file from a layer reports the game path, not the staging or upper path. The call
//! returns an NT path (`\Device\HarddiskVolume...`), so the mapping uses the NT prefixes of the layers.

use std::ffi::c_void;

use super::map_layer_path_nt;
use crate::guard::Guard;
use crate::win32::PreserveLastError;

/// `GetMappedFileNameW`: the name of the mapped file, with a layer prefix replaced by the game folder. A file outside
/// the layers, and a name that does not fit the caller's buffer, gets the answer of Windows unchanged.
///
/// # Safety
/// Called by the program with the arguments of `GetMappedFileNameW`.
pub(crate) unsafe extern "system" fn mapped_file_name(
    process: *mut c_void,
    base: *mut c_void,
    buffer: *mut u16,
    size: u32,
) -> u32 {
    let _guard = Guard::enter();
    let original = original!(get_mapped_file_name, 0);
    // SAFETY: the caller passed a process handle, a base address and a buffer of `size` units.
    let length = unsafe { original.call(process, base, buffer, size) };
    let last_error = PreserveLastError::capture();
    if length == 0 || buffer.is_null() || size == 0 {
        return length;
    }
    // SAFETY: a successful call wrote `length` units to the buffer and a terminator after them.
    let units = unsafe { std::slice::from_raw_parts(buffer, length as usize) };
    let Some(mapped) = map_layer_path_nt(units) else {
        return length;
    };
    if mapped.len() >= size as usize {
        // The game path is longer than the buffer Windows just filled. Keep the original answer instead of failing a
        // call that succeeded; no caller of a layer path is expected to make its buffer shorter than the layer path.
        return length;
    }
    // SAFETY: `mapped.len() < size`, and the buffer holds `size` units.
    unsafe {
        std::ptr::copy_nonoverlapping(mapped.as_ptr(), buffer, mapped.len());
        *buffer.add(mapped.len()) = 0;
    }
    drop(last_error);
    mapped.len() as u32
}
