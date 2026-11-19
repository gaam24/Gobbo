//! The Win32 file calls the hook redirects: opening, attributes, deleting, removing folders, moving, replacing and links. A
//! call whose path is outside the game folder goes ahead unchanged. Inside it, the layers decide: a read sees the visible
//! file, a write goes to the upper layer, and a deletion of a lower-layer file leaves a whiteout instead of touching the
//! game folder.

mod delete;
mod last_error;
mod link;
mod module;
mod move_file;
mod open;
mod replace;
mod source;

use windows_sys::Win32::Foundation::SetLastError;
use windows_sys::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

pub(crate) use delete::{delete_file, remove_directory};
pub(crate) use last_error::PreserveLastError;
pub(crate) use link::{create_hard_link, create_symbolic_link};
pub(crate) use module::{
    get_module_file_name, get_module_file_name_a, get_module_file_name_ex,
    get_module_file_name_ex_a,
};
pub(crate) use move_file::{move_file_ex, move_file_with_progress};
pub(crate) use open::create_file;
pub(crate) use replace::replace_file;

/// Sets the error Windows reports, and returns the failure value of a `BOOL` call.
pub(crate) fn fail(code: u32) -> i32 {
    set_error(code);
    0
}

/// Sets the error that the next `GetLastError` of this thread reports.
pub(crate) fn set_error(code: u32) {
    // SAFETY: `SetLastError` only stores the value for the calling thread.
    unsafe { SetLastError(code) };
}

/// The id of the calling thread, for the report.
pub(crate) fn thread_id() -> u32 {
    // SAFETY: `GetCurrentThreadId` takes no arguments and only reads the id of the calling thread.
    unsafe { GetCurrentThreadId() }
}

/// The name of a `WIN32_FIND_DATAW`, up to its terminator.
pub(crate) fn find_data_name(data: &WIN32_FIND_DATAW) -> &[u16] {
    &data.cFileName[..crate::text::nul_length(&data.cFileName)]
}
