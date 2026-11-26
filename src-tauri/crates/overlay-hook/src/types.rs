//! The Win32 and NT signatures the hook detours, and the structures they take. Each function type is the one of the
//! export it replaces, so a detour takes the same arguments as the original.

use std::ffi::c_void;

use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, WIN32_FIND_DATAW};

pub type GetFileInformationByHandleFn =
    unsafe extern "system" fn(*mut c_void, *mut BY_HANDLE_FILE_INFORMATION) -> i32;
pub type NtQueryInformationFileFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void, u32, u32) -> i32;

pub type CreateFileWFn = unsafe extern "system" fn(
    *const u16,
    u32,
    u32,
    *const c_void,
    u32,
    u32,
    *mut c_void,
) -> *mut c_void;
pub type DeleteFileWFn = unsafe extern "system" fn(*const u16) -> i32;
pub type RemoveDirectoryWFn = unsafe extern "system" fn(*const u16) -> i32;
pub type MoveFileExWFn = unsafe extern "system" fn(*const u16, *const u16, u32) -> i32;
pub type MoveFileWithProgressWFn =
    unsafe extern "system" fn(*const u16, *const u16, *const c_void, *const c_void, u32) -> i32;
pub type FindFirstFileWFn =
    unsafe extern "system" fn(*const u16, *mut WIN32_FIND_DATAW) -> *mut c_void;
pub type FindFirstFileExWFn =
    unsafe extern "system" fn(*const u16, i32, *mut c_void, i32, *const c_void, u32) -> *mut c_void;
pub type FindNextFileWFn = unsafe extern "system" fn(*mut c_void, *mut WIN32_FIND_DATAW) -> i32;
pub type FindCloseFn = unsafe extern "system" fn(*mut c_void) -> i32;
pub type GetFinalPathNameByHandleWFn =
    unsafe extern "system" fn(*mut c_void, *mut u16, u32, u32) -> u32;
/// `GetMappedFileNameW`: the NT name of a mapped file, mapped back from a layer.
pub type GetMappedFileNameWFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u16, u32) -> u32;

/// `CreateProcessInternalW`: every process start goes through it, `CreateProcessW` and `CreateProcessAsUserW` included.
/// The argument list is the one Windows 10 and 11 use: the token, then the same arguments as `CreateProcessW`, then
/// the new token.
pub type CreateProcessWFn = unsafe extern "system" fn(
    *mut c_void,
    *const u16,
    *mut u16,
    *const c_void,
    *const c_void,
    i32,
    u32,
    *const c_void,
    *const u16,
    *const c_void,
    *mut c_void,
    *mut c_void,
) -> i32;
/// `CreateProcessW`: the fallback when `CreateProcessInternalW` is not exported.
pub type CreateProcessPlainFn = unsafe extern "system" fn(
    *const u16,
    *mut u16,
    *const c_void,
    *const c_void,
    i32,
    u32,
    *const c_void,
    *const u16,
    *const c_void,
    *mut c_void,
) -> i32;
/// `CreateProcessAsUserW`: the token comes first, then the arguments of `CreateProcessW`.
pub type CreateProcessAsUserWFn = unsafe extern "system" fn(
    *mut c_void,
    *const u16,
    *mut u16,
    *const c_void,
    *const c_void,
    i32,
    u32,
    *const c_void,
    *const u16,
    *const c_void,
    *mut c_void,
) -> i32;
/// `GetModuleFileNameW`: the full path of a loaded module, mapped back from a layer.
pub type GetModuleFileNameWFn = unsafe extern "system" fn(*mut c_void, *mut u16, u32) -> u32;
/// `GetModuleFileNameA`: the ANSI form of the same path.
pub type GetModuleFileNameAFn = unsafe extern "system" fn(*mut c_void, *mut u8, u32) -> u32;
/// `GetModuleFileNameExW`: the same path for a module of a process the caller names.
pub type GetModuleFileNameExWFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u16, u32) -> u32;
/// `GetModuleFileNameExA`: the ANSI form of `GetModuleFileNameExW`.
pub type GetModuleFileNameExAFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u8, u32) -> u32;

pub type NtCreateFileFn = unsafe extern "system" fn(
    *mut *mut c_void,
    u32,
    *const ObjectAttributes,
    *mut c_void,
    *const i64,
    u32,
    u32,
    u32,
    u32,
    *const c_void,
    u32,
) -> i32;
pub type NtOpenFileFn = unsafe extern "system" fn(
    *mut *mut c_void,
    u32,
    *const ObjectAttributes,
    *mut c_void,
    u32,
    u32,
) -> i32;
pub type NtQueryAttributesFileFn =
    unsafe extern "system" fn(*const ObjectAttributes, *mut c_void) -> i32;
/// `NtQueryDirectoryFile`: a directory listing through an open handle. The information buffer is filled with one of the
/// `FILE_*_INFORMATION` structures, selected by `class`.
pub type NtQueryDirectoryFileFn = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    *const c_void,
    *const c_void,
    *mut c_void,
    *mut c_void,
    u32,
    u32,
    u8,
    *const UnicodeString,
    u8,
) -> i32;
/// `NtQueryDirectoryFileEx`: the same query with the restart and single-entry switches in one flags word.
pub type NtQueryDirectoryFileExFn = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    *const c_void,
    *const c_void,
    *mut c_void,
    *mut c_void,
    u32,
    u32,
    u32,
    *const UnicodeString,
) -> i32;
pub type NtSetInformationFileFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *const c_void, u32, u32) -> i32;
pub type ReplaceFileWFn = unsafe extern "system" fn(
    *const u16,
    *const u16,
    *const u16,
    u32,
    *mut c_void,
    *mut c_void,
) -> i32;
pub type CreateHardLinkWFn =
    unsafe extern "system" fn(*const u16, *const u16, *const c_void) -> i32;
pub type CreateSymbolicLinkWFn = unsafe extern "system" fn(*const u16, *const u16, u32) -> u8;

/// `UNICODE_STRING` as the NT layer uses it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnicodeString {
    pub length: u16,
    pub maximum_length: u16,
    pub buffer: *const u16,
}

/// `OBJECT_ATTRIBUTES` as the NT layer uses it. The loader opens DLLs through these, not through `CreateFileW`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObjectAttributes {
    pub length: u32,
    pub root_directory: *mut c_void,
    pub object_name: *const UnicodeString,
    pub attributes: u32,
    pub security_descriptor: *mut c_void,
    pub security_qos: *mut c_void,
}

// The layouts the hook writes by hand are the layouts of the system's structures, on both bitnesses: a change in either
// fails the build instead of corrupting a call.
const _: () = {
    use std::mem::{offset_of, size_of};
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Win32::Foundation::UNICODE_STRING;

    assert!(size_of::<UnicodeString>() == size_of::<UNICODE_STRING>());
    assert!(offset_of!(UnicodeString, length) == offset_of!(UNICODE_STRING, Length));
    assert!(offset_of!(UnicodeString, maximum_length) == offset_of!(UNICODE_STRING, MaximumLength));
    assert!(offset_of!(UnicodeString, buffer) == offset_of!(UNICODE_STRING, Buffer));
    assert!(size_of::<ObjectAttributes>() == size_of::<OBJECT_ATTRIBUTES>());
    assert!(offset_of!(ObjectAttributes, length) == offset_of!(OBJECT_ATTRIBUTES, Length));
    assert!(
        offset_of!(ObjectAttributes, root_directory)
            == offset_of!(OBJECT_ATTRIBUTES, RootDirectory)
    );
    assert!(offset_of!(ObjectAttributes, object_name) == offset_of!(OBJECT_ATTRIBUTES, ObjectName));
    assert!(offset_of!(ObjectAttributes, attributes) == offset_of!(OBJECT_ATTRIBUTES, Attributes));
    assert!(
        offset_of!(ObjectAttributes, security_descriptor)
            == offset_of!(OBJECT_ATTRIBUTES, SecurityDescriptor)
    );
    assert!(
        offset_of!(ObjectAttributes, security_qos)
            == offset_of!(OBJECT_ATTRIBUTES, SecurityQualityOfService)
    );
};
