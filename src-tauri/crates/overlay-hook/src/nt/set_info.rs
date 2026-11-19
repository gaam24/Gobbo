//! `NtSetInformationFile`: a deletion or a rename made through a handle. Windows deletes a file through a handle when
//! `DeleteFileW` is not the caller, and a handle opened with `DELETE` access is in the upper layer, so the lower entry of
//! the same name is hidden and the game folder is not touched.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{
    STATUS_ACCESS_DENIED, STATUS_DIRECTORY_NOT_EMPTY, STATUS_NOT_SUPPORTED,
    STATUS_OBJECT_NAME_COLLISION, STATUS_UNSUCCESSFUL,
};

use super::layout::{
    FILE_DISPOSITION_DELETE, FILE_DISPOSITION_INFORMATION, FILE_DISPOSITION_INFORMATION_EX,
    FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_EX, RENAME_NAME_AT,
};
use super::names::{join_root, nt_name_units};
use super::rename_info::{RenameInfo, build_rename, parse_rename};
use crate::guard::enter_session;
use crate::handles::{self, LayerKind};
use crate::paths::{in_lower, make_write_target, name_taken, place_path, shown};
use crate::search::has_visible_entries;
use crate::whiteouts::hide;
use crate::win32::PreserveLastError;

/// `NtSetInformationFile`: a deletion made through a handle. Windows deletes a file through a handle when `DeleteFileW`
/// is not the caller, and the handle was opened in the upper layer (any open with `DELETE` access is). The lower entry
/// of the same name is hidden, so the file does not come back; the game folder is not touched.
///
/// # Safety
/// Called by the program with the arguments of `NtSetInformationFile`.
pub(crate) unsafe extern "system" fn nt_set_information_file(
    handle: *mut c_void,
    status: *mut c_void,
    info: *const c_void,
    length: u32,
    class: u32,
) -> i32 {
    if let Some(_guard) = enter_session() {
        let deleting = unsafe { deletion_requested(class, info, length) };
        let renaming = matches!(class, FILE_RENAME_INFORMATION | FILE_RENAME_INFORMATION_EX);
        // A handle outside every layer cannot change a lower entry or the game folder. The final-path check uses a stack
        // buffer; a deletion or rename through it is a pass-through with no allocation here.
        if (deleting || renaming) && handles::layer_kind(handle) == Some(LayerKind::Outside) {
            // Fall through to the original call.
        } else {
            if deleting && let Some(refusal) = unsafe { hide_deleted(handle) } {
                return refusal;
            }
            if let Some(result) = unsafe { rename_in_game(handle, info, length, class) } {
                return match result {
                    Ok((renamed, hide_places)) => {
                        let original = original!(nt_set_information_file, STATUS_UNSUCCESSFUL);
                        let result = unsafe {
                            original.call(
                                handle,
                                status,
                                renamed.as_ptr().cast(),
                                renamed.len() as u32,
                                class,
                            )
                        };
                        let _last_error = PreserveLastError::capture();
                        if result >= 0 {
                            for (mount, rel) in hide_places {
                                hide(mount, &rel);
                            }
                        }
                        result
                    }
                    Err(refusal) => refusal,
                };
            }
        }
    }
    let original = original!(nt_set_information_file, STATUS_UNSUCCESSFUL);
    let result = unsafe { original.call(handle, status, info, length, class) };
    let _last_error = PreserveLastError::capture();
    result
}

/// The information a rename passes on, and the mount and relative path to hide once the rename succeeds.
type Rename = (Vec<u8>, Vec<(&'static crate::state::Mount, String)>);

/// Renames a file through `handle`, when the rename is about the game folder. The handle is to a file of the upper layer
/// when the program opened it for deletion (see `redirect`). The new name goes to the upper layer. The result is the
/// information to pass on, and the game-relative path to hide once the rename succeeds, when a lower entry is left
/// behind. `None` lets the call go ahead unchanged. `Err` is the status that refuses the rename.
///
/// # Safety
/// `info` must point to `length` bytes of the information structure, or be null.
unsafe fn rename_in_game(
    handle: *mut c_void,
    info: *const c_void,
    length: u32,
    class: u32,
) -> Option<Result<Rename, i32>> {
    if !matches!(class, FILE_RENAME_INFORMATION | FILE_RENAME_INFORMATION_EX)
        || info.is_null()
        || (length as usize) < RENAME_NAME_AT
    {
        return None;
    }
    // SAFETY: the length check above proves the structure is readable.
    let bytes = unsafe { std::slice::from_raw_parts(info.cast::<u8>(), length as usize) };
    let RenameInfo {
        replace,
        root,
        name,
    } = parse_rename(bytes, class)?;

    let real = handles::layer_path(handle)?;
    let Some((mount, source_rel)) = handles::relative_in_upper(&real) else {
        // A file of a lower layer: renaming it would write to that layer.
        return handles::relative_in_lower_layers(&real)
            .is_some()
            .then_some(Err(STATUS_ACCESS_DENIED));
    };
    let target = if root == 0 {
        name.strip_prefix(r"\??\")?.to_string()
    } else {
        join_root(&handles::program_path(root as *mut c_void)?, &name)
    };
    let below = match in_lower(mount, &source_rel) {
        Ok(below) => below,
        Err(failure) => return Some(Err(failure.status())),
    };
    if is_folder(&real) && below {
        // A folder with a lower part would stay where it was, as `MoveFileExW` refuses it too.
        return Some(Err(STATUS_NOT_SUPPORTED));
    }
    let mut hide_places: Vec<(&'static crate::state::Mount, String)> = below
        .then(|| (mount, source_rel.clone()))
        .into_iter()
        .collect();
    let Some(target_place) = place_path(&target) else {
        // The rename leaves every mount: the file goes there as it is, and the lower entry it leaves is hidden.
        return Some(Ok((bytes.to_vec(), hide_places)));
    };
    if !std::ptr::eq(mount, target_place.mount) {
        // A rename between two mounts would move a file between two profile layers.
        return Some(Err(STATUS_ACCESS_DENIED));
    }
    let target_shown = match shown(mount, &target_place.rel) {
        Ok(shown) => shown.is_some(),
        Err(failure) => return Some(Err(failure.status())),
    };
    let same_entry = gobbo_overlay_core::same_folded(&target_place.rel, &source_rel);
    if name_taken(same_entry, replace, || target_shown) {
        return Some(Err(STATUS_OBJECT_NAME_COLLISION));
    }
    let new_name = match make_write_target(mount, &target_place.rel) {
        Ok(path) => nt_name_units(&path),
        Err(failure) => return Some(Err(failure.status())),
    };
    // A rename that replaces a file of a lower layer replaces it for the program: the lower file is hidden once the rename
    // succeeds, as `MoveFileExW` hides it (see `move_file`).
    if replace && matches!(in_lower(target_place.mount, &target_place.rel), Ok(true)) {
        hide_places.push((target_place.mount, target_place.rel.clone()));
    }
    Some(Ok((build_rename(bytes, &new_name), hide_places)))
}

/// Whether the information asks to delete the file on close.
///
/// # Safety
/// `info` must point to `length` bytes of the information structure, or be null.
unsafe fn deletion_requested(class: u32, info: *const c_void, length: u32) -> bool {
    if info.is_null() {
        return false;
    }
    // SAFETY (both reads): the length check below proves the structure is at least as long as the field read.
    match class {
        FILE_DISPOSITION_INFORMATION if length >= 1 => unsafe { *info.cast::<u8>() != 0 },
        FILE_DISPOSITION_INFORMATION_EX if length >= 4 => unsafe {
            std::ptr::read_unaligned(info.cast::<u32>()) & FILE_DISPOSITION_DELETE != 0
        },
        _ => false,
    }
}

/// Hides the lower entry of a file deleted through `handle`. `None` lets the deletion go ahead. A handle to the mod layer
/// or to the game folder is refused, and so is a folder that still has visible entries.
///
/// # Safety
/// `handle` must be a file handle of this process, or invalid.
unsafe fn hide_deleted(handle: *mut c_void) -> Option<i32> {
    let real = handles::layer_path(handle)?;
    if let Some((mount, rel)) = handles::relative_in_upper(&real) {
        if is_folder(&real) && has_visible_entries(mount, &rel) {
            return Some(STATUS_DIRECTORY_NOT_EMPTY);
        }
        // A lower copy that cannot be queried is still a copy to hide: the upper one is about to be deleted.
        if in_lower(mount, &rel).unwrap_or(true) {
            hide(mount, &rel);
        }
        return None;
    }
    handles::relative_in_lower_layers(&real)
        .is_some()
        .then_some(STATUS_ACCESS_DENIED)
}

/// Whether a real path is a folder. A path that cannot be read is not.
fn is_folder(real: &str) -> bool {
    std::fs::metadata(real).is_ok_and(|meta| meta.is_dir())
}
