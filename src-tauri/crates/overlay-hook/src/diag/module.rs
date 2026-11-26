//! Where an address is: the module that holds it, and how far into that module it is. The trace uses it to show which code
//! made a call, and the crash report to show where the program stopped.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HMODULE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleHandleExW,
};

use crate::detours::exports::module_path;

/// The base address of this DLL. The frames of the hook are left out when a call is traced back to its caller.
static OWN_BASE: OnceLock<usize> = OnceLock::new();
/// The modules loaded when the hook was installed, sorted by base address. The crash path must not ask the loader.
static MODULES: OnceLock<Vec<Module>> = OnceLock::new();
/// How many frames of the stack are read to find the caller of a call.
const FRAMES: usize = 32;

/// One loaded module, as the snapshot at install saw it.
struct Module {
    base: usize,
    size: usize,
    name: String,
}

/// Keeps the base address of this DLL, from `DllMain`.
pub(crate) fn remember_own(module: *mut c_void) {
    let _ = OWN_BASE.set(module as usize);
}

/// The base address of this DLL, when `DllMain` has stored it. The init export uses it to remember the DLL path for
/// child injection without doing loader work in `DllMain`.
pub(crate) fn own_module() -> Option<*mut c_void> {
    OWN_BASE.get().map(|base| *base as *mut c_void)
}

/// Reads the loaded modules once, at install, when a diagnostic needs them: per-call tracing, the first-chance crash
/// report, an in-process dump or the out-of-process dumper. A module loaded later is not in the cache; the trace falls
/// back to the loader for it, while the crash path reports the address alone (see `locate_in`).
pub(crate) fn remember_modules() {
    if MODULES.get().is_some() {
        return;
    }
    // SAFETY: the snapshot takes the process id and a flags mask; the handle is closed below.
    let snapshot = unsafe {
        CreateToolhelp32Snapshot(
            TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32,
            windows_sys::Win32::System::Threading::GetCurrentProcessId(),
        )
    };

    if snapshot == INVALID_HANDLE_VALUE {
        return;
    }

    let mut entry: MODULEENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
    let mut modules = Vec::new();
    // SAFETY: `entry` has the size the calls expect, and the snapshot is open.
    if unsafe { Module32FirstW(snapshot, &mut entry) } != 0 {
        loop {
            let name = entry
                .szModule
                .iter()
                .position(|unit| *unit == 0)
                .map_or_else(
                    || String::from_utf16_lossy(&entry.szModule),
                    |length| String::from_utf16_lossy(&entry.szModule[..length]),
                );
            modules.push(Module {
                base: entry.modBaseAddr as usize,
                size: entry.modBaseSize as usize,
                name,
            });
            // SAFETY: the snapshot is still open and `entry` is the same structure.
            if unsafe { Module32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }
    // SAFETY: the snapshot came from the call above and is closed once.
    unsafe { CloseHandle(snapshot) };
    modules.sort_by_key(|module| module.base);
    let _ = MODULES.set(modules);
}

/// The module that holds `address` from the install snapshot, if it was loaded then.
fn cached_at(address: usize) -> Option<(usize, String)> {
    let modules = MODULES.get()?;
    let at = modules.partition_point(|module| module.base <= address);
    let module = modules.get(at.checked_sub(1)?)?;
    (address < module.base.saturating_add(module.size)).then(|| (module.base, module.name.clone()))
}

/// The module that holds `address`: its base address, and its file name without the folder. The install snapshot is
/// used first; a module loaded after it is asked from the loader (trace only, never the crash path).
fn module_at(address: usize) -> Option<(usize, String)> {
    cached_at(address).or_else(|| module_at_loader(address))
}

/// The module that holds `address`, asked from the loader.
fn module_at_loader(address: usize) -> Option<(usize, String)> {
    let mut module: HMODULE = std::ptr::null_mut();
    let flags =
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    // SAFETY: Windows only compares the address with the ranges of the loaded modules. Nothing is read at it.
    let found = unsafe { GetModuleHandleExW(flags, address as *const u16, &mut module) };

    if found == 0 || module.is_null() {
        return None;
    }

    let path = module_path(module)?;
    let name = path.file_name().map_or_else(
        || "<unknown>".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Some((module as usize, name))
}

/// The place of `address` as text: the module and the offset into it, or the address when no module holds it.
pub(crate) fn locate(address: usize) -> String {
    locate_in(module_at(address), address)
}

/// The place of `address` from the install snapshot only. The crash handler uses it: it must not ask the loader.
pub(crate) fn locate_cached(address: usize) -> String {
    locate_in(cached_at(address), address)
}

/// The place of `address` as text from a module snapshot: the module and the offset, or the address.
fn locate_in(module: Option<(usize, String)>, address: usize) -> String {
    match module {
        Some((base, name)) => offset_text(&name, address.saturating_sub(base)),
        None => format!("0x{address:x}"),
    }
}

/// `module+0xoffset`.
fn offset_text(name: &str, offset: usize) -> String {
    format!("{name}+0x{offset:x}")
}

/// The place in the program that made the call being traced: the first frame of the stack that is not in this DLL. The
/// unwinder lists the frames, so a function that the compiler inlined into the hook is not listed on its own.
pub(crate) fn caller() -> String {
    let mut frames = [std::ptr::null_mut::<c_void>(); FRAMES];
    // SAFETY: the array has `FRAMES` slots, and the call writes no more than that.
    let captured = unsafe {
        RtlCaptureStackBackTrace(0, FRAMES as u32, frames.as_mut_ptr(), std::ptr::null_mut())
    } as usize;
    let own = OWN_BASE.get().copied();

    for frame in &frames[..captured.min(FRAMES)] {
        let address = *frame as usize;

        match module_at(address) {
            Some((base, _)) if Some(base) == own => {}
            _ => return locate(address),
        }
    }

    "unknown place".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_is_written_in_hex_after_the_module_name() {
        assert_eq!(offset_text("game.exe", 0x1f40), "game.exe+0x1f40");
    }

    /// The crash report uses `locate_cached` even when no diagnostic needed the table at install. With no module to
    /// name it falls back to the address, without asking the loader from the crash path.
    #[test]
    fn an_address_without_a_cached_module_is_shown_on_its_own() {
        assert_eq!(locate_in(None, 0x1f40), "0x1f40");
    }

    #[test]
    fn the_install_snapshot_finds_an_address_in_this_test_binary_without_the_loader() {
        remember_modules();
        let place = locate_cached(remember_modules as *const () as usize);
        assert!(place.contains("+0x"), "{place}");
    }
}
