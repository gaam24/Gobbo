//! The calls that read what a handle names: the file information by handle and the NT file information. They are not
//! redirected, because a handle already answers with the file it was opened on. They are traced, so the checks the game
//! makes on its own files can be followed in a dev build.
//!
//! `NtQueryInformationFile(FileNameInformation)` answers with the game path for a handle of a layer. The hook asks
//! Windows for the layer path itself through the original `GetFinalPathNameByHandleW`, under the guard, so this rewrite
//! never changes the path the hook uses to classify a handle.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::STATUS_UNSUCCESSFUL;
use windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION;

use crate::diag;
use crate::guard::Guard;
use crate::handles;
use crate::trace::trace;
use crate::win32::PreserveLastError;

/// `GetFileInformationByHandle`: the details of the file a handle names, as the program gets them.
///
/// # Safety
/// Called by the program with the arguments of `GetFileInformationByHandle`.
pub(crate) unsafe extern "system" fn get_file_information(
    handle: *mut c_void,
    info: *mut BY_HANDLE_FILE_INFORMATION,
) -> i32 {
    // The trace writer asks for the size of its own file with this call. Without the guard, the hook would trace that
    // query, the writer would write the line, and ask again: an endless loop. The guard also keeps every other call
    // the hook makes itself out of the trace.
    let guard = Guard::enter();
    let nested = guard.is_none();
    let original = original!(get_file_information, 0);
    let succeeded = unsafe { original.call(handle, info) };
    let last_error = PreserveLastError::capture();

    if !nested && diag::settings().calls {
        trace(|| match unsafe { info.as_ref() } {
            Some(info) if succeeded != 0 => format!(
                "result GetFileInformationByHandle attr=0x{:08x} volume={} index={}:{} size={} write={}:{} links={}",
                info.dwFileAttributes,
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
                (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
                info.ftLastWriteTime.dwHighDateTime,
                info.ftLastWriteTime.dwLowDateTime,
                info.nNumberOfLinks,
            ),
            _ => format!(
                "result GetFileInformationByHandle failed error={}",
                last_error.value()
            ),
        });
    }

    succeeded
}

/// `NtQueryInformationFile`: the file information of a handle, by information class. The class and the status are traced.
///
/// # Safety
/// Called by the program with the arguments of `NtQueryInformationFile`.
pub(crate) unsafe extern "system" fn nt_query_information_file(
    handle: *mut c_void,
    status_block: *mut c_void,
    info: *mut c_void,
    length: u32,
    class: u32,
) -> i32 {
    let guard = Guard::enter();
    let nested = guard.is_none();
    let original = original!(nt_query_information_file, STATUS_UNSUCCESSFUL);
    let status = unsafe { original.call(handle, status_block, info, length, class) };
    let _last_error = PreserveLastError::capture();

    if !nested {
        // SAFETY: `info` is the caller's buffer of `length` bytes, as `NtQueryInformationFile` requires.
        unsafe { map_file_name(status, info, length, class) };
    }
    // Built only when the trace is on: the program calls this for every file it inspects.
    if !nested && diag::settings().calls {
        trace(|| {
            format!(
                "status NtQueryInformationFile class={class} 0x{:08x}",
                status as u32
            )
        });
    }

    status
}

/// `FileInformationClass` values whose answer is a `FILE_NAME_INFORMATION` with the NT path of the file.
const FILE_NAME_INFORMATION: u32 = 9;
const FILE_NORMALIZED_NAME_INFORMATION: u32 = 48;
/// `FILE_NAME_INFORMATION.FileName` starts after the 4-byte length; the structure is aligned to 4 bytes.
const FILE_NAME_OFFSET: usize = 4;

/// Rewrites the name in a successful `FILE_NAME_INFORMATION` answer when the handle points into a layer. The game path
/// is normally shorter than the staging path, so it fits; a longer one leaves the original answer untouched instead of
/// turning a successful call into `STATUS_BUFFER_OVERFLOW`.
///
/// # Safety
/// `info` must point to at least `length` writable bytes when `length` is not zero.
unsafe fn map_file_name(status: i32, info: *mut c_void, length: u32, class: u32) {
    if status < 0
        || !matches!(
            class,
            FILE_NAME_INFORMATION | FILE_NORMALIZED_NAME_INFORMATION
        )
        || info.is_null()
    {
        return;
    }
    // SAFETY: the original call succeeded and filled the length word of the structure.
    let file_name_length = unsafe { std::ptr::read_unaligned(info.cast::<u32>()) } as usize;

    if file_name_length == 0 || !file_name_length.is_multiple_of(2) {
        return;
    }

    let capacity = (length as usize).saturating_sub(FILE_NAME_OFFSET);

    if file_name_length > capacity {
        return;
    }
    // SAFETY: the original call wrote `file_name_length` bytes of name after the header, and the buffer is `length`
    // bytes, so `file_name_length <= capacity` keeps the slice inside it.
    let name = unsafe {
        std::slice::from_raw_parts(
            info.cast::<u8>().add(FILE_NAME_OFFSET).cast::<u16>(),
            file_name_length / 2,
        )
    };
    let Some(mapped) = handles::map_layer_path_nt(name) else {
        return;
    };
    let mapped_bytes = mapped.len().saturating_mul(2);

    if mapped_bytes > capacity {
        return;
    }
    // SAFETY: the mapped name fits the caller's buffer. A terminator is added when there is room for it, so a caller that
    // reads the buffer as a string sees the end of the game path, not the tail of the layer path.
    unsafe {
        std::ptr::copy_nonoverlapping(
            mapped.as_ptr(),
            info.cast::<u8>().add(FILE_NAME_OFFSET).cast::<u16>(),
            mapped.len(),
        );
        std::ptr::write_unaligned(info.cast::<u32>(), mapped_bytes as u32);

        if mapped_bytes + 2 <= capacity {
            *info
                .cast::<u8>()
                .add(FILE_NAME_OFFSET + mapped_bytes)
                .cast::<u16>() = 0;
        }
    }
}
