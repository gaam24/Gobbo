//! Hands the loader's stubs back to the hook. Before the loader ran, the launcher patched the start of four ntdll stubs with
//! a jump into the stage 0 DLL, so the loader's own file calls already saw the mod layer. The hook installs its detours on
//! the same functions next, and a stub that still jumps into stage 0 would chain into it. The launcher wrote the original
//! bytes, and the address of the stage 0 configuration, into a handover file named by `GOBBO_OVERLAY_EARLY`. This module
//! writes the bytes back, switches the stage 0 redirect off and consumes the variable: a successful restore removes
//! `GOBBO_OVERLAY_EARLY` from the process environment, so a child started later inherits no stage-0 handover and sees
//! the hook already installed. Without the variable there was no stage 0 and the restore is a success.
//!
//! A partial handover is not a success: a stub left patched would let the loader's file calls bypass every decision the
//! hook makes, so the caller must not install detours on top of it, and the variable stays set to keep the diagnosis.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::trace::log;
use gobbo_overlay_core::early::{ENV, Line, parse_line};

/// Restores the stubs and switches the stage 0 redirect off, then removes the handover file and consumes the variable.
/// Called before the hook creates its own detours.
///
/// Returns `Err` with the reason when the handover was named but could not be trusted: a missing or unreadable file, an
/// invalid line, a stub or the config flag that could not be written, or fewer stubs restored than the file described.
/// An unset variable means there was no stage 0, which is a success. Only a success clears the variable: children must
/// not inherit a handover that no longer exists, while a failed restore keeps it for the diagnosis.
pub(crate) fn restore() -> Result<(), String> {
    let Some(path) = std::env::var_os(ENV) else {
        return Ok(());
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("the handover file could not be read: {error}"))?;
    let mut expected = 0usize;
    let mut restored = 0usize;
    let mut config = false;
    let mut problem: Option<String> = None;

    for line in text.lines() {
        match parse_line(line) {
            Some(Line::Config(address)) => {
                config = write(address, &0u32.to_le_bytes());
                if !config {
                    problem.get_or_insert_with(|| "the config flag could not be cleared".into());
                }
            }
            Some(Line::Stub { address, original }) => {
                expected += 1;
                if write(address, &original) {
                    restored += 1;
                } else {
                    problem.get_or_insert_with(|| {
                        format!("the stub at {address:#x} could not be restored")
                    });
                }
            }
            None if line.trim().is_empty() => {}
            None => {
                problem.get_or_insert_with(|| "a handover line is not valid".into());
            }
        }
    }

    log(&format!(
        "early redirect: {restored} of {expected} stubs handed back, config cleared={config}"
    ));
    if std::fs::remove_file(&path).is_err() {
        log("early redirect: the handover file could not be removed");
    }

    if let Some(problem) = problem {
        return Err(problem);
    }
    if !config {
        return Err("the config flag was not cleared".into());
    }
    if expected != restored {
        return Err(format!("only {restored} of {expected} stubs were restored"));
    }

    // SAFETY: `restore` runs from `GobboOverlayInit` on the launcher's stub, before the detours are enabled and before
    // the crash filter or trace threads start, so no other thread reads or writes the process environment here.
    unsafe { std::env::remove_var(ENV) };
    Ok(())
}

/// Writes bytes into this process's memory at `address`, making the page writable for the write and restoring its
/// protection after, then flushing the instruction cache for the range. Returns whether every step succeeded.
fn write(address: usize, bytes: &[u8]) -> bool {
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
        return false;
    }
    // SAFETY: the destination is the range just made writable, of the same length as `bytes`.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len()) };
    let mut restored = 0;
    // SAFETY: restores the protection `VirtualProtect` returned above.
    let restored_ok =
        unsafe { VirtualProtect(address as *const c_void, bytes.len(), old, &mut restored) };
    if restored_ok == FALSE {
        log("early redirect: a page's protection could not be restored");
        return false;
    }
    // SAFETY: flushes the instruction cache of this process for the range just written.
    let flushed = unsafe {
        FlushInstructionCache(GetCurrentProcess(), address as *const c_void, bytes.len())
    };
    if flushed == FALSE {
        log("early redirect: the instruction cache could not be flushed");
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests set the same environment variable, so they must not run at the same time.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The handover names this process's memory: a stub-sized buffer that holds the patch, and the configuration's flag.
    #[test]
    fn restore_writes_the_stub_bytes_back_and_removes_the_handover() {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        // SAFETY: the variable is set for this test only, and `ENV_LOCK` keeps the other tests off it.
        unsafe { std::env::set_var(ENV, &path) };
        assert!(restore().is_ok());
        assert!(
            std::env::var_os(ENV).is_none(),
            "a successful restore must consume the handover variable so children do not inherit it"
        );

        assert_eq!(&stub[..4], &[0x4C, 0x8B, 0xD1, 0xB8]);
        assert_eq!(config, [0, 0, 0, 0]);
        assert!(!path.exists(), "the handover file was not removed");
    }

    /// A named handover that cannot be fully trusted must be reported, not silently treated as a success.
    #[test]
    fn restore_fails_for_a_missing_file_an_invalid_line_and_a_stub_that_cannot_be_written() {
        let _lock = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let missing =
            std::env::temp_dir().join(format!("gobbo-early-missing-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        // SAFETY: the variable is set for this test only, and `ENV_LOCK` keeps the other tests off it.
        unsafe { std::env::set_var(ENV, &missing) };
        let error = restore();
        assert!(
            std::env::var_os(ENV).is_some(),
            "a failed restore must keep the variable for diagnosis"
        );
        unsafe { std::env::remove_var(ENV) };
        assert!(error.is_err(), "a missing handover must fail: {error:?}");

        let invalid =
            std::env::temp_dir().join(format!("gobbo-early-invalid-{}.txt", std::process::id()));
        std::fs::write(&invalid, "not a handover line\n").unwrap();
        unsafe { std::env::set_var(ENV, &invalid) };
        let error = restore();
        assert!(
            std::env::var_os(ENV).is_some(),
            "a failed restore must keep the variable for diagnosis"
        );
        unsafe { std::env::remove_var(ENV) };
        let _ = std::fs::remove_file(&invalid);
        assert!(error.is_err(), "an invalid line must fail: {error:?}");

        let mut config = [1u8, 0, 0, 0];
        let text = format!("config {}\nstub 1 0000\n", config.as_mut_ptr() as usize);
        std::fs::write(&invalid, text).unwrap();
        unsafe { std::env::set_var(ENV, &invalid) };
        let error = restore();
        assert!(
            std::env::var_os(ENV).is_some(),
            "a failed restore must keep the variable for diagnosis"
        );
        unsafe { std::env::remove_var(ENV) };
        let _ = std::fs::remove_file(&invalid);
        assert!(
            error.is_err(),
            "a stub that cannot be written must fail: {error:?}"
        );
    }
}
