//! Static imports of the program executable.
//!
//! The Windows loader resolves the executable's static imports before the program's entry point. A mod DLL that the
//! executable imports by name (for example `version.dll` next to it) is normally looked up in the game folder. To keep
//! that folder clean, the launcher rewrites the import descriptor to the full path of the DLL in the mod layer. The
//! loader then reads the file from the layer, and the hook maps the loaded module back to the game path.
//!
//! This is separate from the NT hook: the hook covers every DLL the program loads at runtime, while this covers the ones
//! the loader resolves before any program code runs.

use std::ffi::{OsStr, c_void};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::SystemTime;

use windows_sys::Win32::Foundation::{FALSE, HANDLE};
use windows_sys::Win32::Globalization::{CP_ACP, WideCharToMultiByte};
use windows_sys::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
    VirtualProtectEx,
};
use windows_sys::Win32::System::Threading::PROCESS_BASIC_INFORMATION;

use crate::wide;

use gobbo_overlay_pe::Import;

/// The parsed parts of one executable: its static imports and its bitness, both taken from the same read. A launch
/// asks for them twice (the report and the import patch); the executable may be hundreds of megabytes, so one cache
/// entry keyed by its length and modification time saves a second full read.
struct Parsed {
    imports: Vec<Import>,
    pe32: bool,
}

/// The program path, its length and modification time, and the parse of that exact file.
type CachedParse = (PathBuf, u64, SystemTime, Arc<Parsed>);

/// A game can start several helpers with different executables; a one-entry cache would read each one again when the
/// next child starts. The list is small, so a linear lookup stays cheap.
const CACHE_ENTRIES: usize = 8;

static PARSED: OnceLock<Mutex<Vec<CachedParse>>> = OnceLock::new();

fn cache() -> &'static Mutex<Vec<CachedParse>> {
    PARSED.get_or_init(|| Mutex::new(Vec::new()))
}

/// The static imports of a program, cloned from the one-entry cache. The list is small; the executable is not held.
pub fn imports(program: &Path) -> io::Result<Vec<Import>> {
    Ok(parsed(program)?.imports.clone())
}

/// The parsed executable, from the cache when its identity did not change.
fn parsed(program: &Path) -> io::Result<Arc<Parsed>> {
    let metadata = std::fs::metadata(program)?;
    let len = metadata.len();
    let modified = metadata.modified()?;
    {
        let entry = cache().lock().unwrap_or_else(PoisonError::into_inner);

        if let Some((_, _, _, parsed)) =
            entry.iter().find(|(path, cached_len, cached_modified, _)| {
                path == program && *cached_len == len && *cached_modified == modified
            })
        {
            return Ok(Arc::clone(parsed));
        }
    }

    let parsed = Arc::new(parse_file(program)?);
    let mut entry = cache().lock().unwrap_or_else(PoisonError::into_inner);
    entry.retain(|(path, _, _, _)| path != program);

    if entry.len() >= CACHE_ENTRIES {
        entry.remove(0);
    }

    entry.push((program.to_path_buf(), len, modified, Arc::clone(&parsed)));
    Ok(parsed)
}

fn parse_file(program: &Path) -> io::Result<Parsed> {
    let bytes = std::fs::read(program)?;
    let (pe32, imports) = gobbo_overlay_pe::parse(&bytes).map_err(malformed)?;
    Ok(Parsed { imports, pe32 })
}

/// Rewrites the import descriptors of the suspended program so the loader reads each `mapping` DLL from the mod layer.
/// The mapping keys are the import names returned by `imports`; a key without a matching descriptor is ignored.
///
/// # Safety
/// `process` must be the suspended program's process handle, with the rights `CreateProcessW` returns.
pub unsafe fn rewrite(
    process: HANDLE,
    program: &Path,
    mapping: &[(String, PathBuf)],
) -> io::Result<usize> {
    if mapping.is_empty() {
        return Ok(0);
    }

    let parsed = parsed(program)?;
    let imports = &parsed.imports;
    let pe32 = parsed.pe32;
    let base = main_module(process, pe32)?;
    // The image is smaller than 2 GiB; this start is inside the 32-bit RVA range and clear of the usual image.
    let image_size = 0x1000_0000u32;
    let mut patched = 0;

    for (name, path) in mapping {
        let Some(import) = imports
            .iter()
            .find(|import| import.name.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let path = ansi(path)?;
        let Some(remote) = allocate_near(process, base, image_size, path.len() + 1) else {
            continue;
        };
        // SAFETY: `remote` is a fresh allocation of `path.len() + 1` bytes in the target.
        let mut written = 0;

        if unsafe {
            WriteProcessMemory(
                process,
                remote,
                path.as_ptr().cast(),
                path.len() + 1,
                &mut written,
            )
        } == FALSE
            || written != path.len() + 1
        {
            free_remote(process, remote);
            return Err(io::Error::last_os_error());
        }

        let name_rva = (remote as usize)
            .checked_sub(base)
            .and_then(|offset| u32::try_from(offset).ok());
        let Some(name_rva) = name_rva else {
            free_remote(process, remote);
            continue;
        };
        let descriptor = base + import.descriptor_rva as usize + 12;
        let mut old = 0;
        // SAFETY: the descriptor is inside the mapped image, and the protection change covers its four-byte name field.
        if unsafe {
            VirtualProtectEx(
                process,
                descriptor as *const c_void,
                4,
                PAGE_READWRITE,
                &mut old,
            )
        } == FALSE
        {
            free_remote(process, remote);
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the four bytes are the descriptor's `Name` field.
        let written = unsafe {
            WriteProcessMemory(
                process,
                descriptor as *const c_void,
                (&raw const name_rva).cast(),
                4,
                &mut written,
            )
        };
        // SAFETY: the old protection is restored whether the write succeeded or not.
        unsafe {
            VirtualProtectEx(process, descriptor as *const c_void, 4, old, &mut old);
        }

        if written == FALSE {
            free_remote(process, remote);
            return Err(io::Error::last_os_error());
        }

        patched += 1;
    }

    Ok(patched)
}

/// Frees a remote allocation of the suspended process.
fn free_remote(process: HANDLE, remote: *mut c_void) {
    // SAFETY: the allocation is not used after this point.
    unsafe { VirtualFreeEx(process, remote, 0, MEM_RELEASE) };
}

/// The base address of the process's main module. The module list is not initialized before the loader runs, so the
/// address comes from the PEB, which the kernel fills in at process creation.
fn main_module(process: HANDLE, pe32: bool) -> io::Result<usize> {
    type NtQueryInformationProcessFn =
        unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> i32;
    const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;
    let ntdll = wide(OsStr::new("ntdll.dll"));
    // SAFETY: ntdll is loaded in every Windows process, and the name is NUL-terminated.
    let module = unsafe { GetModuleHandleW(ntdll.as_ptr()) };

    if module.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `module` is ntdll and the export name is NUL-terminated.
    let Some(procedure) =
        (unsafe { GetProcAddress(module, c"NtQueryInformationProcess".as_ptr().cast()) })
    else {
        return Err(io::Error::last_os_error());
    };
    // SAFETY: `NtQueryInformationProcess` has this signature; the pointer comes from the export.
    let query: NtQueryInformationProcessFn = unsafe { std::mem::transmute_copy(&procedure) };
    let mut returned = 0;
    let peb = if pe32 && cfg!(target_pointer_width = "64") {
        // A WOW64 process has a 64-bit PEB and a 32-bit PEB. `ProcessWow64Information` returns the address of the
        // latter; `ProcessBasicInformation` would name the 64-bit one. A native 32-bit build has only the 32-bit PEB.
        const PROCESS_WOW64_INFORMATION_CLASS: u32 = 26;
        let mut address: usize = 0;
        // SAFETY: the buffer is the pointer-sized value the class returns, and the process handle is open.
        let status = unsafe {
            query(
                process,
                PROCESS_WOW64_INFORMATION_CLASS,
                (&raw mut address).cast(),
                std::mem::size_of::<usize>() as u32,
                &mut returned,
            )
        };

        if status < 0 {
            return Err(io::Error::other(format!(
                "NtQueryInformationProcess(ProcessWow64Information) failed with 0x{status:08x}"
            )));
        }

        address
    } else {
        let mut information: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the information structure is the size the call expects, and the process handle is open.
        let status = unsafe {
            query(
                process,
                PROCESS_BASIC_INFORMATION_CLASS,
                (&raw mut information).cast(),
                std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                &mut returned,
            )
        };

        if status < 0 {
            return Err(io::Error::other(format!(
                "NtQueryInformationProcess failed with 0x{status:08x}"
            )));
        }

        information.PebBaseAddress as usize
    };

    if peb == 0 {
        return Err(io::Error::other("the process has no PEB address"));
    }
    // ImageBaseAddress follows the process and thread pointers: 0x10 on x64, 0x8 on x86.
    let (offset, size) = if pe32 { (0x08, 4) } else { (0x10, 8) };
    let mut buffer = [0u8; 8];
    let mut read = 0;
    // SAFETY: the PEB page is mapped, and the buffer has room for the pointer.
    if unsafe {
        ReadProcessMemory(
            process,
            (peb + offset) as *const c_void,
            buffer.as_mut_ptr().cast(),
            size,
            &mut read,
        )
    } == FALSE
        || read != size
    {
        return Err(io::Error::last_os_error());
    }

    let base = if pe32 {
        u32::from_le_bytes(buffer[..4].try_into().map_err(|_| invalid())?) as usize
    } else {
        u64::from_le_bytes(buffer)
            .try_into()
            .map_err(|_| invalid())?
    };
    Ok(base)
}

/// Allocates `size` bytes in the target close enough to `base` that the offset fits in the 32-bit import name RVA.
fn allocate_near(
    process: HANDLE,
    base: usize,
    image_size: u32,
    size: usize,
) -> Option<*mut c_void> {
    let mut preferred = base.checked_add(image_size as usize)?.checked_add(0xFFFF)? & !0xFFFF;

    for _ in 0..32 {
        // SAFETY: the address is a hint; Windows returns a new allocation or null.
        let remote = unsafe {
            VirtualAllocEx(
                process,
                preferred as *const c_void,
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };

        if !remote.is_null() {
            let fits = (remote as usize)
                .checked_sub(base)
                .is_some_and(|offset| u32::try_from(offset).is_ok());
            if fits {
                return Some(remote);
            }

            free_remote(process, remote);
        }

        preferred = preferred.checked_add(0x10000)?;
    }

    None
}

/// Whether the image is PE32 (x86) rather than PE32+ (x64). The launcher itself reads `Parsed::pe32`.
/// The path in the system ANSI code page, with a NUL terminator.
fn ansi(path: &Path) -> io::Result<Vec<u8>> {
    let wide = wide(path.as_os_str());
    // SAFETY: the length is the number of units, and a null output asks for the needed size.
    let needed = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            wide.as_ptr(),
            wide.len() as i32 - 1,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if needed <= 0 {
        return Err(io::Error::last_os_error());
    }

    let mut bytes = vec![0u8; needed as usize + 1];
    // SAFETY: `bytes` has room for the converted text and the terminator.
    let written = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            wide.as_ptr(),
            wide.len() as i32 - 1,
            bytes.as_mut_ptr(),
            needed,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if written <= 0 {
        return Err(io::Error::last_os_error());
    }

    bytes.truncate(written as usize);
    bytes.push(0);
    Ok(bytes)
}

/// A malformed PE image.
/// The error of a malformed program image, for the callers that report `io::Error`.
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "the program has a malformed PE header",
    )
}

/// The error of a malformed image read by `gobbo_overlay_pe`.
fn malformed(_: gobbo_overlay_pe::Malformed) -> io::Error {
    invalid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_program_has_a_readable_import_table() {
        let program = std::env::current_exe().unwrap();
        let imports = imports(&program).unwrap();
        assert!(
            imports
                .iter()
                .any(|import| import.name.eq_ignore_ascii_case("kernel32.dll"))
        );
        assert!(imports.iter().all(|import| import.descriptor_rva > 0));
    }

    #[test]
    fn the_test_program_is_not_reported_as_pe32_when_the_build_is_x64() {
        let program = std::env::current_exe().unwrap();
        assert_eq!(
            parsed(&program).unwrap().pe32,
            cfg!(target_pointer_width = "32")
        );
    }
}
