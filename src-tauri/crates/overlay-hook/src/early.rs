//! Hands the loader's stubs back to the hook. Before the loader ran, the launcher patched the start of four ntdll stubs with
//! a jump into the stage 0 DLL, so the loader's own file calls already saw the mod layer. The hook installs its detours on
//! the same functions next, and a stub that still jumps into stage 0 would chain into it. The launcher wrote the original
//! bytes, and the address of the stage 0 configuration, into a handover file named by `GOBBO_OVERLAY_EARLY`. This module
//! writes the bytes back and switches the stage 0 redirect off. Without the variable, or without the file, it does nothing.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::trace::log;
use gobbo_overlay_core::early::{ENV, Line, parse_line};

/// Restores the stubs and switches the stage 0 redirect off, then removes the handover file. Called before the hook
/// creates its own detours.
pub(crate) fn restore() {
    let Some(path) = std::env::var_os(ENV) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let mut stubs = 0;

    for line in text.lines() {
        match parse_line(line) {
            Some(Line::Config(address)) => write(address, &0u32.to_le_bytes()),
            Some(Line::Stub { address, original }) => {
                write(address, &original);
                stubs += 1;
            }
            None if line.trim().is_empty() => {}
            None => log("early redirect: a line is not valid"),
        }
    }

    log(&format!(
        "early redirect: {stubs} stubs handed back to the hook"
    ));
    let _ = std::fs::remove_file(&path);
}

/// Writes bytes into this process's memory at `address`, making the page writable for the write and restoring its
/// protection after, then flushing the instruction cache for the range.
fn write(address: usize, bytes: &[u8]) {
    let mut old = 0;
    // SAFETY: the range is an ntdll stub or the launcher's configuration in this process, read here only as the handover
    // names it; the old protection is restored below.
    let protected = unsafe {
        VirtualProtect(
            address as *const c_void,
            bytes.len(),
            PAGE_EXECUTE_READWRITE,
            &mut old,
        )
    };

    if protected == FALSE {
        log("early redirect: a page could not be made writable");
        return;
    }
    // SAFETY: the destination is the range just made writable, of the same length as `bytes`.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len()) };
    let mut restored = 0;
    // SAFETY: restores the protection `VirtualProtect` returned above.
    let restored_ok =
        unsafe { VirtualProtect(address as *const c_void, bytes.len(), old, &mut restored) };
    if restored_ok == FALSE {
        log("early redirect: a page's protection could not be restored");
    }
    // SAFETY: flushes the instruction cache of this process for the range just written.
    let flushed = unsafe {
        FlushInstructionCache(GetCurrentProcess(), address as *const c_void, bytes.len())
    };
    if flushed == FALSE {
        log("early redirect: the instruction cache could not be flushed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The handover names this process's memory: a stub-sized buffer that holds the patch, and the configuration's flag.
    #[test]
    fn restore_writes_the_stub_bytes_back_and_removes_the_handover() {
        let mut stub = [0xE9u8; 22];
        let mut config = [1u8, 0, 0, 0];
        let path = std::env::temp_dir().join(format!("gobbo-early-{}.txt", std::process::id()));
        let hex: String = [0x4Cu8, 0x8B, 0xD1, 0xB8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let text = format!(
            "config {}\nstub {} {}\n",
            config.as_mut_ptr() as usize,
            stub.as_mut_ptr() as usize,
            hex
        );
        std::fs::write(&path, text).unwrap();
        // SAFETY: the variable is set for this test only; the test binary runs its tests on their own threads, and no
        // other test reads this variable.
        unsafe { std::env::set_var(ENV, &path) };
        restore();

        unsafe { std::env::remove_var(ENV) };
        assert_eq!(&stub[..4], &[0x4C, 0x8B, 0xD1, 0xB8]);
        assert_eq!(config, [0, 0, 0, 0]);
        assert!(!path.exists(), "the handover file was not removed");
    }
}
