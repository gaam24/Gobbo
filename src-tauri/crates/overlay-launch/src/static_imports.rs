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
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

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

/// The parsed parts of one executable: its static imports and its bitness, from the same read.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Parsed {
    imports: Vec<Import>,
    pe32: bool,
}

/// How much of the program to read before deciding that the whole file is needed. The PE headers, the section table
/// and the import directory sit near the start even of a game executable hundreds of megabytes in size. One extra
/// byte is read so a file that is exactly this long is still told apart from a longer one.
const PREFIX_BYTES: usize = 16 * 1024 * 1024;

/// Reads at most `limit` bytes plus one sentinel byte, parses the prefix, and reads the whole file only when the
/// parser says it was cut off. A prefix that ends before the import data is never accepted as an empty import table:
/// [`gobbo_overlay_pe::parse_prefix`] reports `NeedMore` for that case, and the full read then decides.
fn parse_file_limited(program: &Path, limit: usize) -> io::Result<Parsed> {
    let mut file = File::open(program)?;
    let mut bytes = Vec::new();
    // The extra byte says whether the file continues past the prefix without reading more of it.
    file.by_ref()
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;

    if bytes.len() <= limit {
        // The whole file fitted in the read, so there is no prefix to decide about.
        return parsed(gobbo_overlay_pe::parse(&bytes).map_err(malformed)?);
    }

    match gobbo_overlay_pe::parse_prefix(&bytes[..limit]) {
        gobbo_overlay_pe::PrefixParse::Complete(result) => parsed(result.map_err(malformed)?),
        gobbo_overlay_pe::PrefixParse::NeedMore => {
            // Continue on the same handle: a file replaced or changed between two opens could not give a mixed
            // answer from the prefix and the rest of the image.
            file.read_to_end(&mut bytes)?;
            parsed(gobbo_overlay_pe::parse(&bytes).map_err(malformed)?)
        }
    }
}

fn parse_file(program: &Path) -> io::Result<Parsed> {
    parse_file_limited(program, PREFIX_BYTES)
}

fn parsed((pe32, imports): (bool, Vec<Import>)) -> io::Result<Parsed> {
    Ok(Parsed { imports, pe32 })
}

/// The static imports of a program. The bounded read makes parsing on each launch cheap, so no cache is needed.
pub fn imports(program: &Path) -> io::Result<Vec<Import>> {
    Ok(parse_file(program)?.imports)
}

/// The RVAs of every import descriptor whose DLL name matches `name`, in table order. A legal PE can have two
/// descriptors for one DLL; leaving either one bound to the game folder defeats the redirect.
fn descriptor_rvas(imports: &[Import], name: &str) -> Vec<u32> {
    imports
        .iter()
        .filter(|import| import.name.eq_ignore_ascii_case(name))
        .map(|import| import.descriptor_rva)
        .collect()
}

/// The two writes of an `IMAGE_IMPORT_DESCRIPTOR` patch: zeros for `TimeDateStamp` (+4) and `ForwarderChain` (+8), and
/// the new `Name` RVA (+12). Clearing the bound-import fields makes the loader resolve the new name instead of using
/// stale forwarders. `OriginalFirstThunk` (+0) is never written: the loader needs the lookup table it names.
fn descriptor_patches(name_rva: u32) -> ([u8; 8], [u8; 4]) {
    ([0u8; 8], name_rva.to_le_bytes())
}

/// Rewrites every import descriptor of the suspended program whose DLL is in `mapping` so the loader reads each DLL
/// from the mod layer. Each matching descriptor gets its own remote name and descriptor patch. The mapping keys are the
/// import names returned by `imports`; a key without a matching descriptor is ignored.
///
/// # Safety
/// `process` must be the suspended program's process handle, with the rights `CreateProcessW` returns.
pub unsafe fn rewrite(
    process: HANDLE,
    program: &Path,
    mapping: &[(String, PathBuf)],
) -> io::Result<()> {
    if mapping.is_empty() {
        return Ok(());
    }

    let parsed = parse_file(program)?;
    let imports = &parsed.imports;
    let pe32 = parsed.pe32;
    let base = main_module(process, pe32)?;

    for (name, path) in mapping {
        let descriptors = descriptor_rvas(imports, name);
        if descriptors.is_empty() {
            continue;
        }
        let path = ansi(path)?;

        for descriptor_rva in descriptors {
            let Some(remote) = allocate_near(process, base, NEAR_IMAGE_SIZE, path.len() + 1) else {
                return Err(io::Error::other(format!(
                    "could not allocate memory near the image for the import {name}"
                )));
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
                return Err(io::Error::other(format!(
                    "the layer path for the import {name} is too far from the image"
                )));
            };
            let (bound, name_bytes) = descriptor_patches(name_rva);
            let descriptor = base + descriptor_rva as usize;
            let mut old = 0;
            // SAFETY: the descriptor is inside the mapped image; the protection change covers the fields written below
            // and is restored whether either write succeeds.
            if unsafe {
                VirtualProtectEx(
                    process,
                    descriptor as *const c_void,
                    12,
                    PAGE_READWRITE,
                    &mut old,
                )
            } == FALSE
            {
                let error = io::Error::last_os_error();
                free_remote(process, remote);
                return Err(error);
            }
            // SAFETY: the eight bytes at +4 are the descriptor's `TimeDateStamp` and `ForwarderChain` fields; +0 is
            // left alone so `OriginalFirstThunk` still names the lookup table.
            let cleared = unsafe {
                WriteProcessMemory(
                    process,
                    (descriptor + 4) as *const c_void,
                    bound.as_ptr().cast(),
                    bound.len(),
                    &mut written,
                )
            };
            let clear_error = (cleared == FALSE).then(io::Error::last_os_error);
            let bound_written = written;
            // SAFETY: the four bytes at +12 are the descriptor's `Name` field.
            let patched = unsafe {
                WriteProcessMemory(
                    process,
                    (descriptor + 12) as *const c_void,
                    name_bytes.as_ptr().cast(),
                    name_bytes.len(),
                    &mut written,
                )
            };
            let patch_error = (patched == FALSE).then(io::Error::last_os_error);
            let name_written = written;
            // SAFETY: restores the protection the call above returned.
            unsafe {
                VirtualProtectEx(process, descriptor as *const c_void, 12, old, &mut old);
            }

            if let Some(error) = clear_error.or(patch_error) {
                free_remote(process, remote);
                return Err(error);
            }
            if bound_written != bound.len() {
                free_remote(process, remote);
                return Err(io::Error::other(format!(
                    "only {bound_written} of the eight bound-import bytes of the descriptor for {name} were written"
                )));
            }
            if name_written != name_bytes.len() {
                free_remote(process, remote);
                return Err(io::Error::other(format!(
                    "only {name_written} of the four bytes of the import descriptor for {name} were written"
                )));
            }
        }
    }

    Ok(())
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

/// How far above the image to start looking. The import descriptor's `Name` is a 32-bit RVA, so a span of 256 MiB
/// from the image base keeps every address in it nameable while clearing the image's own addresses.
const NEAR_IMAGE_SIZE: u32 = 0x1000_0000;

/// How many 64 KiB steps `allocate_near` tries above `NEAR_IMAGE_SIZE`. A small bound covers the usual gap after an
/// image; if it is taken, the launch fails instead of looping over the address space forever or leaving the import
/// bound to the game folder.
const NEAR_RETRIES: u32 = 32;

/// Allocates `size` bytes in the target close enough to `base` that the offset fits in the 32-bit import name RVA.
fn allocate_near(
    process: HANDLE,
    base: usize,
    image_size: u32,
    size: usize,
) -> Option<*mut c_void> {
    let mut preferred = base.checked_add(image_size as usize)?.checked_add(0xFFFF)? & !0xFFFF;

    for _ in 0..NEAR_RETRIES {
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

// `GetShortPathNameW` is a stable kernel32 export. Declaring it here avoids widening this crate's windows-sys features
// for one fallback; the rest of the Win32 calls come from windows-sys.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetShortPathNameW(long_path: *const u16, short_path: *mut u16, capacity: u32) -> u32;
}

/// The path in the system ANSI code page, with a NUL terminator. A path outside the code page is retried as its short
/// (8.3) name, which often stays in ASCII; the original path is named in the error when neither form fits.
fn ansi(path: &Path) -> io::Result<Vec<u8>> {
    let long = wide(path.as_os_str());
    if let Some(bytes) = ansi_units(&long)? {
        return Ok(bytes);
    }

    let unrepresentable = || {
        io::Error::other(format!(
            "the layer path {} cannot be represented in the system ANSI code page",
            path.display()
        ))
    };
    let Some(short) = short_path(&long) else {
        return Err(unrepresentable());
    };
    match ansi_units(&short)? {
        Some(bytes) => Ok(bytes),
        None => Err(unrepresentable()),
    }
}

/// Converts NUL-terminated UTF-16 into the system ANSI code page. `Ok(None)` means a character had to be substituted,
/// so the loader would look for a different name than the caller gave.
fn ansi_units(units: &[u16]) -> io::Result<Option<Vec<u8>>> {
    // SAFETY: the length is the number of units, and a null output asks for the needed size.
    let needed = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            units.as_ptr(),
            units.len() as i32 - 1,
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
    let mut used_default = 0;
    // SAFETY: `bytes` has room for the converted text and the terminator, and `used_default` is a valid BOOL for the
    // call. A character outside the code page makes the loader look for a different name, so it is reported by `None`.
    let written = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            0,
            units.as_ptr(),
            units.len() as i32 - 1,
            bytes.as_mut_ptr(),
            needed,
            std::ptr::null(),
            &mut used_default,
        )
    };

    if written <= 0 {
        return Err(io::Error::last_os_error());
    }

    if used_default != 0 {
        return Ok(None);
    }

    bytes.truncate(written as usize);
    bytes.push(0);
    Ok(Some(bytes))
}

/// The short (8.3) form of a NUL-terminated path, itself NUL-terminated, or `None` when the volume has none.
fn short_path(long: &[u16]) -> Option<Vec<u16>> {
    // SAFETY: `long` is NUL-terminated; a zero capacity asks for the size of the result, including its terminator.
    let needed = unsafe { GetShortPathNameW(long.as_ptr(), std::ptr::null_mut(), 0) };
    let capacity = usize::try_from(needed).ok()?;
    if capacity == 0 {
        return None;
    }
    let mut short = vec![0u16; capacity];
    // SAFETY: `short` has room for `capacity` units including the terminator, and `long` is NUL-terminated.
    let written = unsafe { GetShortPathNameW(long.as_ptr(), short.as_mut_ptr(), needed) };
    let used = usize::try_from(written).ok()?;
    if used == 0 || used >= capacity {
        return None;
    }
    short.truncate(used + 1);
    Some(short)
}

/// The error of a malformed program image, for the callers that report `io::Error`.
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "the program has a malformed PE header",
    )
}

/// The error of a malformed image read by `gobbo_overlay_pe`, with the parser's reason.
fn malformed(detail: gobbo_overlay_pe::Malformed) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("the program could not be parsed: {detail}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Writes a program-shaped file under the system temp folder and returns its path. The caller removes it.
    fn a_program_file(bytes: &[u8], tail: usize) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "gobbo-static-imports-{}-{stamp}.exe",
            std::process::id()
        ));
        let mut file = File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        file.write_all(&vec![0xAB; tail]).unwrap();
        drop(file);
        path
    }

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

    /// The failure branch is not unit-tested: whether a short name exists depends on the volume, and whether it is in
    /// CP_ACP depends on the machine's code page. Neither can be forced by a portable fixture.
    #[test]
    fn an_ascii_layer_path_is_converted_with_a_terminator() {
        let bytes = ansi(Path::new(r"C:\Games\mods\proxy.dll")).expect("ASCII is representable");
        assert_eq!(bytes, b"C:\\Games\\mods\\proxy.dll\0".to_vec());
    }

    #[test]
    fn a_representable_short_form_is_converted_with_a_terminator() {
        let short = wide(OsStr::new(r"C:\PROGRA~1\proxy.dll"));
        let bytes = ansi_units(&short)
            .expect("the conversion itself succeeds")
            .expect("ASCII is representable");
        assert_eq!(bytes, b"C:\\PROGRA~1\\proxy.dll\0".to_vec());
    }

    /// Three descriptors, two for one DLL in different case, each with stale bound-import fields. The builder lives in
    /// `gobbo-overlay-pe` so its header layout stays in step with the parser's own fixtures.
    fn duplicate_dll_image() -> Vec<u8> {
        gobbo_overlay_pe::test_support::image_with_bound_imports(
            false,
            &["version.dll", "VERSION.DLL", "kernel32.dll"],
            &[(0x1234_5678, 0x9ABC_DEF0); 3],
        )
    }

    #[test]
    fn every_descriptor_of_a_dll_is_selected_case_insensitively() {
        let bytes = duplicate_dll_image();
        let (pe32, imports) = gobbo_overlay_pe::parse(&bytes).unwrap();
        assert!(!pe32);
        assert_eq!(imports.len(), 3);
        assert_eq!(
            descriptor_rvas(&imports, "Version.dll"),
            vec![0x1000, 0x1014]
        );
        assert_eq!(descriptor_rvas(&imports, "kernel32.dll"), vec![0x1028]);
    }

    #[test]
    fn a_descriptor_patch_clears_bound_imports_and_writes_the_name() {
        let (bound, name) = descriptor_patches(0x1234_5678);
        assert_eq!(bound, [0u8; 8]);
        assert_eq!(name, 0x1234_5678u32.to_le_bytes());
        // Only +4..+12 and +12..+16 are written; `OriginalFirstThunk` at +0 is not part of either patch.
    }

    #[test]
    fn the_test_program_is_not_reported_as_pe32_when_the_build_is_x64() {
        let program = std::env::current_exe().unwrap();
        assert_eq!(
            parse_file(&program).unwrap().pe32,
            cfg!(target_pointer_width = "32")
        );
    }

    /// The bounded reader must give the same bitness and imports as parsing the whole file: a prefix may never turn
    /// a real import list into a shorter one.
    #[test]
    fn the_bounded_reader_parses_the_test_program_like_the_whole_file() {
        let program = std::env::current_exe().unwrap();
        let whole = std::fs::read(&program).unwrap();
        let expected = parsed(gobbo_overlay_pe::parse(&whole).unwrap()).unwrap();
        assert_eq!(parse_file(&program).unwrap(), expected);
    }

    /// The reader's bound is the only limit on disk I/O here. An unusually large tail after the program is parsed
    /// from the prefix alone and does not change the result.
    #[test]
    fn a_large_tail_after_the_program_is_not_read_into_the_parser() {
        let program = std::env::current_exe().unwrap();
        let original = std::fs::read(&program).unwrap();
        let expected = gobbo_overlay_pe::parse(&original).unwrap();
        let path = a_program_file(&original, PREFIX_BYTES + 64 * 1024);
        let bytes = std::fs::read(&path).unwrap();
        // The parser decided from the first `PREFIX_BYTES`: no byte of the tail was needed.
        assert_eq!(
            gobbo_overlay_pe::parse_prefix(&bytes[..PREFIX_BYTES]),
            gobbo_overlay_pe::PrefixParse::Complete(Ok(expected.clone()))
        );
        assert_eq!(parse_file(&path).unwrap(), parsed(expected).unwrap());
        std::fs::remove_file(&path).unwrap();
    }

    /// A prefix that ends in a header or inside an import descriptor must fall back to the whole file, not report an
    /// empty import list.
    #[test]
    fn a_truncated_prefix_falls_back_to_the_whole_file() {
        let program = std::env::current_exe().unwrap();
        let whole = std::fs::read(&program).unwrap();
        let expected = parsed(gobbo_overlay_pe::parse(&whole).unwrap()).unwrap();
        // No bytes at all, and a prefix cut in the PE headers, both force the fallback.
        assert_eq!(parse_file_limited(&program, 0).unwrap(), expected);
        assert_eq!(parse_file_limited(&program, 0x90).unwrap(), expected);

        // A prefix cut inside the first import descriptor of a synthetic image does the same.
        let bytes = duplicate_dll_image();
        let path = a_program_file(&bytes, 0);
        let found = parse_file_limited(&path, 0x205).unwrap();
        assert_eq!(found.imports.len(), 3);
        assert_eq!(
            descriptor_rvas(&found.imports, "Version.dll"),
            vec![0x1000, 0x1014]
        );
        assert_eq!(
            descriptor_rvas(&found.imports, "kernel32.dll"),
            vec![0x1028]
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_malformed_program_is_rejected_by_the_bounded_reader() {
        let path = a_program_file(b"not a PE image", 0);
        let error = parse_file(&path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        std::fs::remove_file(&path).unwrap();
    }
}
