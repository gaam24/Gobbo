//! `CreateFileW`: a read opens the top-most layer's file, and a write opens the upper layer's copy.

use std::ffi::c_void;

use gobbo_overlay_decisions::{Kind, Layer};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_EXISTS, ERROR_INVALID_FUNCTION, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CREATE_NEW, OPEN_EXISTING, TRUNCATE_EXISTING,
};

use super::{PreserveLastError, fail};
use crate::diag::{self, record_decision};
use crate::guard::Guard;
use crate::paths::{Redirected, redirect_opened, shown_name, unsafe_name, writes_requested};
use crate::text::text;
use crate::trace::{trace_call, trace_result};

/// `CreateFileW`: a read opens the top-most layer's file, and a write opens the upper layer's copy.
///
/// # Safety
/// Called by the program with the arguments of `CreateFileW`.
pub(crate) unsafe extern "system" fn create_file(
    name: *const u16,
    access: u32,
    share: u32,
    security: *const c_void,
    disposition: u32,
    flags: u32,
    template: *mut c_void,
) -> *mut c_void {
    unsafe { trace_call("CreateFileW", name) };
    // The guard stays for the original call too: `CreateFileW` reaches `NtCreateFile`, which the hook also covers. A
    // second redirection of the already redirected name is both wrong and expensive.
    let guard = Guard::enter();
    // A new file over a name the layers show fails. Nothing is copied first: a copy would stay in the upper layer and
    // hide every later change to the mod layer for that file.
    let taken = guard.is_some() && disposition == CREATE_NEW && unsafe { shown_name(name) };

    if taken {
        record_decision(Kind::Refuse, Layer::None, ERROR_FILE_EXISTS, || unsafe {
            crate::text::display_name(name)
        });
        fail(ERROR_FILE_EXISTS);
        return INVALID_HANDLE_VALUE;
    }

    let writing = writes_requested(access) || disposition != OPEN_EXISTING;
    // A device namespace can alias a file in the game folder; a write through it must not pass through unchanged. The
    // name is still unclassified here, so this refusal is traced but builds no ring path.
    if writing && unsafe { unsafe_name(name) } {
        crate::trace::trace(|| {
            format!("CreateFileW refused a device namespace: {}", unsafe {
                crate::text::display_name(name)
            })
        });
        fail(ERROR_ACCESS_DENIED);
        return INVALID_HANDLE_VALUE;
    }

    let keeps_contents = disposition != CREATE_ALWAYS && disposition != TRUNCATE_EXISTING;
    let redirected = match guard.as_ref() {
        Some(_guard) => unsafe { redirect_opened(name, writing, keeps_contents) },
        None => None,
    };
    let (redirected, outside) = match redirected {
        Some(Ok(Redirected::Layer { name: wide })) => (Some(wide), false),
        Some(Ok(Redirected::Same)) => (None, false),
        Some(Err(failure)) => {
            // The failure is logged as well, so a crash that follows a missing file shows the value the program got.
            let error = failure.win32();
            record_decision(Kind::Refuse, Layer::None, error, || unsafe {
                crate::text::display_name(name)
            });
            fail(error);
            trace_result("CreateFileW", false, error);
            return INVALID_HANDLE_VALUE;
        }
        None => (None, true),
    };
    let path = redirected.as_ref().map_or(name, |wide| wide.as_ptr());

    if redirected.is_some() {
        // The game-relative path is classified a second time, and only when the call report is on: it is not worth an
        // allocation on every redirected open of a normal session.
        diag::count("REDIRECT", || {
            let rel =
                unsafe { crate::paths::place(name) }.map_or_else(String::new, |place| place.rel);
            format!(
                "{} -> layer {rel:?} access={access:#x} disposition={disposition}",
                unsafe { text(name) }.unwrap_or_default()
            )
        });
    }

    if outside && writing && guard.is_some() && crate::diag::settings().pass_writes {
        unsafe {
            crate::trace::pass_through("CreateFileW", name, || {
                format!("access={access:#x} disposition={disposition}")
            });
        }

        record_decision(Kind::PassThrough, Layer::None, 0, || unsafe {
            crate::text::display_name(name)
        });
    }

    let original = original!(create_file, {
        fail(ERROR_INVALID_FUNCTION);
        INVALID_HANDLE_VALUE
    });
    let handle =
        unsafe { original.call(path, access, share, security, disposition, flags, template) };
    let last_error = PreserveLastError::capture();
    trace_result(
        "CreateFileW",
        handle != INVALID_HANDLE_VALUE,
        last_error.value(),
    );
    handle
}
