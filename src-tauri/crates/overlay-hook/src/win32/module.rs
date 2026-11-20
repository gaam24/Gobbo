//! `GetModuleFileNameW` / `A`: a DLL loaded from a layer reports the game path, so a program that compares its own folder
//! with the game folder does not see the overlay state folder.

use std::ffi::c_void;
use std::ptr::null_mut;

use detour::TypedDetour;
use gobbo_overlay_core::Layers;
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_FUNCTION};
use windows_sys::Win32::Globalization::{CP_ACP, WideCharToMultiByte};

use super::{PreserveLastError, fail, set_error};
use crate::guard::Guard;
use crate::handles;
use crate::state::layers;
use crate::types::{GetModuleFileNameExWFn, GetModuleFileNameWFn};

/// `GetModuleFileNameW`: the real path of the module, with a layer prefix replaced by the game folder.
///
/// # Safety
/// Called by the program with the arguments of `GetModuleFileNameW`.
pub(crate) unsafe extern "system" fn get_module_file_name(
    module: *mut c_void,
    buffer: *mut u16,
    size: u32,
) -> u32 {
    let _guard = Guard::enter();
    let original = original!(get_module_file_name, {
        fail(ERROR_INVALID_FUNCTION) as u32
    });
    // The common path stays in UTF-16: map a layer prefix in a stack buffer, or copy the program's path as it is. No
    // `String` and no `Vec`. A path longer than the stack takes the growing path below.
    let mut actual = [0u16; 1024];

    if let Some(length) = unsafe { read_path_units(original, module, &mut actual) } {
        return copy_mapped_module(&actual[..length], buffer, size);
    }
    // SAFETY: `original` is the Windows function and `module` is the handle the program passed.
    let Some(answer) = guest_long_path(unsafe { read_path(original, module) }) else {
        return 0;
    };
    copy(&answer, buffer, size)
}

/// `GetModuleFileNameA`: the ANSI form of the same answer. The path is read through the original `W` function and
/// converted with the system ANSI code page, so a non-ASCII path keeps the bytes `A` would have produced.
///
/// # Safety
/// Called by the program with the arguments of `GetModuleFileNameA`.
pub(crate) unsafe extern "system" fn get_module_file_name_a(
    module: *mut c_void,
    buffer: *mut u8,
    size: u32,
) -> u32 {
    let _guard = Guard::enter();
    let original = original!(get_module_file_name, {
        fail(ERROR_INVALID_FUNCTION) as u32
    });
    let mut actual = [0u16; 1024];

    if let Some(length) = unsafe { read_path_units(original, module, &mut actual) } {
        return copy_mapped_module_ansi(&actual[..length], buffer, size);
    }
    // SAFETY: `original` is the Windows function and `module` is the handle the program passed.
    let Some(answer) = guest_long_path_ansi(unsafe { read_path(original, module) }) else {
        return 0;
    };
    copy_a(&answer, buffer, size)
}

/// The game path of a module path read with a growing buffer: a path in a layer mapped back as text, any other path as
/// it is. `None` when Windows had no path. The program's last error from the read is restored before it returns, so the
/// truncation error a copy sets afterwards is not overwritten.
fn guest_long_path(path: Option<Vec<u16>>) -> Option<Vec<u16>> {
    let _last_error = PreserveLastError::capture();
    let path = path?;
    let mapped = layers().and_then(|layers| map(&String::from_utf16_lossy(&path), layers));
    Some(mapped.map_or(path, |mapped| mapped.encode_utf16().collect()))
}

/// The ANSI form of the growing path: the game path as text, converted with the system ANSI code page.
fn guest_long_path_ansi(path: Option<Vec<u16>>) -> Option<Vec<u8>> {
    let wide = guest_long_path(path)?;
    let _last_error = PreserveLastError::capture();
    ansi(&wide)
}

/// The path of `module` into `buffer` when it fits. `None` means the caller must use the growing `read_path`.
///
/// # Safety
/// `original` must be the original `GetModuleFileNameW`, and `module` a handle it accepts.
unsafe fn read_path_units(
    original: &TypedDetour<GetModuleFileNameWFn>,
    module: *mut c_void,
    buffer: &mut [u16; 1024],
) -> Option<usize> {
    // SAFETY: the buffer is `buffer.len()` units long, and the call writes at most that many.
    unsafe { read_path_units_with(|buffer, size| original.call(module, buffer, size), buffer) }
}

/// The shared body of `read_path_units` and `read_ex_path_units`.
///
/// # Safety
/// `call` must write at most `size` units into the buffer it is given.
unsafe fn read_path_units_with(
    mut call: impl FnMut(*mut u16, u32) -> u32,
    buffer: &mut [u16; 1024],
) -> Option<usize> {
    // SAFETY: the buffer is `buffer.len()` units long, and the call writes at most that many.
    let length = call(buffer.as_mut_ptr(), buffer.len() as u32);
    let length = length as usize;
    (length != 0 && length < buffer.len()).then_some(length)
}

/// The game path of a module path in a layer, in the caller's buffer; the actual path when it is outside every layer.
fn guest_module_units<'a>(actual: &'a [u16], mapped: &'a mut [u16; 1024]) -> &'a [u16] {
    match handles::guest_module_path_units(actual, mapped) {
        Some(length) => &mapped[..length],
        None => actual,
    }
}

/// Copies the game path of a module path in a layer into the program's UTF-16 buffer.
fn copy_mapped_module(actual: &[u16], buffer: *mut u16, size: u32) -> u32 {
    let last_error = PreserveLastError::capture();
    let mut mapped = [0u16; 1024];
    let answer = guest_module_units(actual, &mut mapped);
    // The truncation path sets `ERROR_INSUFFICIENT_BUFFER`; the guard must not restore the original error over it.
    drop(last_error);
    copy(answer, buffer, size)
}

/// Copies the game path of a module path in a layer into the program's ANSI buffer.
fn copy_mapped_module_ansi(actual: &[u16], buffer: *mut u8, size: u32) -> u32 {
    let last_error = PreserveLastError::capture();
    let mut mapped = [0u16; 1024];
    let answer = guest_module_units(actual, &mut mapped);
    let mut bytes = [0u8; 2048];

    if let Some(length) = ansi_units(answer, &mut bytes) {
        drop(last_error);
        return copy_a(&bytes[..length], buffer, size);
    }

    let Some(answer) = ansi(answer) else {
        return 0;
    };
    drop(last_error);
    copy_a(&answer, buffer, size)
}

/// The path of `module` with the original `W` function, growing a stack buffer when the path does not fit.
///
/// # Safety
/// `original` must be the original `GetModuleFileNameW`, and `module` a handle it accepts.
unsafe fn read_path(
    original: &TypedDetour<GetModuleFileNameWFn>,
    module: *mut c_void,
) -> Option<Vec<u16>> {
    unsafe { read_path_with(|buffer, size| original.call(module, buffer, size)) }
}

/// `GetModuleFileNameExW`: the path of a module of the process the caller names, mapped back from a layer.
///
/// # Safety
/// Called by the program with the arguments of `GetModuleFileNameExW`.
pub(crate) unsafe extern "system" fn get_module_file_name_ex(
    process: *mut c_void,
    module: *mut c_void,
    buffer: *mut u16,
    size: u32,
) -> u32 {
    let _guard = Guard::enter();
    let original = original!(get_module_file_name_ex, {
        fail(ERROR_INVALID_FUNCTION) as u32
    });
    let mut actual = [0u16; 1024];

    if let Some(length) = unsafe { read_ex_path_units(original, process, module, &mut actual) } {
        return copy_mapped_module(&actual[..length], buffer, size);
    }
    // SAFETY: `original` is the Windows function and `process`/`module` are the handles the program passed.
    let Some(answer) = guest_long_path(unsafe { read_ex_path(original, process, module) }) else {
        return 0;
    };
    copy(&answer, buffer, size)
}

/// `GetModuleFileNameExA`: the ANSI form of the same answer.
///
/// # Safety
/// Called by the program with the arguments of `GetModuleFileNameExA`.
pub(crate) unsafe extern "system" fn get_module_file_name_ex_a(
    process: *mut c_void,
    module: *mut c_void,
    buffer: *mut u8,
    size: u32,
) -> u32 {
    let _guard = Guard::enter();
    let original = original!(get_module_file_name_ex, {
        fail(ERROR_INVALID_FUNCTION) as u32
    });
    let mut actual = [0u16; 1024];

    if let Some(length) = unsafe { read_ex_path_units(original, process, module, &mut actual) } {
        return copy_mapped_module_ansi(&actual[..length], buffer, size);
    }
    // SAFETY: `original` is the Windows function and `process`/`module` are the handles the program passed.
    let Some(answer) = guest_long_path_ansi(unsafe { read_ex_path(original, process, module) })
    else {
        return 0;
    };
    copy_a(&answer, buffer, size)
}

/// The path of `module` in `process` into `buffer` when it fits. `None` means the caller must use `read_ex_path`.
///
/// # Safety
/// `original` must be the original `GetModuleFileNameExW`, and the handles must be ones it accepts.
unsafe fn read_ex_path_units(
    original: &TypedDetour<GetModuleFileNameExWFn>,
    process: *mut c_void,
    module: *mut c_void,
    buffer: &mut [u16; 1024],
) -> Option<usize> {
    // SAFETY: the buffer is `buffer.len()` units long, and the call writes at most that many.
    unsafe {
        read_path_units_with(
            |buffer, size| original.call(process, module, buffer, size),
            buffer,
        )
    }
}

/// The path of `module` in `process` with the original `GetModuleFileNameExW`, growing a stack buffer when needed.
///
/// # Safety
/// `original` must be the original `GetModuleFileNameExW`, and the handles must be ones it accepts.
unsafe fn read_ex_path(
    original: &TypedDetour<GetModuleFileNameExWFn>,
    process: *mut c_void,
    module: *mut c_void,
) -> Option<Vec<u16>> {
    unsafe { read_path_with(|buffer, size| original.call(process, module, buffer, size)) }
}

/// Reads a path with a growing buffer. `call` makes the original call with the buffer and its size.
///
/// # Safety
/// `call` must write at most `size` units into the buffer it is given.
unsafe fn read_path_with(mut call: impl FnMut(*mut u16, u32) -> u32) -> Option<Vec<u16>> {
    let mut stack = [0u16; 1024];
    let mut length = call(stack.as_mut_ptr(), stack.len() as u32);

    if length == 0 {
        return None;
    }

    if (length as usize) < stack.len() {
        return Some(stack[..length as usize].to_vec());
    }

    let mut capacity = stack.len() * 2;

    loop {
        let mut buffer = vec![0u16; capacity];
        length = call(buffer.as_mut_ptr(), capacity as u32);

        if length == 0 {
            return None;
        }

        if (length as usize) < capacity {
            buffer.truncate(length as usize);
            return Some(buffer);
        }

        capacity *= 2;

        if capacity > 32_768 {
            return None;
        }
    }
}

/// The game path of a layer path, in the DOS form the call returns.
fn map(path: &str, layers: &Layers) -> Option<String> {
    let verbatim = match path.strip_prefix(r"\\") {
        Some(server) => format!(r"\\?\UNC\{server}"),
        None => format!(r"\\?\{path}"),
    };
    handles::map_layer_path_cached(&verbatim)
        .or_else(|| handles::map_layer_path(&verbatim, layers))
        .map(|mapped| mapped.strip_prefix(r"\\?\").unwrap_or(&mapped).to_string())
}

/// Converts a UTF-16 path to the system ANSI code page in a caller's buffer. No allocation; `None` when the buffer is
/// too small or the path cannot be represented.
fn ansi_units(path: &[u16], out: &mut [u8]) -> Option<usize> {
    // SAFETY: `out` is `out.len()` bytes long, and the call writes at most that many.
    let written = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            path.as_ptr(),
            path.len() as i32,
            out.as_mut_ptr(),
            out.len() as i32,
            null_mut(),
            null_mut(),
        )
    };
    (written > 0).then_some(written as usize)
}

/// Converts a UTF-16 path to the system ANSI code page, growing a buffer for a path longer than `ansi_units` handles.
fn ansi(path: &[u16]) -> Option<Vec<u8>> {
    // SAFETY: the length is the number of units in `path`, and a null output asks for the needed size.
    let needed = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            path.as_ptr(),
            path.len() as i32,
            null_mut(),
            0,
            null_mut(),
            null_mut(),
        )
    };

    if needed <= 0 {
        return None;
    }

    let mut buffer = vec![0u8; needed as usize];
    // SAFETY: `buffer` is `needed` bytes long, as the call above asked.
    let written = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            path.as_ptr(),
            path.len() as i32,
            buffer.as_mut_ptr(),
            needed,
            null_mut(),
            null_mut(),
        )
    };

    if written <= 0 {
        return None;
    }

    buffer.truncate(written as usize);
    Some(buffer)
}

/// Copies the answer into the program's buffer with the truncation rules of `GetModuleFileNameW`.
fn copy(path: &[u16], buffer: *mut u16, size: u32) -> u32 {
    copy_units(path, buffer, size)
}

/// Copies the ANSI answer with the truncation rules of `GetModuleFileNameA`: the size is in bytes.
fn copy_a(path: &[u8], buffer: *mut u8, size: u32) -> u32 {
    copy_units(path, buffer, size)
}

/// A value whose zero is the terminator of a Win32 buffer.
trait Zero {
    const ZERO: Self;
}

impl Zero for u16 {
    const ZERO: Self = 0;
}

impl Zero for u8 {
    const ZERO: Self = 0;
}

fn copy_units<T: Copy + Zero>(path: &[T], buffer: *mut T, size: u32) -> u32 {
    if size == 0 {
        return 0;
    }

    let size = size as usize;
    let count = path.len().min(size - 1);
    // SAFETY: the caller passed a buffer of `size` units, and `count + 1` of them are written.
    unsafe {
        std::ptr::copy_nonoverlapping(path.as_ptr(), buffer, count);
        *buffer.add(count) = T::ZERO;
    }

    if path.len() >= size {
        set_error(ERROR_INSUFFICIENT_BUFFER);
        size as u32
    } else {
        path.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> Layers {
        Layers {
            root: std::path::PathBuf::from(r"C:\Game"),
            staging: Some(std::path::PathBuf::from(r"C:\State\mods")),
            upper: std::path::PathBuf::from(r"C:\State\overwrite"),
        }
    }

    #[test]
    fn a_module_in_a_layer_reports_the_game_path() {
        assert_eq!(
            map(r"C:\State\overwrite\bin\mod.dll", &layers()).as_deref(),
            Some(r"C:\Game\bin\mod.dll")
        );
        assert_eq!(
            map(r"C:\State\mods\bin\mod.dll", &layers()).as_deref(),
            Some(r"C:\Game\bin\mod.dll")
        );
        assert_eq!(map(r"C:\Other\mod.dll", &layers()), None);
    }

    #[test]
    fn a_small_buffer_is_filled_to_its_last_unit_and_reports_failure() {
        let text: Vec<u16> = "abcde".encode_utf16().collect();
        let mut buffer = [0u16; 4];
        assert_eq!(copy(&text, buffer.as_mut_ptr(), 4), 4);
        assert_eq!(buffer, [97, 98, 99, 0]);
    }

    #[test]
    fn a_buffer_that_fits_gets_the_whole_path() {
        let text: Vec<u16> = "abc".encode_utf16().collect();
        let mut buffer = [0u16; 4];
        assert_eq!(copy(&text, buffer.as_mut_ptr(), 4), 3);
        assert_eq!(buffer, [97, 98, 99, 0]);
    }

    #[test]
    fn an_ansi_path_is_converted_in_a_stack_buffer() {
        let path: Vec<u16> = r"C:\Game\a.txt".encode_utf16().collect();
        let mut out = [0u8; 64];
        let length = ansi_units(&path, &mut out).unwrap();
        assert_eq!(&out[..length], br"C:\Game\a.txt");
        let mut small = [0u8; 2];
        assert!(ansi_units(&path, &mut small).is_none());
    }

    #[test]
    fn the_ansi_copy_uses_bytes_and_terminates_the_buffer() {
        let text = b"abcde";
        let mut buffer = [0u8; 4];
        assert_eq!(copy_a(text, buffer.as_mut_ptr(), 4), 4);
        assert_eq!(buffer, [b'a', b'b', b'c', 0]);
        let mut buffer = [0u8; 8];
        assert_eq!(copy_a(text, buffer.as_mut_ptr(), 8), 5);
        assert_eq!(&buffer[..6], b"abcde\0");
    }
}
