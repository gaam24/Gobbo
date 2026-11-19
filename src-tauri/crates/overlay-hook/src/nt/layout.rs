//! The constants of the NT file calls, and the layout of the information structures the hook reads from them.

/// `FILE_OPEN` and the dispositions that replace the file's contents. Other dispositions create or open for writing.
pub(super) const FILE_OPEN: u32 = 1;
pub(super) const FILE_CREATE: u32 = 2;
pub(super) const FILE_SUPERSEDE: u32 = 0;
pub(super) const FILE_OVERWRITE: u32 = 4;
pub(super) const FILE_OVERWRITE_IF: u32 = 5;

/// The create option that deletes the file when its last handle closes.
pub(super) const FILE_DELETE_ON_CLOSE: u32 = 0x0000_1000;

/// The information classes of `NtSetInformationFile` that delete a file: the disposition, in its two forms.
pub(super) const FILE_DISPOSITION_INFORMATION: u32 = 13;
pub(super) const FILE_DISPOSITION_INFORMATION_EX: u32 = 64;
pub(super) const FILE_DISPOSITION_DELETE: u32 = 0x1;

/// `FILE_RENAME_INFORMATION` (class 10) and `FILE_RENAME_INFORMATION_EX` (class 65). The flags are at 0
/// (`ReplaceIfExists` is the first byte of the plain form and a flag in the `_EX` form); the root directory and the name
/// length follow at pointer-size-dependent offsets.
pub(super) const FILE_RENAME_INFORMATION: u32 = 10;
pub(super) const FILE_RENAME_INFORMATION_EX: u32 = 65;
pub(super) const FILE_RENAME_REPLACE_IF_EXISTS: u32 = 0x1;

/// The offset and size of the root directory handle: 8 bytes on 64-bit Windows, 4 on 32-bit.
#[cfg(target_pointer_width = "64")]
pub(super) const RENAME_ROOT_AT: usize = 8;
#[cfg(target_pointer_width = "32")]
pub(super) const RENAME_ROOT_AT: usize = 4;
pub(super) const RENAME_ROOT_SIZE: usize = std::mem::size_of::<usize>();

/// The offset of the name length in bytes, a `u32`.
#[cfg(target_pointer_width = "64")]
pub(super) const RENAME_LENGTH_AT: usize = 16;
#[cfg(target_pointer_width = "32")]
pub(super) const RENAME_LENGTH_AT: usize = 8;

/// The offset of the name, after the flags, the root directory and the length.
#[cfg(target_pointer_width = "64")]
pub(super) const RENAME_NAME_AT: usize = 20;
#[cfg(target_pointer_width = "32")]
pub(super) const RENAME_NAME_AT: usize = 12;

// The offsets above are the fields of the system's `FILE_RENAME_INFORMATION` on both bitnesses.
const _: () = {
    use std::mem::offset_of;
    use windows_sys::Wdk::Storage::FileSystem::FILE_RENAME_INFORMATION;

    assert!(offset_of!(FILE_RENAME_INFORMATION, RootDirectory) == RENAME_ROOT_AT);
    assert!(offset_of!(FILE_RENAME_INFORMATION, FileNameLength) == RENAME_LENGTH_AT);
    assert!(offset_of!(FILE_RENAME_INFORMATION, FileName) == RENAME_NAME_AT);
};
