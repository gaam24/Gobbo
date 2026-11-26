//! The listings that a program reads through `FindFirstFileW`, `FindNextFileW` and `FindClose`. The entries are kept in our
//! own table, keyed by a handle far from the ones Windows gives out, so `FindNextFileW` and `FindClose` know which calls
//! are ours.

use rustc_hash::FxHashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_FUNCTION, ERROR_NO_MORE_FILES, ERROR_PATH_NOT_FOUND,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FIND_FIRST_EX_CASE_SENSITIVE, FIND_FIRST_EX_LARGE_FETCH,
    FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY, FindExInfoBasic, FindExInfoStandard,
    FindExSearchLimitToDirectories, FindExSearchNameMatch, WIN32_FIND_DATAW,
};

use super::listing::{RootStream, Taken, find_layers_own};
use crate::guard::Guard;
use crate::paths::{place, shown};
use crate::trace::{trace, trace_call};
use crate::win32::{PreserveLastError, fail, set_error};

/// One folder listing that a program is reading: the entries of the upper and staging layers (and the `.` and `..` the
/// listing starts with), and the root layer, which is read as the program asks for it.
struct Search {
    taken: Taken,
    next: usize,
    root: Option<RootStream>,
}

impl Search {
    /// Writes the next entry into `out`, or returns false at the end.
    ///
    /// # Safety
    /// `out` must be writable for one `WIN32_FIND_DATAW`.
    unsafe fn advance_into(&mut self, out: *mut WIN32_FIND_DATAW) -> bool {
        if let Some(entry) = self.taken.entries.get(self.next) {
            unsafe { *out = *entry };
            self.next += 1;
            return true;
        }

        let Some(root) = self.root.as_mut() else {
            return false;
        };

        if unsafe { root.next_visible_into(&self.taken, out) } {
            self.next += 1;
            true
        } else {
            false
        }
    }
}

/// The listings of this process by handle. Each listing has its own lock, so the root layer is read (a disk call) with
/// the table unlocked; the table lock is held only to find or remove a listing.
static SEARCHES: LazyLock<Mutex<FxHashMap<usize, Arc<Mutex<Search>>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));
/// The maximum number of live listings. A new listing at the cap passes to the original call instead of evicting a
/// live handle, so a program that forgets to close handles cannot grow the table and no enumeration is reset.
const MAX_SEARCHES: usize = 128;
/// The first handle of our listings. It is an odd value well above a normal aligned kernel handle, and it still fits a
/// 32-bit `usize`.
const SEARCH_BASE: usize = (1usize << (usize::BITS - 8)) | 1;
static NEXT_SEARCH: AtomicUsize = AtomicUsize::new(SEARCH_BASE);

/// Hands out the next handle key. The counter counts up from `SEARCH_BASE`; a value outside the valid window is never
/// handed out, because the table may still hold an entry with that key from an earlier pass through the window.
fn next_search_key() -> usize {
    loop {
        let key = NEXT_SEARCH.fetch_add(1, Ordering::Relaxed);

        if (SEARCH_BASE..=usize::MAX - 1).contains(&key) {
            return key;
        }

        // Only the thread that still sees its own post-fetch value may reset; a failed CAS means another thread
        // already moved the counter, so this thread just takes a fresh key.
        let _ = NEXT_SEARCH.compare_exchange(
            key.wrapping_add(1),
            SEARCH_BASE,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }
}

/// Whether a handle can be one of our listings. A real handle is small and aligned; this check runs before the lock, so a
/// real handle never pays for the table.
fn ours(handle: *mut c_void) -> bool {
    handle != INVALID_HANDLE_VALUE && handle as usize >= SEARCH_BASE
}

/// Whether `FindFirstFileExW` may build a merged listing for these arguments. A level, search mode or flag the hook does
/// not implement is passed to the original call instead of pretending it worked.
fn supported_ex(level: i32, search: i32, filter: *const c_void, flags: u32) -> bool {
    let level = level == FindExInfoStandard || level == FindExInfoBasic;
    let search = search == FindExSearchNameMatch || search == FindExSearchLimitToDirectories;
    let known = FIND_FIRST_EX_CASE_SENSITIVE
        | FIND_FIRST_EX_LARGE_FETCH
        | FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY;
    level && search && filter.is_null() && flags & !known == 0
}

/// Whether the folder part of a pattern has a wildcard. The hook resolves one folder with `shown` and asks the system
/// only for the names inside it; a wildcard folder needs the original call, which expands every matching folder.
fn folder_has_wildcard(rel: &str) -> bool {
    rel.contains('*') || rel.contains('?')
}

/// The shared body of `FindFirstFileW` and `FindFirstFileExW`. `start` builds a merged listing, or returns `None` to
/// use the original call; `call` performs that call and returns `Err` when its `original!` fallback already left.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null. `start` and `call` must pass the caller's arguments on
/// unchanged.
unsafe fn find_first_with(
    operation: &'static str,
    name: *const u16,
    start: impl FnOnce() -> Option<*mut c_void>,
    call: impl FnOnce() -> Result<*mut c_void, *mut c_void>,
) -> *mut c_void {
    unsafe { trace_call(operation, name) };
    // Keep the guard across the original call: `FindFirstFileW` reaches the NT layer, where a second hooked call would
    // classify the same path again.
    let guard = Guard::enter();

    if guard.is_some()
        && let Some(handle) = start()
    {
        trace(|| format!("result {operation} listed handle={handle:?}"));
        return handle;
    }

    match call() {
        Ok(handle) => {
            let _last_error = PreserveLastError::capture();
            trace(|| format!("result {operation} handle={handle:?}"));
            handle
        }
        // The fallback left before an original call produced a result, so there is no result to restore or trace.
        Err(handle) => handle,
    }
}

/// `FindFirstFileW`: a listing of a folder of the game, merged from the layers.
///
/// # Safety
/// Called by the program with the arguments of `FindFirstFileW`.
pub(crate) unsafe extern "system" fn find_first(
    name: *const u16,
    data: *mut WIN32_FIND_DATAW,
) -> *mut c_void {
    unsafe {
        find_first_with(
            "FindFirstFileW",
            name,
            || start_search(name, data, FindExInfoStandard, FindExSearchNameMatch, 0),
            || {
                let original = original!(find_first, {
                    fail(ERROR_INVALID_FUNCTION);
                    Err(INVALID_HANDLE_VALUE)
                });
                Ok(original.call(name, data))
            },
        )
    }
}

/// `FindFirstFileExW`: the same as `FindFirstFileW` for the standard information levels.
///
/// # Safety
/// Called by the program with the arguments of `FindFirstFileExW`.
pub(crate) unsafe extern "system" fn find_first_ex(
    name: *const u16,
    level: i32,
    data: *mut c_void,
    search: i32,
    filter: *const c_void,
    flags: u32,
) -> *mut c_void {
    trace(|| format!("args FindFirstFileExW level={level} search={search} flags=0x{flags:x}"));
    unsafe {
        find_first_with(
            "FindFirstFileExW",
            name,
            || {
                if !supported_ex(level, search, filter, flags) {
                    return None;
                }
                start_search(name, data.cast(), level, search, flags)
            },
            || {
                let original = original!(find_first_ex, {
                    fail(ERROR_INVALID_FUNCTION);
                    Err(INVALID_HANDLE_VALUE)
                });
                Ok(original.call(name, level, data, search, filter, flags))
            },
        )
    }
}

/// `FindNextFileW`: the next entry of one of our listings, or the original call for any other handle.
///
/// # Safety
/// Called by the program with the arguments of `FindNextFileW`.
pub(crate) unsafe extern "system" fn find_next(
    handle: *mut c_void,
    data: *mut WIN32_FIND_DATAW,
) -> i32 {
    if ours(handle)
        && let Some(result) = unsafe { next_entry(handle, data) }
    {
        trace(|| format!("result FindNextFileW listed handle={handle:?} -> {result}"));
        return result;
    }
    // The original call continues a listing through `NtQueryDirectoryFile`; the guard keeps the NT hook from treating
    // that internal call as a new game listing.
    let guard = Guard::enter();
    let original = original!(find_next, {
        fail(ERROR_INVALID_FUNCTION);
        0
    });
    let result = unsafe { original.call(handle, data) };
    let _last_error = PreserveLastError::capture();
    drop(guard);
    trace(|| format!("result FindNextFileW handle={handle:?} -> {result}"));
    result
}

/// `FindClose`: closes one of our listings, or passes any other handle on.
///
/// # Safety
/// Called by the program with the arguments of `FindClose`.
pub(crate) unsafe extern "system" fn find_close(handle: *mut c_void) -> i32 {
    if ours(handle) {
        crate::stat_lock();

        if SEARCHES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(handle as usize))
            .is_some()
        {
            trace(|| format!("result FindClose listed handle={handle:?}"));
            return 1;
        }
    }
    // Keep the guard across the close too: a real find handle may close an NT directory handle internally.
    let guard = Guard::enter();
    let original = original!(find_close, {
        fail(ERROR_INVALID_FUNCTION);
        0
    });
    let result = unsafe { original.call(handle) };
    let _last_error = PreserveLastError::capture();
    drop(guard);
    trace(|| format!("result FindClose handle={handle:?} -> {result}"));
    result
}

/// Starts a listing for a pattern such as `C:\Game\data\*`, when the folder is inside the game folder. The first entry
/// goes into `data`. `None` means the pattern is outside the game folder or all listing slots are live, and the
/// original call runs.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 pattern, and `data` must be writable.
unsafe fn start_search(
    name: *const u16,
    data: *mut WIN32_FIND_DATAW,
    level: i32,
    search: i32,
    flags: u32,
) -> Option<*mut c_void> {
    // SAFETY: the callers pass the program's NUL-terminated pattern; `place` reads it and copies what it keeps.
    let place = unsafe { place(name) }?;
    let mount = place.mount;
    let pattern = place.rel;
    let (rel, mask) = pattern.rsplit_once('\\').unwrap_or(("", pattern.as_str()));
    // A wildcard in a directory component spans several folders. Probing one of them would report a path-not-found
    // for the literal `data*`; the original call expands the pattern across every matching folder.
    if folder_has_wildcard(rel) {
        return None;
    }
    let (own, attributes) = match shown(mount, rel) {
        Ok(Some((_, path, attributes))) if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 => {
            (path, attributes)
        }
        // The folder is missing or hidden: the program gets the error Windows gives for a missing folder.
        Ok(_) => return Some(no_entries(ERROR_PATH_NOT_FOUND)),
        // A refused attribute query is reported as Windows reports it, not as a missing folder.
        Err(failure) => return Some(no_entries(failure.win32())),
    };
    let layers = find_layers_own(
        mount,
        rel,
        mask,
        level,
        search,
        flags,
        true,
        Some(&own),
        Some(attributes),
    );

    if let Some((handle, first)) = layers.direct {
        // SAFETY: `data` is the program's `WIN32_FIND_DATAW` output, writable for one entry; `first` is plain data.
        unsafe { *data = first };
        return Some(handle);
    }

    let mut listing = Search {
        taken: layers.taken,
        next: 0,
        root: layers.root,
    };

    // SAFETY: `data` is writable for one `WIN32_FIND_DATAW` for the duration of the call; the listing keeps no borrow.
    if !unsafe { listing.advance_into(data) } {
        return Some(no_entries(ERROR_FILE_NOT_FOUND));
    }

    crate::stat_lock();
    let mut searches = SEARCHES.lock().unwrap_or_else(PoisonError::into_inner);

    // After a full wrap a key can still be live in the table, so the table is the final authority on collisions.
    let key = loop {
        let key = next_search_key();
        if !searches.contains_key(&key) {
            break key;
        }
    };

    // A live search is never evicted: when the table is full, leave it to the original call rather than resetting an
    // enumeration the program is still reading.
    if searches.len() >= MAX_SEARCHES {
        return None;
    }

    searches.insert(key, Arc::new(Mutex::new(listing)));
    Some(key as *mut c_void)
}

/// The failure of a listing: the error is set, and the handle is the invalid one.
fn no_entries(code: u32) -> *mut c_void {
    set_error(code);
    INVALID_HANDLE_VALUE
}

/// The next entry of one of our listings: `Some(1)` with the entry, `Some(0)` at the end, `None` for any other handle.
///
/// # Safety
/// `data` must be writable.
unsafe fn next_entry(handle: *mut c_void, data: *mut WIN32_FIND_DATAW) -> Option<i32> {
    crate::stat_lock();
    let search = SEARCHES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(handle as usize))
        .cloned()?;
    let mut search = search.lock().unwrap_or_else(PoisonError::into_inner);

    if unsafe { search.advance_into(data) } {
        Some(1)
    } else {
        set_error(ERROR_NO_MORE_FILES);
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::GetLastError;

    #[test]
    fn the_helper_uses_the_started_handle_or_the_original_result() {
        let started = std::ptr::dangling_mut::<u16>().cast::<c_void>();
        let original = std::ptr::dangling_mut::<u32>().cast::<c_void>();

        // A merged listing wins and never reaches the original call.
        let handle = unsafe {
            find_first_with(
                "FindFirstFileW",
                std::ptr::null(),
                || Some(started),
                || panic!("the original must not run"),
            )
        };
        assert_eq!(handle, started);

        // With no listing the original runs, and its error survives the capture and the result trace.
        // SAFETY: the test owns this thread's error.
        set_error(1111);
        let handle = unsafe {
            find_first_with(
                "FindFirstFileW",
                std::ptr::null(),
                || None,
                || {
                    // SAFETY: same as above.
                    set_error(4321);
                    Ok(original)
                },
            )
        };
        // SAFETY: same as above.
        let error = unsafe { GetLastError() };
        assert_eq!(handle, original);
        assert_eq!(error, 4321);

        // A fallback from the original closure is passed through without a result trace.
        let handle = unsafe {
            find_first_with(
                "FindFirstFileW",
                std::ptr::null(),
                || None,
                || Err(INVALID_HANDLE_VALUE),
            )
        };
        assert_eq!(handle, INVALID_HANDLE_VALUE);
    }

    #[test]
    fn only_an_unknown_level_search_or_flag_goes_to_the_original() {
        assert!(supported_ex(
            FindExInfoStandard,
            FindExSearchNameMatch,
            std::ptr::null(),
            0
        ));
        assert!(supported_ex(
            FindExInfoBasic,
            FindExSearchLimitToDirectories,
            std::ptr::null(),
            FIND_FIRST_EX_CASE_SENSITIVE | FIND_FIRST_EX_LARGE_FETCH
        ));
        assert!(!supported_ex(2, FindExSearchNameMatch, std::ptr::null(), 0));
        assert!(!supported_ex(FindExInfoStandard, 2, std::ptr::null(), 0));
        assert!(!supported_ex(
            FindExInfoStandard,
            FindExSearchNameMatch,
            std::ptr::null(),
            0x100
        ));
        assert!(!supported_ex(
            FindExInfoStandard,
            FindExSearchNameMatch,
            &1u8 as *const u8 as *const c_void,
            0
        ));
    }

    #[test]
    fn our_listing_handles_are_separate_from_real_handles() {
        assert!(ours(SEARCH_BASE as *mut c_void));
        assert!(ours((SEARCH_BASE + 2) as *mut c_void));
        assert!(!ours(4 as *mut c_void));
        assert!(!ours(INVALID_HANDLE_VALUE));
    }

    #[test]
    fn a_wildcard_folder_goes_to_the_original_call() {
        assert!(folder_has_wildcard("archive*"));
        assert!(folder_has_wildcard("data?\\nested"));
        assert!(!folder_has_wildcard("archive\\nested"));
        assert!(!folder_has_wildcard(""));
        // The leaf of a pattern is the caller's mask, not a folder: only the part before the last separator is checked.
    }
}
