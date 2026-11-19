//! The lookup of what the detours replace: the path of a loaded module, the address of an export, and the detour made on
//! that address.

use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use detour::TypedDetour;
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleW, GetProcAddress, LoadLibraryW,
};

use crate::text::wide;

/// The full path of a loaded module.
pub(crate) fn module_path(module: *mut c_void) -> Option<PathBuf> {
    // The buffer grows until the whole path fits. A path cut short would load the wrong file into the children.
    let mut size = 1024;
    loop {
        let mut buffer = vec![0u16; size];
        // SAFETY: the buffer is `size` UTF-16 units long, and the module handle comes from the loader.
        let length =
            unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), size as u32) } as usize;
        if length == 0 {
            return None;
        }
        if length < size {
            buffer.truncate(length);
            return Some(PathBuf::from(std::ffi::OsString::from_wide(&buffer)));
        }
        size *= 2;
        if size > 32_768 {
            return None;
        }
    }
}

/// A detour that replaces the export `export` of `module` with `hook`.
///
/// # Safety
/// `F` must be the function type of that export, and `hook` must have the same signature.
pub(crate) unsafe fn detour<F: detour::Function>(
    module: &str,
    export: &str,
    hook: F,
) -> Result<TypedDetour<F>, String> {
    unsafe {
        let target = target::<F>(module, export)?;
        TypedDetour::new(target, hook).map_err(|error| error.to_string())
    }
}

/// The address of an export of `module`, as the function type `F`.
///
/// # Safety
/// `F` must be the function type of that export.
unsafe fn target<F: Copy>(module: &str, export: &str) -> Result<F, String> {
    let module_name = wide(OsStr::new(module));
    let mut bytes = export.as_bytes().to_vec();
    bytes.push(0);
    unsafe {
        let mut handle = GetModuleHandleW(module_name.as_ptr());
        if handle.is_null() {
            // An optional module (`advapi32` for `CreateProcessAsUserW`) may not be loaded yet. Loading it here, on the
            // install thread, is outside the loader lock.
            handle = LoadLibraryW(module_name.as_ptr());
        }
        let Some(address) = GetProcAddress(handle, bytes.as_ptr()) else {
            return Err(format!("{module} has no {export}"));
        };
        Ok(std::mem::transmute_copy::<_, F>(&address))
    }
}
