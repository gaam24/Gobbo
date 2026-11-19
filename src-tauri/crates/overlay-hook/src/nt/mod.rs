//! The NT layer under the Win32 file API. The loader opens DLLs through these calls, and programs that skip `KernelBase`
//! reach the files through them too, so the same redirection applies here. Object names relative to a directory handle
//! are not redirected: the directory handle was opened through a call that was already redirected.
//!
//! - `open`: `NtCreateFile` and `NtOpenFile`, and the deletion of a file opened for it.
//! - `query`: `NtQueryAttributesFile` and `NtQueryFullAttributesFile`, the loader's existence checks.
//! - `set_info`: `NtSetInformationFile`, the deletions and renames made through a handle.
//! - `directory`: `NtQueryDirectoryFile`, a merged directory listing through a handle.
//! - `names`: the object names of the attributes, and the redirected attributes.
//! - `rename_info`: the rename structure, read from the program's bytes and built for the upper layer.
//! - `layout`: the constants and the offsets of the NT structures the hook reads.

mod directory;
mod layout;
mod names;
mod open;
mod query;
mod rename_info;
mod set_info;

pub(crate) use directory::{nt_query_directory_file, nt_query_directory_file_ex};
pub(crate) use open::{nt_create_file, nt_open_file};
pub(crate) use query::{nt_query_attributes_file, nt_query_full_attributes_file};
pub(crate) use set_info::nt_set_information_file;

use crate::trace::call_with;
use crate::types::ObjectAttributes;

/// Counts one NT call (see `diag`) and logs it with the object name of its attributes, when `GOBBO_OVERLAY_LOG` is set.
///
/// # Safety
/// `attributes` must point to an `OBJECT_ATTRIBUTES` of the program, or be null.
pub(super) unsafe fn trace_nt(operation: &str, attributes: *const ObjectAttributes) {
    call_with(operation, || unsafe { names::object_name(attributes) });
}
