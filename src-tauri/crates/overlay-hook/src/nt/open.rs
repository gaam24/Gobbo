//! `NtCreateFile` and `NtOpenFile`: the same redirection as `CreateFileW`, for the object name the NT layer receives. The
//! loader opens DLLs through `NtOpenFile`.

use std::ffi::c_void;

use gobbo_overlay_decisions::{Kind, Layer};
use windows_sys::Win32::Foundation::{STATUS_OBJECT_NAME_COLLISION, STATUS_UNSUCCESSFUL};

use super::layout::{
    FILE_CREATE, FILE_DELETE_ON_CLOSE, FILE_OPEN, FILE_OVERWRITE, FILE_OVERWRITE_IF, FILE_SUPERSEDE,
};
use super::names::{
    call_redirected, object_name, place_name, redirect_nt, refuse_device_write, shown_object,
};
use super::trace_nt;
use crate::diag::record_decision;
use crate::guard::{Guard, enter_session};
use crate::paths::{in_lower, is_visible_dir, writes_requested};
use crate::search::has_visible_entries;
use crate::trace::trace_status;
use crate::types::ObjectAttributes;
use crate::whiteouts::hide;
use crate::win32::PreserveLastError;

/// `NtCreateFile`: the same redirection as `CreateFileW`, for the object name the NT layer receives.
///
/// # Safety
/// Called by the program with the arguments of `NtCreateFile`.
pub(crate) unsafe extern "system" fn nt_create_file(
    handle: *mut *mut c_void,
    access: u32,
    attributes: *const ObjectAttributes,
    status: *mut c_void,
    allocation: *const i64,
    file_attributes: u32,
    share: u32,
    disposition: u32,
    options: u32,
    ea: *const c_void,
    ea_length: u32,
) -> i32 {
    unsafe { trace_nt("NtCreateFile", attributes) };
    // As `CreateFileW`: keep the guard for the whole call, so the hook does not see its own redirected name again.
    let guard = Guard::enter();
    let writing = writes_requested(access) || (disposition != FILE_OPEN);
    // The device check stays ahead of classification: the name here is still the one the program passed.
    if let Some(status) = unsafe { refuse_device_write(attributes, writing, "NtCreateFile") } {
        return status;
    }
    // A new file over a name the layers show fails before anything is copied.
    let taken =
        guard.is_some() && disposition == FILE_CREATE && unsafe { shown_object(attributes) };
    if taken {
        record_decision(
            Kind::Refuse,
            Layer::None,
            STATUS_OBJECT_NAME_COLLISION as u32,
            || unsafe { object_name(attributes) },
        );
        return STATUS_OBJECT_NAME_COLLISION;
    }

    let keeps_contents = !matches!(
        disposition,
        FILE_SUPERSEDE | FILE_OVERWRITE | FILE_OVERWRITE_IF
    );
    let redirected = unsafe {
        redirect_open(
            "NtCreateFile",
            attributes,
            access,
            writing,
            keeps_contents,
            Some(disposition),
            &guard,
        )
    };
    let original = original!(nt_create_file, STATUS_UNSUCCESSFUL);
    let result = unsafe {
        call_redirected(attributes, redirected, |attributes| {
            original.call(
                handle,
                access,
                attributes,
                status,
                allocation,
                file_attributes,
                share,
                disposition,
                options,
                ea,
                ea_length,
            )
        })
    };
    let _last_error = PreserveLastError::capture();
    trace_status("NtCreateFile result", result);
    drop(guard);

    unsafe { after_open(attributes, result, options) };
    result
}

/// `NtOpenFile`: the same redirection, for opening an existing object. The loader opens DLLs this way.
///
/// # Safety
/// Called by the program with the arguments of `NtOpenFile`.
pub(crate) unsafe extern "system" fn nt_open_file(
    handle: *mut *mut c_void,
    access: u32,
    attributes: *const ObjectAttributes,
    status: *mut c_void,
    share: u32,
    options: u32,
) -> i32 {
    unsafe { trace_nt("NtOpenFile", attributes) };
    let guard = Guard::enter();
    let writing = writes_requested(access);
    if let Some(status) = unsafe { refuse_device_write(attributes, writing, "NtOpenFile") } {
        return status;
    }
    let redirected = unsafe {
        redirect_open(
            "NtOpenFile",
            attributes,
            access,
            writing,
            true,
            None,
            &guard,
        )
    };
    let original = original!(nt_open_file, STATUS_UNSUCCESSFUL);
    let result = unsafe {
        call_redirected(attributes, redirected, |attributes| {
            original.call(handle, access, attributes, status, share, options)
        })
    };
    let _last_error = PreserveLastError::capture();
    trace_status("NtOpenFile result", result);
    drop(guard);

    unsafe { after_open(attributes, result, options) };
    result
}

/// Redirects an open or create, records a decision for refusals after classification, and logs a pass-through write. A
/// device-namespace write is refused by the caller before this runs, so every error recorded here was classified.
///
/// # Safety
/// `attributes` must be the `OBJECT_ATTRIBUTES` the program passed, and `guard` the guard the caller entered.
unsafe fn redirect_open(
    operation: &'static str,
    attributes: *const ObjectAttributes,
    access: u32,
    writing: bool,
    keeps_contents: bool,
    disposition: Option<u32>,
    guard: &Option<Guard>,
) -> Option<Result<crate::paths::Redirected, crate::paths::Failure>> {
    let redirected = match guard.as_ref() {
        Some(_guard) => unsafe { redirect_nt(attributes, writing, keeps_contents) },
        None => None,
    };

    if let Some(Err(failure)) = &redirected {
        record_decision(
            Kind::Refuse,
            Layer::None,
            failure.status() as u32,
            || unsafe { object_name(attributes) },
        );
    }

    if guard.is_some() && redirected.is_none() && writing && crate::diag::settings().pass_writes {
        let name = unsafe { object_name(attributes) };
        let extra = disposition.map_or_else(String::new, |d| format!(" disposition={d}"));
        crate::trace::log(&format!(
            "pass-through {operation} {name} access=0x{access:x}{extra}"
        ));
        record_decision(Kind::PassThrough, Layer::None, 0, || name);
    }

    redirected
}

/// After an open that succeeded: a file opened with `FILE_DELETE_ON_CLOSE` is deleted when its handle closes, which is how
/// `del` deletes a file. Its lower entry is hidden now, so the file does not come back at that close.
///
/// # Safety
/// `attributes` must be the valid `OBJECT_ATTRIBUTES` the open was given.
unsafe fn after_open(attributes: *const ObjectAttributes, status: i32, options: u32) {
    if status < 0 || options & FILE_DELETE_ON_CLOSE == 0 {
        return;
    }

    let Some(_guard) = enter_session() else {
        return;
    };
    let Some(place) = (unsafe { place_name(attributes) }) else {
        return;
    };
    // A folder with visible entries is not deleted, so its lower entries stay visible.
    if is_visible_dir(place.mount, &place.rel) && has_visible_entries(place.mount, &place.rel) {
        return;
    }
    // A lower copy that cannot be queried is still a copy to hide: the file is about to be deleted from the upper layer.
    if in_lower(place.mount, &place.rel).unwrap_or(true) {
        hide(place.mount, &place.rel);
    }
}
