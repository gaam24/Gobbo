//! `NtQueryDirectoryFile`: a directory listing through an open handle, merged from the three layers. The caller gets the
//! same `FILE_*_INFORMATION` structures it would get from the original call for the six classes the formatter knows
//! (`DIRECTORY`, `FULL`, `BOTH`, `NAMES`, `ID_FULL`, `ID_BOTH`); an unknown class is passed to the original call, one
//! entry at a time, with the pattern and the restart flag applied.
//!
//! The listing is a snapshot, as a real directory query is: the first call with `RestartScan` builds it, and the next
//! calls continue from the entry after the last one returned. The state is keyed by the handle and its game-relative
//! folder; a handle reused for another folder rebuilds the listing.

use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use gobbo_overlay_core::nt_listing::{self, Entry, Fill};
use gobbo_overlay_core::{Layer, Whiteouts};
use gobbo_overlay_decisions::{Kind, Layer as DecisionLayer};
use windows_sys::Win32::Foundation::{
    STATUS_BUFFER_OVERFLOW, STATUS_INVALID_INFO_CLASS, STATUS_INVALID_PARAMETER,
    STATUS_NO_MORE_FILES, STATUS_OBJECT_PATH_NOT_FOUND, STATUS_SUCCESS,
};
use windows_sys::Win32::Storage::FileSystem::{
    FindExInfoStandard, FindExSearchNameMatch, WIN32_FIND_DATAW,
};

use super::names::name_units;
use crate::diag::record_decision;
use crate::guard::Guard;
use crate::handles;
use crate::paths::shown;
use crate::search::{ListingLayers, find_layers, has_entry};
use crate::state::Mount;

use crate::types::UnicodeString;

/// How many listings are kept. A program that opens many folders in a row must not grow the table without a bound.
const MAX_LISTINGS: usize = 128;
/// How many entries are pulled from the stream for one response at least. The caller's buffer usually takes fewer; the
/// rest stay in this small batch for the next call.
const BATCH: usize = 64;
/// The most entries one response pulls, so a large caller's buffer does not grow a listing without a bound.
const MAX_BATCH: usize = 128;

/// One listing a program is reading. The upper and staging entries are held (a mod folder is small), and the root layer
/// is streamed: memory follows the mod layers, not the size of the game folder.
struct Listing {
    mount: &'static Mount,
    /// The game-relative folder and the actual final path of the handle. The path is compared as UTF-16 on every call:
    /// a handle reused for another folder must rebuild the listing, and a matching one needs no `String`.
    rel: String,
    raw: Vec<u16>,
    /// The name filter the listing was built with. A query on the same handle with another name needs a new snapshot:
    /// the entries and the cursor below belong to this filter.
    mask: String,
    /// The whiteout snapshot the listing was built from. A newer snapshot falls back to the visibility check.
    whiteouts: Arc<Whiteouts>,
    layers: ListingLayers,
    /// The next entry of `layers.taken` to put into `batch`.
    next: usize,
    batch: Vec<WIN32_FIND_DATAW>,
    batch_pos: usize,
    used: u64,
}

impl Listing {
    /// Whether this listing belongs to the handle's current final path.
    fn matches(&self, raw: &[u16]) -> bool {
        self.raw.as_slice() == raw
    }

    /// The next entries of the stream, up to `want`. The `taken` entries (dots, upper, staging) come first, then the
    /// root layer one entry at a time.
    fn fill_batch(&mut self, want: usize) {
        self.batch.clear();

        while self.batch.len() < want {
            if self.next < self.layers.taken.entries.len() {
                self.batch.push(self.layers.taken.entries[self.next]);
                self.next += 1;
                continue;
            }

            let ListingLayers { taken, root, .. } = &mut self.layers;
            let Some(root) = root.as_mut() else {
                break;
            };
            let Some(data) = root.next_visible(taken) else {
                break;
            };
            self.batch.push(data);
        }

        self.batch_pos = 0;
    }

    /// Answers one directory query from the batch, pulling a new batch when the old one is consumed. The stream cursor
    /// advances when an entry enters the batch, so an overflow (nothing consumed) keeps the same entries for the retry.
    fn respond(
        &mut self,
        class: u32,
        info: *mut c_void,
        length: u32,
        single: bool,
    ) -> (i32, usize, usize) {
        if self.batch_pos >= self.batch.len() {
            // A `FILE_NAMES_INFORMATION` entry is at least 12 bytes plus its name; 48 is a conservative average. A
            // larger caller buffer then takes fewer calls, while the batch stays bounded.
            let want = if single {
                1
            } else {
                (length as usize / 48).clamp(BATCH, MAX_BATCH)
            };
            self.fill_batch(want);
        }

        let (status, bytes, consumed) =
            respond(class, &self.batch, self.batch_pos, info, length, single);
        if status == STATUS_SUCCESS {
            self.batch_pos += consumed;
        }

        (status, bytes, consumed)
    }
}

/// Whether the whiteout snapshot a listing was built from is still the mount's current one. A change replaces the
/// mount's `Arc`, so a pointer mismatch means the entries were built against the old visibility and must be rebuilt.
fn snapshot_fresh(listing: &Listing) -> bool {
    Arc::ptr_eq(&listing.whiteouts, &listing.mount.whiteouts_snapshot())
}

/// Whether a cached merged listing can keep answering this query: the same folder of the same mount, built with the
/// caller's name filter against the mount's current whiteout snapshot.
fn serves_cached(listing: &Listing, mount: &Mount, rel: &str, mask: &str) -> bool {
    listing.rel == rel
        && listing.mask == mask
        && std::ptr::eq(listing.mount, mount)
        && snapshot_fresh(listing)
}

/// One handle in the listing table: a plain folder the original NT query serves, or a merged snapshot.
enum TableEntry {
    Plain { raw: Vec<u16>, used: u64 },
    // The merged listing is large; a box keeps the table entry small.
    Merged(Box<Listing>),
}

impl TableEntry {
    fn used(&self) -> u64 {
        match self {
            Self::Plain { used, .. } => *used,
            Self::Merged(listing) => listing.used,
        }
    }
}

static LISTINGS: LazyLock<Mutex<FxHashMap<usize, TableEntry>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));
static USED: AtomicU64 = AtomicU64::new(0);

/// `NtQueryDirectoryFile`: a listing of a folder of the game, merged from the layers.
///
/// # Safety
/// Called by the program with the arguments of `NtQueryDirectoryFile`.
// The NT signature has eleven arguments; grouping them into a struct would copy the whole call at every listing.
#[allow(
    clippy::too_many_arguments,
    reason = "the NT query prototype takes these arguments"
)]
pub(crate) unsafe extern "system" fn nt_query_directory_file(
    handle: *mut c_void,
    event: *mut c_void,
    apc: *const c_void,
    apc_context: *const c_void,
    iosb: *mut c_void,
    info: *mut c_void,
    length: u32,
    class: u32,
    single: u8,
    name: *const UnicodeString,
    restart: u8,
) -> i32 {
    unsafe {
        query_or_original(
            "NtQueryDirectoryFile",
            query(
                handle,
                event,
                apc,
                apc_context,
                iosb,
                info,
                length,
                class,
                single,
                name,
                restart,
            ),
            || {
                let original = original!(nt_query_directory_file, STATUS_INVALID_PARAMETER);
                original.call(
                    handle,
                    event,
                    apc,
                    apc_context,
                    iosb,
                    info,
                    length,
                    class,
                    single,
                    name,
                    restart,
                )
            },
        )
    }
}

/// `NtQueryDirectoryFileEx`: the same merged listing, with `SL_RESTART_SCAN` and `SL_RETURN_SINGLE_ENTRY` in `flags`.
///
/// # Safety
/// Called by the program with the arguments of `NtQueryDirectoryFileEx`.
// The NT signature has ten arguments; grouping them into a struct would copy the whole call at every listing.
#[allow(
    clippy::too_many_arguments,
    reason = "the NT query prototype takes these arguments"
)]
pub(crate) unsafe extern "system" fn nt_query_directory_file_ex(
    handle: *mut c_void,
    event: *mut c_void,
    apc: *const c_void,
    apc_context: *const c_void,
    iosb: *mut c_void,
    info: *mut c_void,
    length: u32,
    class: u32,
    flags: u32,
    name: *const UnicodeString,
) -> i32 {
    // `SL_INDEX_SPECIFIED` and an unknown flag change the cursor in a way the merged snapshot cannot follow.
    let query = match ex_switches(flags) {
        Some((single, restart)) => unsafe {
            query(
                handle,
                event,
                apc,
                apc_context,
                iosb,
                info,
                length,
                class,
                single,
                name,
                restart,
            )
        },
        None => None,
    };

    unsafe {
        query_or_original("NtQueryDirectoryFileEx", query, || {
            let original = original!(nt_query_directory_file_ex, STATUS_INVALID_PARAMETER);
            original.call(
                handle,
                event,
                apc,
                apc_context,
                iosb,
                info,
                length,
                class,
                flags,
                name,
            )
        })
    }
}

/// The single-entry and restart switches of `NtQueryDirectoryFileEx`, or `None` for a flag the merged listing does not
/// implement.
fn ex_switches(flags: u32) -> Option<(u8, u8)> {
    const SL_RESTART_SCAN: u32 = 0x01;
    const SL_RETURN_SINGLE_ENTRY: u32 = 0x02;
    const KNOWN: u32 = SL_RESTART_SCAN | SL_RETURN_SINGLE_ENTRY;
    (flags & !KNOWN == 0).then(|| {
        (
            u8::from(flags & SL_RETURN_SINGLE_ENTRY != 0),
            u8::from(flags & SL_RESTART_SCAN != 0),
        )
    })
}

/// The answer of a directory query: the merged listing when `query` answered, otherwise the original call.
///
/// # Safety
/// The closure must call the original function with the arguments of the hook.
unsafe fn query_or_original(
    operation: &'static str,
    query: Option<i32>,
    original: impl FnOnce() -> i32,
) -> i32 {
    if let Some(status) = query {
        crate::trace::trace_status(operation, status);
        return status;
    }

    let status = original();
    let _last_error = crate::win32::PreserveLastError::capture();
    crate::trace::trace_status(operation, status);
    status
}

/// The merged listing, when the handle is a directory of the game and the arguments are synchronous and supported.
/// `None` passes the call to the original.
///
/// # Safety
/// The pointers must be the ones of `NtQueryDirectoryFile`.
// The NT signature has eleven arguments; grouping them into a struct would copy the whole call at every listing.
#[allow(
    clippy::too_many_arguments,
    reason = "the NT query prototype takes these arguments"
)]
unsafe fn query(
    handle: *mut c_void,
    event: *mut c_void,
    apc: *const c_void,
    _apc_context: *const c_void,
    iosb: *mut c_void,
    info: *mut c_void,
    length: u32,
    class: u32,
    single: u8,
    name: *const UnicodeString,
    restart: u8,
) -> Option<i32> {
    // An asynchronous query needs the event or the APC to be signalled by the original call; its entries cannot be
    // served from the merged snapshot.
    if !event.is_null() || !apc.is_null() || iosb.is_null() || info.is_null() {
        return None;
    }

    if !nt_listing::knows(class) {
        return None;
    }
    // Our own `FindFirstFileExW` calls reach here; they are already inside a hooked call and must use the original.
    let _guard = Guard::enter()?;
    let stamp = USED.fetch_add(1, Ordering::Relaxed);
    let mut raw_buffer = [0u16; 512];
    let raw_length = handles::final_path_units(handle, &mut raw_buffer);
    // A handle outside every mount passes straight through. Classify it before taking the listing lock, so the
    // pass-through path keeps its zero-lock budget.
    let raw_kind = raw_length.and_then(|length| handles::layer_kind_units(&raw_buffer[..length]));

    if raw_kind == Some(handles::LayerKind::Outside) {
        return None;
    }

    // A cached listing was built for one name filter; a query with another name must not continue its cursor, so the
    // filter is read before the cached-listing lookup.
    let mask = mask(name)?;

    if let Some(path_length) = raw_length
        && restart == 0
    {
        let raw = &raw_buffer[..path_length];
        // The handle still names the folder the listing was built from: continue the snapshot without resolving the
        // path to a `String` or probing the layers again. A real directory query is a snapshot too; the visibility
        // check belongs to the build, and a handle reused for another folder has another final path and rebuilds.
        crate::stat_lock();
        let mut listings = LISTINGS.lock().unwrap_or_else(PoisonError::into_inner);

        if let Some(entry) = listings.get_mut(&(handle as usize)) {
            match entry {
                TableEntry::Plain { raw: stored, used } if stored.as_slice() == raw => {
                    *used = stamp;
                    return None;
                }

                TableEntry::Merged(listing)
                    if listing.matches(raw) && listing.mask == mask && snapshot_fresh(listing) =>
                {
                    listing.used = stamp;
                    let (status, bytes, _) = listing.respond(class, info, length, single != 0);
                    write_iosb(iosb, status, bytes);
                    return Some(status);
                }
                _ => {}
            }
        }
    }

    let (mount, rel) = match raw_length {
        Some(path_length) => {
            let real = String::from_utf16_lossy(&raw_buffer[..path_length]);
            handles::relative_in_layers(&real)?
        }
        None => {
            let (kind, real) = handles::layer_path_kind(handle)?;

            if kind == handles::LayerKind::Outside {
                return None;
            }

            handles::relative_in_layers(&real)?
        }
    };
    let (own_layer, own) = match shown(mount, &rel) {
        // A handle to a file is not a directory; the original call reports `STATUS_NOT_A_DIRECTORY`.
        Ok(Some((layer, path, _))) if path.is_dir() => (layer, path),
        Ok(Some(_)) => return None,
        // The folder is hidden by a whiteout or missing: the handle must not list a lower layer through it.
        Ok(None) => {
            write_iosb(iosb, STATUS_OBJECT_PATH_NOT_FOUND, 0);
            return Some(STATUS_OBJECT_PATH_NOT_FOUND);
        }
        Err(failure) => {
            let status = failure.status();
            write_iosb(iosb, status, 0);
            return Some(status);
        }
    };
    // A redirected open sits on the top layer, so enumerating that handle can never show a name that lives only in
    // the root layer: a single-name query would answer "not found". Only a handle on the root layer itself names the
    // real folder and may pass through to the original.
    if own_layer == Layer::Root && plain_folder(mount, &rel, mask.as_ref()) {
        // The merged view is the real game folder; remember the handle so continuation calls pass through without
        // probing the layers again.
        crate::stat_lock();
        let mut listings = LISTINGS.lock().unwrap_or_else(PoisonError::into_inner);

        // At the cap, a new handle passes to the original instead of evicting a live enumeration.
        if listings.len() >= MAX_LISTINGS && !listings.contains_key(&(handle as usize)) {
            return None;
        }

        listings.insert(
            handle as usize,
            TableEntry::Plain {
                raw: raw_length.map_or_else(Vec::new, |length| raw_buffer[..length].to_vec()),
                used: stamp,
            },
        );
        return None;
    }

    crate::trace::call_with("NtQueryDirectoryFile", || rel.clone());

    crate::stat_lock();
    let mut listings = LISTINGS.lock().unwrap_or_else(PoisonError::into_inner);
    let rebuild = restart != 0
        || match listings.get(&(handle as usize)) {
            None => true,
            Some(TableEntry::Merged(listing)) => !serves_cached(listing, mount, &rel, &mask),
            Some(TableEntry::Plain { raw, .. }) => {
                raw_length.is_none_or(|length| raw.as_slice() != &raw_buffer[..length])
            }
        };
    if rebuild {
        drop(listings);
        let layers = find_layers(
            mount,
            &rel,
            &mask,
            FindExInfoStandard,
            FindExSearchNameMatch,
            0,
            false,
            Some(&own),
        );
        record_decision(Kind::List, DecisionLayer::None, 0, || {
            mount.decision_path(&rel)
        });
        crate::stat_lock();
        listings = LISTINGS.lock().unwrap_or_else(PoisonError::into_inner);

        // At the cap, a new handle passes to the original instead of evicting a live enumeration. A handle already in
        // the table only replaces its own entry, so its restart or changed path still rebuilds as before.
        if listings.len() >= MAX_LISTINGS && !listings.contains_key(&(handle as usize)) {
            return None;
        }

        listings.insert(
            handle as usize,
            TableEntry::Merged(Box::new(Listing {
                mount,
                rel: rel.clone(),
                raw: raw_length.map_or_else(Vec::new, |length| raw_buffer[..length].to_vec()),
                mask: mask.as_ref().to_string(),
                whiteouts: mount.whiteouts_snapshot(),
                layers,
                next: 0,
                batch: Vec::with_capacity(BATCH),
                batch_pos: 0,
                used: stamp,
            })),
        );
    }

    match listings.get_mut(&(handle as usize))? {
        TableEntry::Merged(listing) => {
            listing.used = stamp;
            let (status, bytes, _) = listing.respond(class, info, length, single != 0);
            write_iosb(iosb, status, bytes);
            Some(status)
        }
        TableEntry::Plain { used, .. } => {
            *used = stamp;
            None
        }
    }
}

/// Whether the folder's merged view is exactly the real game folder: no whiteout affects this folder's own listing, and
/// neither the upper nor the staging layer has a non-dot entry matching the caller's pattern. The original NT query can
/// then serve the program unchanged, with its own cursor and flags.
fn plain_folder(mount: &Mount, rel: &str, mask: &str) -> bool {
    if mount.whiteouts_snapshot().affects_listing(rel) {
        return false;
    }

    !has_entry(
        mount,
        Layer::Upper,
        rel,
        mask,
        FindExInfoStandard,
        FindExSearchNameMatch,
        0,
    ) && !has_entry(
        mount,
        Layer::Staging,
        rel,
        mask,
        FindExInfoStandard,
        FindExSearchNameMatch,
        0,
    )
}

/// The pattern of the call, or `*` when it has none. A pattern with a separator is refused: NtQueryDirectoryFile takes a
/// name relative to the directory, not a path. The common `None`/empty case borrows a static string.
///
/// # Safety
/// `name` must be null or a valid `UNICODE_STRING`.
fn mask(name: *const UnicodeString) -> Option<Cow<'static, str>> {
    let Some(name) = (unsafe { name.as_ref() }) else {
        return Some(Cow::Borrowed("*"));
    };
    let units = unsafe { name_units(name) };

    if units.is_empty() {
        return Some(Cow::Borrowed("*"));
    }

    let mask = String::from_utf16_lossy(units);

    if mask.contains(['\\', '/']) {
        return None;
    }

    Some(Cow::Owned(mask))
}

/// Formats up to `length` bytes of entries starting at `start` into `info`. The result is the status, the number of
/// bytes written, and how many entries were consumed.
///
/// # Safety
/// `info` must be writable for `length` bytes and be the buffer the program passed to `NtQueryDirectoryFile`, valid for
/// the duration of that call. The caller's NT buffer contract also covers the `IO_STATUS_BLOCK` written by `query`.
fn respond(
    class: u32,
    entries: &[WIN32_FIND_DATAW],
    start: usize,
    info: *mut c_void,
    length: u32,
    single: bool,
) -> (i32, usize, usize) {
    if start >= entries.len() {
        return (STATUS_NO_MORE_FILES, 0, 0);
    }

    if !nt_listing::knows(class) {
        return (STATUS_INVALID_INFO_CLASS, 0, 0);
    }

    // A `usize` that cannot hold the byte count is an impossible buffer for this process; refuse it instead of
    // truncating a valid listing into a smaller slice.
    let Ok(length) = usize::try_from(length) else {
        return (STATUS_INVALID_PARAMETER, 0, 0);
    };
    // SAFETY: the caller guarantees `info` points to `length` writable bytes, and `length` is now the exact byte count
    // the program passed. The slice lives only until `nt_listing::fill` returns.
    let buffer = unsafe { std::slice::from_raw_parts_mut(info.cast::<u8>(), length) };

    match nt_listing::fill(
        class,
        entries.len(),
        |index| dir_entry(&entries[index]),
        start,
        buffer,
        single,
    ) {
        Fill::UnknownClass => (STATUS_INVALID_INFO_CLASS, 0, 0),
        Fill::End => (STATUS_NO_MORE_FILES, 0, 0),
        Fill::Overflow => (STATUS_BUFFER_OVERFLOW, 0, 0),
        Fill::Written { bytes, consumed } => (STATUS_SUCCESS, bytes, consumed),
    }
}

/// The fields of one `WIN32_FIND_DATAW` as the formatter takes them: each name runs to its terminator.
fn dir_entry(data: &WIN32_FIND_DATAW) -> Entry<'_> {
    Entry {
        name: crate::win32::find_data_name(data),
        short_name: &data.cAlternateFileName[..crate::text::nul_length(&data.cAlternateFileName)],
        attributes: data.dwFileAttributes,
        size: file_size(data),
        created: filetime(data.ftCreationTime),
        accessed: filetime(data.ftLastAccessTime),
        written: filetime(data.ftLastWriteTime),
    }
}

/// A `FILETIME` as the 64-bit value the NT structures carry.
fn filetime(time: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    u64::from(time.dwLowDateTime) | (u64::from(time.dwHighDateTime) << 32)
}

/// The size of a file. `WIN32_FIND_DATAW` keeps it in two halves.
fn file_size(data: &WIN32_FIND_DATAW) -> u64 {
    u64::from(data.nFileSizeLow) | (u64::from(data.nFileSizeHigh) << 32)
}

/// The `IO_STATUS_BLOCK` layout: the status, then a pointer-sized information field.
#[repr(C)]
struct IoStatusBlock {
    status: i32,
    information: usize,
}

fn write_iosb(iosb: *mut c_void, status: i32, information: usize) {
    // SAFETY: `iosb` was checked non-null and is the caller's `IO_STATUS_BLOCK`. The unaligned write keeps the hook
    // safe even for a caller that passes a buffer with weaker alignment than the structure asks for.
    unsafe {
        iosb.cast::<IoStatusBlock>().write_unaligned(IoStatusBlock {
            status,
            information,
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, attributes: u32) -> WIN32_FIND_DATAW {
        // SAFETY: `WIN32_FIND_DATAW` is plain data; all zeros is a valid value.
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        data.dwFileAttributes = attributes;
        data.nFileSizeLow = 7;

        for (slot, unit) in data.cFileName.iter_mut().zip(name.encode_utf16()) {
            *slot = unit;
        }

        data
    }

    #[test]
    fn the_ex_switches_map_and_an_unknown_flag_is_passed_through() {
        assert_eq!(ex_switches(0), Some((0, 0)));
        assert_eq!(ex_switches(0x1), Some((0, 1)));
        assert_eq!(ex_switches(0x2), Some((1, 0)));
        assert_eq!(ex_switches(0x3), Some((1, 1)));
        assert_eq!(ex_switches(0x4), None);
        assert_eq!(ex_switches(0x10), None);
    }

    #[test]
    fn a_buffer_too_small_for_one_entry_overflows_and_consumes_nothing() {
        let entries = [entry("a", 0)];
        let mut buffer = [0u8; 32];
        let (status, bytes, consumed) = respond(
            nt_listing::DIRECTORY,
            &entries,
            0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            false,
        );
        assert_eq!(status, STATUS_BUFFER_OVERFLOW);
        assert_eq!(bytes, 0);
        assert_eq!(consumed, 0);
    }

    /// The hook-specific part of the end case: `Fill::End` becomes `STATUS_NO_MORE_FILES` and the cursor is not
    /// advanced. The core test only pins the bare marker and the unknown-class dispatch.
    #[test]
    fn an_exhausted_listing_maps_the_end_to_no_more_files_without_consuming() {
        let entries = [entry("a", 0)];
        let mut buffer = [0u8; 512];
        let (status, bytes, consumed) = respond(
            nt_listing::DIRECTORY,
            &entries,
            1,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            false,
        );
        assert_eq!(status, STATUS_NO_MORE_FILES);
        assert_eq!(bytes, 0);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn a_changed_whiteout_snapshot_stops_a_listing_from_serving_a_cached_query() {
        let layers = gobbo_overlay_core::Layers::new(
            std::path::PathBuf::from("game"),
            None,
            std::path::PathBuf::from("upper"),
        );
        let mount: &'static Mount = Box::leak(Box::new(Mount::new(
            "game".to_string(),
            std::path::PathBuf::from("game"),
            layers,
            Whiteouts::default(),
        )));
        let rel = "folder".to_string();
        let snapshot = mount.whiteouts_snapshot();
        let listing = |whiteouts: Arc<Whiteouts>| Listing {
            mount,
            rel: rel.clone(),
            raw: Vec::new(),
            mask: "*".to_string(),
            whiteouts,
            layers: ListingLayers {
                taken: crate::search::Taken::leading(Vec::new()),
                root: None,
                direct: None,
            },
            next: 0,
            batch: Vec::new(),
            batch_pos: 0,
            used: 0,
        };

        assert!(serves_cached(
            &listing(Arc::clone(&snapshot)),
            mount,
            &rel,
            "*"
        ));
        assert!(!serves_cached(
            &listing(Arc::new(Whiteouts::default())),
            mount,
            &rel,
            "*"
        ));
        assert!(!serves_cached(
            &listing(Arc::clone(&snapshot)),
            mount,
            "other",
            "*"
        ));
        assert!(!serves_cached(
            &listing(Arc::clone(&snapshot)),
            mount,
            &rel,
            "name.txt"
        ));
    }
}
