//! `DeleteFileW` and `RemoveDirectoryW`: a file or an empty folder of the upper layer is removed there, and an entry of a
//! lower layer is hidden by a whiteout. The game folder is never changed.

use std::path::PathBuf;

use gobbo_overlay_core::{Layer, wide};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_DIR_NOT_EMPTY, ERROR_DIRECTORY, ERROR_FILE_NOT_FOUND,
    ERROR_INVALID_FUNCTION,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;

use super::{PreserveLastError, fail};
use crate::guard::Guard;
use crate::paths::{in_lower, missing, place, shown, unsafe_name};
use crate::search::has_visible_entries;
use crate::state::Mount;
use crate::trace::{trace, trace_call};
use crate::whiteouts::hide;

/// The shared body of `DeleteFileW` and `RemoveDirectoryW`: the guard, the in-game answer, and the pass-through.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
unsafe fn delete_or_remove(
    operation: &'static str,
    name: *const u16,
    in_game: impl FnOnce(&Mount, &str) -> i32,
    original: impl FnOnce() -> i32,
) -> i32 {
    unsafe { trace_call(operation, name) };

    // Keep the guard across the original call: the NT layer would classify the same path again.
    let guard = Guard::enter();
    // A device namespace can alias a file in the game folder. Like `CreateFileW`, the refusal needs a real session:
    // without one there is nothing to refuse, and the hook's own re-entrant calls pass through untouched.
    if guard.is_some() && unsafe { unsafe_name(name) } {
        return fail(ERROR_ACCESS_DENIED);
    }

    if guard.is_some()
        && let Some(place) = unsafe { place(name) }
    {
        return in_game(place.mount, &place.rel);
    }

    if crate::diag::settings().pass_writes {
        unsafe { crate::trace::pass_through(operation, name, String::new) };
    }

    let result = original();
    let _last_error = PreserveLastError::capture();
    result
}

/// `DeleteFileW`: a file of the upper layer is deleted there, and a file of a lower layer is hidden by a whiteout.
/// The game folder is never changed.
///
/// # Safety
/// Called by the program with the arguments of `DeleteFileW`.
pub(crate) unsafe extern "system" fn delete_file(name: *const u16) -> i32 {
    let deleted = unsafe {
        delete_or_remove("DeleteFileW", name, delete_in_game, || {
            let original = original!(delete_file, {
                fail(ERROR_INVALID_FUNCTION);
                0
            });
            original.call(name)
        })
    };
    trace(|| format!("result DeleteFileW deleted={deleted}"));
    deleted
}

/// What deleting a game-relative file does, decided from what the layers show.
#[derive(Debug, PartialEq, Eq)]
enum DeletePlan {
    /// The name is a folder. A folder is removed with `RemoveDirectoryW`, not here.
    Refuse,
    /// No layer shows the file, so the call fails as it does for a missing file.
    Missing,
    /// The upper copy is deleted when `upper` is set, and a lower entry is hidden when `hide` is set.
    Remove { upper: bool, hide: bool },
}

/// The plan for a deletion: `is_folder` is whether the name is a visible folder, `in_upper` whether the upper copy is a
/// file, and `below` whether a lower entry is visible.
fn delete_plan(is_folder: bool, in_upper: bool, below: bool) -> DeletePlan {
    if is_folder {
        DeletePlan::Refuse
    } else if !in_upper && !below {
        DeletePlan::Missing
    } else {
        DeletePlan::Remove {
            upper: in_upper,
            hide: below,
        }
    }
}

/// One visible entry: its layer, its path there, and its attributes.
type ShownEntry = (Layer, PathBuf, u32);

/// The visible entry and whether a lower layer has it, or the Win32 error a probe reported.
fn shown_and_lower(mount: &Mount, rel: &str) -> Result<(Option<ShownEntry>, bool), u32> {
    let shown = match shown(mount, rel) {
        Ok(shown) => shown,
        Err(failure) => return Err(failure.win32()),
    };
    let below = match in_lower(mount, rel) {
        Ok(below) => below,
        Err(failure) => return Err(failure.win32()),
    };
    Ok((shown, below))
}

/// Deletes the upper copy when there is one, hides a lower entry, and keeps the original call's error across the
/// whiteout write. `upper` is the copy to remove; `missing_upper` is the error a caller reports when it expected a copy
/// that was not shown. `remove` performs the original call on the wide upper path.
fn remove_upper_and_hide_below(
    mount: &Mount,
    rel: &str,
    upper: Option<&PathBuf>,
    missing_upper: Option<u32>,
    below: bool,
    remove: impl FnOnce(*const u16) -> i32,
) -> i32 {
    // The whiteout write below does its own file I/O. Keep the error of the original call until after it.
    // With no upper copy there was no original call, so capture the incoming error just before the write.
    let _last_error = if let Some(upper) = upper {
        let upper_wide = wide(upper.as_os_str());
        let result = remove(upper_wide.as_ptr());
        let last_error = PreserveLastError::capture();

        if result == 0 {
            return 0;
        }

        Some(last_error)
    } else if let Some(code) = missing_upper {
        return fail(code);
    } else if below {
        Some(PreserveLastError::capture())
    } else {
        None
    };

    if below {
        hide(mount, rel);
    }

    1
}

/// The deletion of a game-relative file: the upper copy is deleted, and a lower entry is hidden.
fn delete_in_game(mount: &Mount, rel: &str) -> i32 {
    let (shown, below) = match shown_and_lower(mount, rel) {
        Ok(result) => result,
        Err(code) => return fail(code),
    };
    let plan = delete_plan(
        shown
            .as_ref()
            .is_some_and(|(_, _, attributes)| attributes & FILE_ATTRIBUTE_DIRECTORY != 0),
        // The upper copy is a file exactly when `shown` picked the upper layer and it is not a folder. Asking
        // `upper.is_file()` was one extra metadata call for the same answer.
        shown.as_ref().is_some_and(|(layer, _, attributes)| {
            *layer == Layer::Upper && attributes & FILE_ATTRIBUTE_DIRECTORY == 0
        }),
        below,
    );

    match plan {
        DeletePlan::Refuse => fail(ERROR_ACCESS_DENIED),
        DeletePlan::Missing => fail(match missing(mount, rel) {
            Ok(missing) => missing.win32(),
            Err(failure) => failure.win32(),
        }),
        DeletePlan::Remove {
            upper: in_upper,
            hide: below,
        } => {
            let (upper, missing_upper) = if in_upper {
                match shown.as_ref() {
                    Some((_, upper, _)) => (Some(upper), None),
                    // Defensive: the plan asked for an upper copy that the visible entry did not provide.
                    None => (None, Some(ERROR_FILE_NOT_FOUND)),
                }
            } else {
                (None, None)
            };
            remove_upper_and_hide_below(
                mount,
                rel,
                upper,
                missing_upper,
                below,
                |upper_wide| unsafe {
                    let original = original!(delete_file, {
                        fail(ERROR_INVALID_FUNCTION);
                        0
                    });
                    original.call(upper_wide)
                },
            )
        }
    }
}

/// `RemoveDirectoryW`: an empty folder of the upper layer is removed, and a folder of a lower layer is hidden by a
/// whiteout, which hides everything below it too. The game folder itself cannot be removed.
///
/// # Safety
/// Called by the program with the arguments of `RemoveDirectoryW`.
pub(crate) unsafe extern "system" fn remove_directory(name: *const u16) -> i32 {
    unsafe {
        delete_or_remove("RemoveDirectoryW", name, remove_in_game, || {
            let original = original!(remove_directory, {
                fail(ERROR_INVALID_FUNCTION);
                0
            });
            original.call(name)
        })
    }
}

/// The first reason a folder cannot be removed, decided from what the layers show. `is_root` is the game folder itself,
/// `shown` is whether a layer shows the folder (and whether that copy is a folder), and `has_entries` says whether the
/// folder still lists an entry. `None` means the folder can be removed.
fn removal_error(
    is_root: bool,
    shown: Option<bool>,
    missing: impl FnOnce() -> u32,
    has_entries: impl FnOnce() -> bool,
) -> Option<u32> {
    if is_root {
        return Some(ERROR_ACCESS_DENIED);
    }

    match shown {
        None => Some(missing()),
        Some(false) => Some(ERROR_DIRECTORY),
        Some(true) if has_entries() => Some(ERROR_DIR_NOT_EMPTY),
        Some(true) => None,
    }
}

/// The removal of a game-relative folder: an empty upper copy is removed, and a lower entry is hidden.
fn remove_in_game(mount: &Mount, rel: &str) -> i32 {
    let (shown, below) = match shown_and_lower(mount, rel) {
        Ok(result) => result,
        Err(code) => return fail(code),
    };
    let shown_is_folder = shown
        .as_ref()
        .map(|(_, _, attributes)| attributes & FILE_ATTRIBUTE_DIRECTORY != 0);
    let missing_code = || match missing(mount, rel) {
        Ok(missing) => missing.win32(),
        Err(failure) => failure.win32(),
    };

    if let Some(code) = removal_error(rel.is_empty(), shown_is_folder, missing_code, || {
        has_visible_entries(mount, rel)
    }) {
        return fail(code);
    }
    // The upper folder is the one `shown` returned; there is no need to ask the disk about it again.
    let upper = match shown.as_ref() {
        Some((Layer::Upper, upper, _)) => Some(upper),
        _ => None,
    };
    remove_upper_and_hide_below(mount, rel, upper, None, below, |upper_wide| unsafe {
        let original = original!(remove_directory, {
            fail(ERROR_INVALID_FUNCTION);
            0
        });
        original.call(upper_wide)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win32::set_error;
    use gobbo_overlay_core::{Layers, Whiteouts};
    use windows_sys::Win32::Foundation::{ERROR_PATH_NOT_FOUND, GetLastError};

    fn test_mount() -> &'static Mount {
        let layers = Layers::new(
            PathBuf::from(r"C:\Game"),
            None,
            PathBuf::from(r"C:\Game\upper"),
        );
        Box::leak(Box::new(Mount::new(
            "test".to_string(),
            PathBuf::from(r"C:\Game"),
            layers,
            Whiteouts::default(),
        )))
    }

    #[test]
    fn a_failed_removal_leaves_the_lower_file_visible_and_restores_the_error() {
        let mount = test_mount();
        let upper = PathBuf::from(r"C:\Game\upper\data\a.txt");
        // SAFETY: the test owns this thread's error.
        set_error(1111);
        let result =
            remove_upper_and_hide_below(mount, r"data\a.txt", Some(&upper), None, true, |_| {
                // SAFETY: same as above; this stands for the original call's error.
                set_error(4321);
                0
            });
        // SAFETY: same as above.
        let error = unsafe { GetLastError() };

        assert_eq!(result, 0);
        assert_eq!(error, 4321);
        assert!(!mount.whiteouts_snapshot().hides(r"data\a.txt"));
    }

    #[test]
    fn a_successful_removal_hides_the_lower_file_and_restores_the_error() {
        let mount = test_mount();
        let upper = PathBuf::from(r"C:\Game\upper\data\a.txt");
        // SAFETY: the test owns this thread's error.
        set_error(1111);
        let result =
            remove_upper_and_hide_below(mount, r"data\a.txt", Some(&upper), None, true, |_| {
                // SAFETY: same as above; this stands for the original call's error.
                set_error(4321);
                1
            });
        // SAFETY: same as above.
        let error = unsafe { GetLastError() };

        assert_eq!(result, 1);
        assert_eq!(error, 4321);
        assert!(mount.whiteouts_snapshot().hides(r"data\a.txt"));
    }

    #[test]
    fn a_missing_upper_copy_reports_the_callers_error_without_hiding() {
        let mount = test_mount();
        let result = remove_upper_and_hide_below(
            mount,
            r"data\a.txt",
            None,
            Some(ERROR_FILE_NOT_FOUND),
            true,
            |_| panic!("there is no upper copy to remove"),
        );
        // SAFETY: the test owns this thread's error.
        let error = unsafe { GetLastError() };

        assert_eq!(result, 0);
        assert_eq!(error, ERROR_FILE_NOT_FOUND);
        assert!(!mount.whiteouts_snapshot().hides(r"data\a.txt"));
    }

    #[test]
    fn a_folder_is_refused_and_a_missing_file_is_missing() {
        assert_eq!(delete_plan(true, false, true), DeletePlan::Refuse);
        assert_eq!(delete_plan(false, false, false), DeletePlan::Missing);
    }

    #[test]
    fn an_upper_copy_is_deleted_and_a_lower_entry_is_hidden() {
        assert_eq!(
            delete_plan(false, true, false),
            DeletePlan::Remove {
                upper: true,
                hide: false
            }
        );
        assert_eq!(
            delete_plan(false, false, true),
            DeletePlan::Remove {
                upper: false,
                hide: true
            }
        );
        assert_eq!(
            delete_plan(false, true, true),
            DeletePlan::Remove {
                upper: true,
                hide: true
            }
        );
    }

    #[test]
    fn a_folder_removal_names_the_first_reason_it_is_refused() {
        let no_entries = || false;
        assert_eq!(
            removal_error(true, Some(true), || ERROR_FILE_NOT_FOUND, no_entries),
            Some(ERROR_ACCESS_DENIED)
        );
        assert_eq!(
            removal_error(false, None, || ERROR_PATH_NOT_FOUND, no_entries),
            Some(ERROR_PATH_NOT_FOUND)
        );
        assert_eq!(
            removal_error(false, None, || ERROR_FILE_NOT_FOUND, no_entries),
            Some(ERROR_FILE_NOT_FOUND)
        );
        assert_eq!(
            removal_error(false, Some(false), || ERROR_FILE_NOT_FOUND, no_entries),
            Some(ERROR_DIRECTORY)
        );
        assert_eq!(
            removal_error(false, Some(true), || ERROR_FILE_NOT_FOUND, || true),
            Some(ERROR_DIR_NOT_EMPTY)
        );
        assert_eq!(
            removal_error(false, Some(true), || ERROR_FILE_NOT_FOUND, no_entries),
            None
        );
    }
}
