//! `NtSetInformationFile`: a deletion or a rename made through a handle. Windows deletes a file through a handle when
//! `DeleteFileW` is not the caller, and a handle opened with `DELETE` access is in the upper layer, so the lower entry of
//! the same name is hidden and the game folder is not touched.

use std::ffi::c_void;

use gobbo_overlay_decisions::{Kind, Layer};
use windows_sys::Win32::Foundation::{
    STATUS_ACCESS_DENIED, STATUS_DIRECTORY_NOT_EMPTY, STATUS_INVALID_PARAMETER,
    STATUS_NOT_SUPPORTED, STATUS_OBJECT_NAME_COLLISION, STATUS_UNSUCCESSFUL,
};

use super::layout::{
    FILE_DISPOSITION_DELETE, FILE_DISPOSITION_INFORMATION, FILE_DISPOSITION_INFORMATION_EX,
    FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_EX, RENAME_NAME_AT,
};
use super::names::{join_root, nt_name_units};
use super::rename_info::{RenameInfo, build_rename, parse_rename};
use crate::diag::record_decision;
use crate::guard::enter_session;
use crate::handles::{self, LayerKind};
use crate::paths::{in_lower, make_write_target, name_taken, place_path, shown, unsafe_units};
use crate::search::has_visible_entries;
use crate::trace::trace;
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
    // A deletion decides before the original call whether it must refuse and what lower entry it may hide. The hide is
    // deferred until the original succeeds, so a failed deletion leaves the lower file visible.
    let mut hide_after_success: Option<(&'static crate::state::Mount, String)> = None;
    // Bind the guard outside the block: it must stay alive across the original call and the whiteout write below, so
    // every file call the hook makes there passes through untouched. `if let` would drop it at the end of the block.
    let guard = enter_session();
    if guard.is_some() {
        let deleting = unsafe { deletion_requested(class, info, length) };
        let renaming = matches!(class, FILE_RENAME_INFORMATION | FILE_RENAME_INFORMATION_EX);
        // A handle outside every layer cannot change a lower entry or the game folder. The final-path check uses a stack
        // buffer; a deletion or rename through it is a pass-through with no allocation here.
        if (deleting || renaming) && handles::layer_kind(handle) == Some(LayerKind::Outside) {
            // Fall through to the original call.
        } else {
            if deleting {
                match unsafe { deletion_whiteout_for(handle) } {
                    Ok(hide) => hide_after_success = hide,
                    Err(refusal) => return refusal,
                }
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
    // Capture the original call's error before `hide()` touches the disk; dropping `_last_error` restores it on return.
    // `guard` is still alive here, so the whiteout's own file calls pass through the hook untouched.
    let _last_error = PreserveLastError::capture();

    if let Some((mount, rel)) = whiteout_on_success(hide_after_success, result) {
        hide(mount, &rel);
    }

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

    // A device namespace can alias a file in the game folder: a rename onto it fails closed instead of passing
    // through as an unclassifiable name, the same way the open hooks refuse it. The name is unclassified, so this
    // refusal is traced and builds no ring path.
    if unsafe_units(&name.encode_utf16().collect::<Vec<u16>>()) {
        trace(|| format!("NtSetInformationFile refused a device namespace: {name}"));
        return Some(Err(STATUS_ACCESS_DENIED));
    }

    let real = handles::layer_path(handle)?;
    let Some((mount, source_rel)) = handles::relative_in_upper(&real) else {
        // A file of a lower layer: renaming it would write to that layer.
        if let Some((mount, rel)) = handles::relative_in_lower_layers(&real) {
            record_decision(
                Kind::Refuse,
                Layer::None,
                STATUS_ACCESS_DENIED as u32,
                || mount.decision_path(&rel),
            );
            return Some(Err(STATUS_ACCESS_DENIED));
        }
        return None;
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

    let Some(renamed) = build_rename(bytes, &new_name) else {
        return Some(Err(STATUS_INVALID_PARAMETER));
    };
    Some(Ok((renamed, hide_places)))
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

/// Where a deleted handle lies, as plain data so the refusal/hide decision can be tested without a real handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeletedPlace {
    /// The handle's final path could not be queried.
    Unqueryable,
    /// The handle is in a mount's upper layer.
    Upper {
        /// The entry is a folder that still has visible children.
        folder_has_entries: bool,
        /// Whether a lower copy exists; `None` when the layers could not be queried.
        lower: Option<bool>,
    },
    /// The handle is in one of a mount's lower layers.
    Lower,
    /// The handle is in no layer of any mount.
    Elsewhere,
}

/// The pure pre-call decision for a deletion. `Err` is the status to return before the original call; `Ok(Some(..))` is
/// the lower entry to hide once the original succeeds; `Ok(None)` passes the call through with no whiteout.
///
/// The whiteout is kept until a successful original call, so a deletion Windows refuses (a read-only attribute,
/// `STATUS_CANNOT_DELETE`, ...) leaves the lower file visible as it does in an unmodded run.
fn deletion_decision<T>(place: DeletedPlace, upper: Option<T>) -> Result<Option<T>, i32> {
    match place {
        DeletedPlace::Unqueryable | DeletedPlace::Elsewhere => Ok(None),
        DeletedPlace::Lower => Err(STATUS_ACCESS_DENIED),
        DeletedPlace::Upper {
            folder_has_entries,
            lower,
        } => {
            if folder_has_entries {
                Err(STATUS_DIRECTORY_NOT_EMPTY)
            } else if lower.unwrap_or(true) {
                // A lower copy that cannot be queried is still a copy to hide: the upper one is being deleted.
                Ok(upper)
            } else {
                Ok(None)
            }
        }
    }
}

/// The whiteout a deletion writes for the status the original call returned. A negative status writes nothing, so the
/// lower file stays visible even though the hook was asked to delete it.
fn whiteout_on_success<T>(hide: Option<T>, result: i32) -> Option<T> {
    if result >= 0 { hide } else { None }
}

/// Resolves `handle` into the whiteout to write after the original deletion succeeds. `Err` is the status to return
/// before calling the original; `Ok(None)` lets the call pass through. A handle to the game folder is refused, and so
/// is a folder that still has visible entries.
///
/// # Safety
/// `handle` must be a file handle of this process, or invalid.
unsafe fn deletion_whiteout_for(
    handle: *mut c_void,
) -> Result<Option<(&'static crate::state::Mount, String)>, i32> {
    let Some(real) = handles::layer_path(handle) else {
        return deletion_decision(DeletedPlace::Unqueryable, None);
    };

    let upper = handles::relative_in_upper(&real);
    let place = match upper.as_ref() {
        Some((mount, rel)) => {
            let folder_has_entries = is_folder(&real) && has_visible_entries(mount, rel);
            if folder_has_entries {
                // The folder refusal short-circuits the lower query, as it did before the split.
                DeletedPlace::Upper {
                    folder_has_entries: true,
                    lower: None,
                }
            } else {
                DeletedPlace::Upper {
                    folder_has_entries: false,
                    lower: in_lower(mount, rel).ok(),
                }
            }
        }
        None => {
            if let Some((mount, rel)) = handles::relative_in_lower_layers(&real) {
                record_decision(
                    Kind::Refuse,
                    Layer::None,
                    STATUS_ACCESS_DENIED as u32,
                    || mount.decision_path(&rel),
                );
                DeletedPlace::Lower
            } else {
                DeletedPlace::Elsewhere
            }
        }
    };
    deletion_decision(place, upper)
}

/// Whether a real path is a folder. A path that cannot be read is not.
fn is_folder(real: &str) -> bool {
    std::fs::metadata(real).is_ok_and(|meta| meta.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression is a Rust lifetime: binding the guard outside the `if` keeps it alive across the calls that
    /// follow. This pins the pattern without a real handle or detour table; the hook itself needs both.
    #[test]
    fn an_outer_guard_stays_inside_after_the_if_block() {
        use crate::guard::Guard;

        let guard = Guard::enter();
        assert!(guard.is_some());
        if guard.is_some() {
            assert!(Guard::enter().is_none());
        }
        // Still inside after the block, as the hook is across its original call and its whiteout write.
        assert!(Guard::enter().is_none());
        drop(guard);
        assert!(Guard::enter().is_some());
    }

    /// The upper entry to hide, as plain data: the decision needs no real handle or mount.
    fn upper() -> Option<&'static str> {
        Some("upper\\file.txt")
    }

    #[test]
    fn an_unqueryable_or_outside_handle_passes_without_a_whiteout() {
        assert_eq!(
            deletion_decision(DeletedPlace::Unqueryable, upper()),
            Ok(None)
        );
        assert_eq!(
            deletion_decision(DeletedPlace::Elsewhere, upper()),
            Ok(None)
        );
    }

    #[test]
    fn a_handle_in_a_lower_layer_is_refused_before_the_original_call() {
        assert_eq!(
            deletion_decision(DeletedPlace::Lower, upper()),
            Err(STATUS_ACCESS_DENIED)
        );
    }

    #[test]
    fn a_non_empty_folder_is_refused_before_the_original_call() {
        let place = DeletedPlace::Upper {
            folder_has_entries: true,
            lower: Some(true),
        };
        assert_eq!(
            deletion_decision(place, upper()),
            Err(STATUS_DIRECTORY_NOT_EMPTY)
        );
    }

    #[test]
    fn a_lower_copy_or_a_failed_query_is_hidden_after_success() {
        for lower in [Some(true), None] {
            let place = DeletedPlace::Upper {
                folder_has_entries: false,
                lower,
            };
            assert_eq!(deletion_decision(place, upper()), Ok(upper()));
        }
    }

    #[test]
    fn an_upper_file_without_a_lower_copy_passes_without_a_whiteout() {
        let place = DeletedPlace::Upper {
            folder_has_entries: false,
            lower: Some(false),
        };
        assert_eq!(deletion_decision(place, upper()), Ok(None));
    }

    /// The order is the regression: the whiteout is computed before the call but written only for a successful status.
    #[test]
    fn a_failed_deletion_leaves_the_lower_file_visible() {
        let hide = deletion_decision(
            DeletedPlace::Upper {
                folder_has_entries: false,
                lower: Some(true),
            },
            upper(),
        )
        .ok()
        .flatten();
        assert_eq!(whiteout_on_success(hide, STATUS_UNSUCCESSFUL), None);
        assert_eq!(whiteout_on_success(hide, 0), upper());
    }
}
