//! `NtQueryAttributesFile` and `NtQueryFullAttributesFile`: the attributes of the file the program would open. The loader
//! checks DLLs and their existence this way.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::STATUS_UNSUCCESSFUL;

use super::names::{call_redirected, redirect_nt};
use super::trace_nt;
use crate::guard::Guard;
use crate::trace::trace_status;
use crate::types::ObjectAttributes;
use crate::win32::PreserveLastError;

/// `NtQueryAttributesFile`: the attributes of the file the program would open. The loader checks DLLs this way.
///
/// # Safety
/// Called by the program with the arguments of `NtQueryAttributesFile`.
pub(crate) unsafe extern "system" fn nt_query_attributes_file(
    attributes: *const ObjectAttributes,
    info: *mut c_void,
) -> i32 {
    unsafe {
        query(
            "NtQueryAttributesFile",
            attributes,
            info,
            |attributes, info| {
                original!(nt_query_attributes_file, STATUS_UNSUCCESSFUL).call(attributes, info)
            },
        )
    }
}

/// `NtQueryFullAttributesFile`: the same as `NtQueryAttributesFile`, with more details. Loader existence checks use it.
///
/// # Safety
/// Called by the program with the arguments of `NtQueryFullAttributesFile`.
pub(crate) unsafe extern "system" fn nt_query_full_attributes_file(
    attributes: *const ObjectAttributes,
    info: *mut c_void,
) -> i32 {
    unsafe {
        query(
            "NtQueryFullAttributesFile",
            attributes,
            info,
            |attributes, info| {
                original!(nt_query_full_attributes_file, STATUS_UNSUCCESSFUL).call(attributes, info)
            },
        )
    }
}

/// The body both query calls share: the name is traced, a read is redirected to the layer that holds the file, and the
/// status is traced. `call` makes the original query with the attributes and the information it gets.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`, and `info` must be the program's information buffer.
unsafe fn query(
    operation: &str,
    attributes: *const ObjectAttributes,
    info: *mut c_void,
    call: impl FnOnce(*const ObjectAttributes, *mut c_void) -> i32,
) -> i32 {
    unsafe { trace_nt(operation, attributes) };
    let guard = Guard::enter();
    // An attribute query only reads, so a device name is never refused here, exactly as before the refactor.
    let redirected = match guard.as_ref() {
        Some(_guard) => unsafe { redirect_nt(attributes, false, true) },
        None => None,
    };
    let status =
        unsafe { call_redirected(attributes, redirected, |attributes| call(attributes, info)) };
    let _last_error = PreserveLastError::capture();
    trace_status(operation, status);
    status
}
