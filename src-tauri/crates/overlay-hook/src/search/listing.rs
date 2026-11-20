//! The entries of a folder as the program sees them: the three layers merged, without the lower entries that whiteouts hide,
//! and each entry in the `WIN32_FIND_DATAW` form that a listing returns.

use gobbo_overlay_core::{
    Layer, Whiteouts, fold_unit, is_copy_temp_name, join, wildcard_match, wildcard_match_units,
};
use rustc_hash::FxHashMap;
use std::ffi::c_void;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FIND_FIRST_EX_CASE_SENSITIVE, FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY,
    FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, FindClose, FindExInfoBasic,
    FindExInfoStandard, FindExSearchLimitToDirectories, FindFirstFileExW, FindNextFileW,
    GetFileInformationByHandleEx, OPEN_EXISTING, WIN32_FIND_DATAW,
};

use crate::paths::{find_exact, visible, with_pattern};
use crate::state::Mount;
use crate::text::{same_folded_wide, wide};
use crate::win32::PreserveLastError;

/// Whether the folder lists at least one entry the program can see. It stops at the first entry, so a caller that only
/// asks whether the folder is empty does not build a whole listing.
pub(crate) fn has_visible_entries(mount: &Mount, rel: &str) -> bool {
    if any_entry(&join(&mount.upper, rel)) {
        return true;
    }

    let whiteouts = mount.whiteouts_snapshot();

    if whiteouts.is_empty() {
        // Without whiteouts every entry of every lower layer is visible; there is no name to inspect.
        return mount
            .layer_paths()
            .filter(|(layer, _)| *layer != Layer::Upper)
            .any(|(_, base)| any_entry(&join(base, rel)));
    }

    if whiteouts.hides(rel) {
        return false;
    }

    mount
        .layer_paths()
        .filter(|(layer, _)| *layer != Layer::Upper)
        .any(|(_, base)| {
            let base = join(base, rel);
            fs::read_dir(base).is_ok_and(|entries| {
                entries.filter_map(Result::ok).any(|entry| {
                    let name = entry.file_name();
                    let mut units = [0u16; 260];
                    let mut length = 0;

                    for unit in name.encode_wide() {
                        if length == units.len() {
                            break;
                        }

                        units[length] = unit;
                        length += 1;
                    }

                    !whiteouts.hides_child_units_in_visible_folder(rel, &units[..length])
                })
            })
        })
}

/// Whether one folder has an entry at all, stopping at the first one. A copy-up temporary file is not an entry.
fn any_entry(folder: &Path) -> bool {
    crate::stat_io();
    fs::read_dir(folder).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| !entry.file_name().to_str().is_some_and(is_copy_temp_name))
    })
}

/// The first entry of `base\rel\mask` in one layer. The caller closes the handle.
fn find_first(
    base: &Path,
    rel: &str,
    mask: &str,
    level: i32,
    search: i32,
    flags: u32,
) -> (HANDLE, WIN32_FIND_DATAW) {
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    crate::stat_io();
    let handle = with_pattern(base, rel, mask, |pattern| {
        // SAFETY: the pattern is NUL-terminated, and `data` is writable for the requested information level.
        unsafe {
            FindFirstFileExW(
                pattern,
                level,
                (&raw mut data).cast::<c_void>(),
                search,
                null(),
                flags,
            )
        }
    });
    (handle, data)
}

/// Whether one layer has at least one entry matching the caller's pattern. The handle is closed again; the caller only
/// needs the answer (for example, to know that a folder has no upper or staging entry).
pub(crate) fn has_entry(
    mount: &Mount,
    layer: Layer,
    rel: &str,
    mask: &str,
    level: i32,
    search: i32,
    flags: u32,
) -> bool {
    let Some((_, base)) = mount.layer_paths().find(|(found, _)| *found == layer) else {
        return false;
    };
    let (handle, mut data) = find_first(base, rel, mask, level, search, flags);

    if handle == INVALID_HANDLE_VALUE {
        return false;
    }

    loop {
        if !is_dot_name(crate::win32::find_data_name(&data)) {
            // SAFETY: the handle came from `find_first` and is closed once.
            unsafe { FindClose(handle) };
            return true;
        }
        // SAFETY: the handle is open and `data` is writable.
        if unsafe { FindNextFileW(handle, &raw mut data) } == 0 {
            break;
        }
    }
    // SAFETY: the handle came from `find_first` and is closed once.
    unsafe { FindClose(handle) };
    false
}

/// Whether a name is a copy-up temporary file. The ASCII prefix is checked in UTF-16 first, so a dot name such as
/// `.gitignore` does not allocate a `String` just to be told it is not a copy.
fn is_copy_temp(name: &[u16]) -> bool {
    const PREFIX: &[u8] = b".gobbo-copy-";
    name.len() > PREFIX.len()
        && name
            .iter()
            .zip(PREFIX)
            .all(|(unit, byte)| *unit == u16::from(*byte))
        && is_copy_temp_name(&String::from_utf16_lossy(name))
}

/// The entries the upper and staging layers show, in the order a listing returns them, with an index by folded name. The
/// root layer is listed as a stream, and a root entry that one of these layers shows is not listed again.
pub(crate) struct Taken {
    pub(crate) entries: Vec<WIN32_FIND_DATAW>,
    /// The layer of every entry, parallel to `entries`. The leading `.`/`..` entries have no lower spelling.
    layers: Vec<Layer>,
    /// The position of the first indexed entry with each folded name hash. A hash shared by two names is rare, and the
    /// second and later positions go to `collisions`, so the common entry allocates nothing for its index.
    index: FxHashMap<u64, usize>,
    collisions: Vec<(u64, usize)>,
}

impl Taken {
    /// The entries that come before the layers, such as `.` and `..`. They are not indexed: a root entry is not hidden
    /// by them.
    pub(crate) fn leading(entries: Vec<WIN32_FIND_DATAW>) -> Self {
        let layers = entries.iter().map(|_| Layer::Root).collect();
        Self {
            entries,
            layers,
            index: FxHashMap::default(),
            collisions: Vec::new(),
        }
    }

    /// Adds an entry of the upper or staging layer, and indexes its name.
    fn push_layer(&mut self, data: WIN32_FIND_DATAW, layer: Layer) {
        let hash = name_hash(crate::win32::find_data_name(&data));
        let position = self.entries.len();

        if let Some(previous) = self.index.insert(hash, position) {
            // The new position stays in the index; the older one moves to the collision list, so `find` can still reach
            // it after a hash collision.
            self.collisions.push((hash, previous));
        }

        self.entries.push(data);
        self.layers.push(layer);
    }

    /// The position of an upper or staging entry with this name, ignoring case under the folding rule.
    fn find(&self, name: &[u16]) -> Option<usize> {
        let hash = name_hash(name);
        let same = |position: usize| {
            same_folded_wide(crate::win32::find_data_name(&self.entries[position]), name)
        };

        if let Some(&position) = self.index.get(&hash)
            && same(position)
        {
            return Some(position);
        }

        self.collisions
            .iter()
            .find(|&&(other, position)| other == hash && same(position))
            .map(|&(_, position)| position)
    }

    /// Whether an upper or staging entry has this name, ignoring case under the folding rule.
    fn shows(&self, name: &[u16]) -> bool {
        self.find(name).is_some()
    }
}

/// The batch size of the root spelling pass. One `GetFileInformationByHandleEx` call fills it, so a folder of 1000
/// entries needs about a dozen calls instead of one `FindNextFileW` per entry; `MAX_LISTINGS` bounds the total memory.
const BATCH_BYTES: usize = 8 * 1024;

/// The root layer read in batches with `GetFileInformationByHandleEx`, used by the spelling pass. The information class
/// has no search mask, so the caller's mask is applied in user mode; an ASCII name is matched without a `String`.
struct BatchReader {
    handle: HANDLE,
    /// The batch buffer; `u64` gives it the eight-byte alignment the records require.
    buffer: Vec<u64>,
    offset: usize,
    started: bool,
    done: bool,
    mask: String,
    with_short_names: bool,
}

impl BatchReader {
    /// Opens the folder for a batch pass. `None` when the arguments need the find stream, the path is not ASCII, or the
    /// volume does not answer the batch; the caller then uses `find_first` and `FindNextFileW`.
    fn open(base: &Path, rel: &str, mask: &str, level: i32, flags: u32) -> Option<Self> {
        if !batch_supported(level, flags) {
            return None;
        }
        // A non-ASCII folder or base path measured slower through the batch handle than through the find stream.
        if !rel.is_ascii() || !base.to_str().is_some_and(str::is_ascii) {
            return None;
        }
        // The hook's own open must pass through the file hooks. The caller normally holds the guard; this covers a
        // direct call as well.
        let _guard = crate::guard::Guard::enter();
        let path = wide(join(base, rel).as_os_str());
        crate::stat_io();
        // SAFETY: `path` is NUL-terminated, and the flags open the folder for its directory entries only.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut reader = Self {
            handle,
            buffer: vec![0u64; BATCH_BYTES / 8],
            offset: 0,
            started: false,
            done: false,
            mask: mask.to_string(),
            with_short_names: level == FindExInfoStandard,
        };

        if !reader.fill() {
            // An empty folder or a volume that does not answer the batch: the find stream is the fallback.
            // SAFETY: the handle was opened above and is not stored anywhere else.
            unsafe { CloseHandle(handle) };
            return None;
        }

        Some(reader)
    }

    /// Fills the buffer with the next batch. The first call restarts the query at the beginning of the folder.
    fn fill(&mut self) -> bool {
        if self.done {
            return false;
        }

        let class = if self.started {
            FileIdBothDirectoryInfo
        } else {
            FileIdBothDirectoryRestartInfo
        };
        // The class query reaches `NtQueryDirectoryFile`; the guard keeps that internal call out of the NT listing hook.
        let _guard = crate::guard::Guard::enter();
        crate::stat_io();
        // SAFETY: the buffer is writable for `bytes` bytes, and the handle is an open directory handle.
        let filled = unsafe {
            GetFileInformationByHandleEx(
                self.handle,
                class,
                self.buffer.as_mut_ptr().cast(),
                self.bytes() as u32,
            )
        };
        self.started = true;

        if filled == 0 {
            self.done = true;
            return false;
        }

        self.offset = 0;
        true
    }

    fn bytes(&self) -> usize {
        self.buffer.len() * std::mem::size_of::<u64>()
    }

    /// The next entry of the folder, before the caller's mask is applied.
    fn next_raw(&mut self) -> Option<WIN32_FIND_DATAW> {
        if self.offset >= self.bytes() && !self.fill() {
            return None;
        }
        // SAFETY: the buffer is eight-byte aligned, `offset` is an eight-byte multiple, and the system wrote one
        // `FILE_ID_BOTH_DIR_INFO` there with its name inside the buffer.
        let info = unsafe {
            &*self
                .buffer
                .as_ptr()
                .cast::<u8>()
                .add(self.offset)
                .cast::<FILE_ID_BOTH_DIR_INFO>()
        };
        let name_length = info.FileNameLength as usize / 2;
        let name = unsafe { std::slice::from_raw_parts(info.FileName.as_ptr(), name_length) };
        let next = info.NextEntryOffset as usize;
        self.offset = if next == 0 {
            self.bytes()
        } else {
            self.offset.saturating_add(next)
        };

        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        data.dwFileAttributes = info.FileAttributes;
        data.ftCreationTime = filetime(info.CreationTime as u64);
        data.ftLastAccessTime = filetime(info.LastAccessTime as u64);
        data.ftLastWriteTime = filetime(info.LastWriteTime as u64);
        data.nFileSizeHigh = (info.EndOfFile as u64 >> 32) as u32;
        data.nFileSizeLow = info.EndOfFile as u32;

        for (slot, unit) in data.cFileName.iter_mut().zip(name) {
            *slot = *unit;
        }

        if self.with_short_names {
            let short_length = (info.ShortNameLength.max(0) as usize / 2).min(info.ShortName.len());
            data.cAlternateFileName[..short_length]
                .copy_from_slice(&info.ShortName[..short_length]);
        }

        Some(data)
    }

    /// The next entry that matches the caller's mask.
    fn next(&mut self) -> Option<WIN32_FIND_DATAW> {
        loop {
            let data = self.next_raw()?;

            if wildcard_match_units(&self.mask, crate::win32::find_data_name(&data)) {
                return Some(data);
            }
        }
    }
}

impl Drop for BatchReader {
    fn drop(&mut self) {
        // SAFETY: the handle was opened in `open` and is closed once, here.
        unsafe { CloseHandle(self.handle) };
    }
}

/// Whether the batch can replace the find stream for these arguments. A case-sensitive or on-disk-only search keeps the
/// system stream, because the batch does not implement those semantics.
fn batch_supported(level: i32, flags: u32) -> bool {
    (level == FindExInfoStandard || level == FindExInfoBasic)
        && flags & (FIND_FIRST_EX_CASE_SENSITIVE | FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY) == 0
}

/// The root layer of a folder, read one entry at a time with the system listing. Its entries are not kept: a caller that
/// wants them all asks for each in turn, and a caller that wants the first one takes it and leaves the rest to the
/// listing. The find handle is closed when the stream is dropped.
pub(crate) struct RootStream {
    handle: HANDLE,
    first: Option<WIN32_FIND_DATAW>,
    done: bool,
    rel: String,
    whiteouts: Arc<Whiteouts>,
    directories_only: bool,
}

impl RootStream {
    /// Whether the root entry is one a listing shows, before the upper and staging entries are considered.
    fn accepts(&self, data: &WIN32_FIND_DATAW) -> bool {
        let directory = data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;

        if self.directories_only && !directory {
            return false;
        }

        let name = crate::win32::find_data_name(data);
        !is_dot_name(name)
            && !is_copy_temp(name)
            && (self.whiteouts.is_empty()
                || !self
                    .whiteouts
                    .hides_child_units_in_visible_folder(&self.rel, name))
    }

    /// The next root entry that a listing shows, or `None` at the end. `taken` holds the upper and staging entries.
    pub(crate) fn next_visible(&mut self, taken: &Taken) -> Option<WIN32_FIND_DATAW> {
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

        unsafe { self.next_visible_into(taken, &raw mut data) }.then_some(data)
    }

    /// `next_visible` that writes the accepted entry into `out`, so a root entry goes from `FindNextFileW` to the
    /// program's buffer without an intermediate copy.
    ///
    /// # Safety
    /// `out` must be writable for one `WIN32_FIND_DATAW`.
    pub(crate) unsafe fn next_visible_into(
        &mut self,
        taken: &Taken,
        out: *mut WIN32_FIND_DATAW,
    ) -> bool {
        loop {
            if let Some(first) = self.first.take() {
                unsafe { *out = first };
            } else {
                if self.done {
                    return false;
                }

                crate::stat_io();
                // SAFETY: the handle came from the system listing of this folder and is still open; `out` is writable.
                if unsafe { FindNextFileW(self.handle, out) } == 0 {
                    self.done = true;
                    return false;
                }
            }
            // SAFETY: the branch above wrote one entry into `out`.
            let data = unsafe { &*out };

            if self.accepts(data) && !taken.shows(crate::win32::find_data_name(data)) {
                return true;
            }
        }
    }
}

// SAFETY: the find handle is a kernel object that every thread of the process may use. A `RootStream` is reached only through
// the lock of its listing, so two threads never call on it at once.
unsafe impl Send for RootStream {}

impl Drop for RootStream {
    fn drop(&mut self) {
        // SAFETY: the handle came from `FindFirstFileExW` in `find_layers` and is closed once, here.
        unsafe { FindClose(self.handle) };
    }
}
/// The parts of a listing: the entries of the upper and staging layers, the root layer still to be read, and the real
/// root handle when the merged view is exactly the game folder.
pub(crate) struct Layers {
    pub(crate) taken: Taken,
    pub(crate) root: Option<RootStream>,
    /// The real `FindFirstFileExW` handle and its first entry, when no upper/staging entry and no whiteout can change
    /// the view. The caller returns the handle to the program, so the rest of the listing runs in the system.
    pub(crate) direct: Option<(HANDLE, WIN32_FIND_DATAW)>,
}

// SAFETY: a handle and a find entry are plain values, and a `Layers` is reached only through the lock of its listing.
unsafe impl Send for Layers {}

/// The entries of a game-relative folder for a listing: the upper and staging layers read now, and the root layer as a
/// stream. The layers are merged by the system listing call itself. The pattern filters the names in each layer, so the
/// hook does not walk and match every entry by hand. `allow_direct` lets a caller that can return a real find handle take
/// the game folder unchanged.
#[expect(
    clippy::too_many_arguments,
    reason = "the caller already has the listing arguments; grouping them would add a struct used once"
)]
pub(crate) fn find_layers(
    mount: &Mount,
    rel: &str,
    mask: &str,
    level: i32,
    search: i32,
    flags: u32,
    allow_direct: bool,
    leading: Vec<WIN32_FIND_DATAW>,
) -> Layers {
    // The internal listing calls set their own last error; keep the caller's value for the success path.
    let _last_error = PreserveLastError::capture();
    let whiteouts = mount.whiteouts_snapshot();
    let hidden_folder = whiteouts.hides(rel);
    let directories_only = search == FindExSearchLimitToDirectories;
    let mut taken = Taken::leading(leading);

    for (layer, base) in mount.layer_paths() {
        if layer != Layer::Upper && hidden_folder {
            break;
        }

        if layer == Layer::Root {
            // With no whiteout and no upper/staging entry, the merged view is the game folder itself. Returning the
            // real handle lets the program's FindNextFileW/FindClose calls pass through without a synthetic listing.
            let direct = allow_direct
                && !hidden_folder
                && taken.layers.iter().all(|layer| *layer == Layer::Root)
                && !whiteouts.affects_listing(rel);
            let (handle, data) = find_first(base, rel, mask, level, search, flags);

            if handle == INVALID_HANDLE_VALUE {
                apply_spellings(
                    &mut taken,
                    mount,
                    rel,
                    &whiteouts,
                    hidden_folder,
                    mask,
                    level,
                    search,
                    flags,
                );
                return Layers {
                    taken,
                    root: None,
                    direct: None,
                };
            }

            apply_spellings(
                &mut taken,
                mount,
                rel,
                &whiteouts,
                hidden_folder,
                mask,
                level,
                search,
                flags,
            );

            if direct {
                return Layers {
                    taken,
                    root: None,
                    direct: Some((handle, data)),
                };
            }

            return Layers {
                taken,
                root: Some(RootStream {
                    handle,
                    first: Some(data),
                    done: false,
                    rel: rel.to_string(),
                    whiteouts,
                    directories_only,
                }),
                direct: None,
            };
        }

        let (handle, mut data) = find_first(base, rel, mask, level, search, flags);

        if handle == INVALID_HANDLE_VALUE {
            continue;
        }

        loop {
            let name = crate::win32::find_data_name(&data);
            let directory = data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
            let visible = (!directories_only || directory)
                && !is_dot_name(name)
                && !is_copy_temp(name)
                && (layer == Layer::Upper
                    || whiteouts.is_empty()
                    || !whiteouts.hides_child_units_in_visible_folder(rel, name));
            if visible && !taken.shows(name) {
                taken.push_layer(data, layer);
            }
            // SAFETY: the handle came from the listing above, and `data` is writable.
            crate::stat_io();

            if unsafe { FindNextFileW(handle, &raw mut data) } == 0 {
                break;
            }
        }
        // SAFETY: the handle came from the listing above and is closed once.
        unsafe { FindClose(handle) };
    }

    apply_spellings(
        &mut taken,
        mount,
        rel,
        &whiteouts,
        hidden_folder,
        mask,
        level,
        search,
        flags,
    );
    Layers {
        taken,
        root: None,
        direct: None,
    }
}

/// How many upper/staging entries still take one exact lower-layer search each. An exact search costs tens of
/// microseconds, while one root pass costs a fraction of a microsecond per root entry. `overlay-bench` puts the crossover
/// for a 64-entry folder with 16 overrides and a 1000-entry folder with 10 overrides near this value.
const SPELLING_LOOKUP_LIMIT: usize = 8;

/// Puts the lower layer's spelling on the entries the listing will show. A small set asks the exact name in each lower
/// layer; a large one reads the root folder once and reuses that spelling for every taken name, as Windows keeps the
/// name of an existing file when a file is written over it.
#[expect(
    clippy::too_many_arguments,
    reason = "the caller already has the listing arguments; grouping them would add a struct used once"
)]
fn apply_spellings(
    taken: &mut Taken,
    mount: &Mount,
    rel: &str,
    whiteouts: &Whiteouts,
    hidden_folder: bool,
    mask: &str,
    level: i32,
    search: i32,
    flags: u32,
) {
    if taken.entries.is_empty() || hidden_folder {
        return;
    }

    let count = taken.entries.len();

    if count > SPELLING_LOOKUP_LIMIT {
        // The root pass writes its spelling straight into the entries, so a large merged folder does not keep a second
        // copy of every `WIN32_FIND_DATAW` (about 600 bytes each) just to publish it afterwards.
        let from_root =
            apply_root_spellings(taken, mount, rel, whiteouts, mask, level, search, flags);
        for (index, from_root) in from_root.iter().enumerate() {
            if *from_root || taken.layers[index] != Layer::Upper {
                continue;
            }

            let name = crate::win32::find_data_name(&taken.entries[index]);
            let Some(lower) = staging_spelling(mount, rel, name, whiteouts) else {
                continue;
            };
            taken.entries[index].cFileName = lower.cFileName;
            taken.entries[index].cAlternateFileName = lower.cAlternateFileName;
        }

        return;
    }

    for index in 0..count {
        let layer = taken.layers[index];

        if layer == Layer::Root {
            continue;
        }

        let name = crate::win32::find_data_name(&taken.entries[index]);
        let Some(lower) = lower_spelling(mount, rel, layer, name, whiteouts, hidden_folder) else {
            continue;
        };
        taken.entries[index].cFileName = lower.cFileName;
        taken.entries[index].cAlternateFileName = lower.cAlternateFileName;
    }
}

/// Puts the root folder's spelling on every taken entry whose name it also has, in one pass over that folder. The
/// returned flags say which entries the root had: an entry the root has keeps the root spelling even when a whiteout
/// hides that copy, and only an entry the root lacks asks the staging layer.
#[expect(
    clippy::too_many_arguments,
    reason = "the caller already has the listing arguments; grouping them would add a struct used once"
)]
fn apply_root_spellings(
    taken: &mut Taken,
    mount: &Mount,
    rel: &str,
    whiteouts: &Whiteouts,
    mask: &str,
    level: i32,
    search: i32,
    flags: u32,
) -> Vec<bool> {
    crate::stat_spelling();
    let mut found = vec![false; taken.entries.len()];
    let Some((_, base)) = mount.layer_paths().find(|(layer, _)| *layer == Layer::Root) else {
        return found;
    };
    let mut remaining = taken
        .entries
        .iter()
        .zip(&taken.layers)
        .filter(|(_, layer)| **layer != Layer::Root)
        .count();
    if let Some(mut batch) = BatchReader::open(base, rel, mask, level, flags) {
        while let Some(data) = batch.next() {
            spell_entry(taken, rel, whiteouts, &data, &mut found, &mut remaining);

            if remaining == 0 {
                break;
            }
        }

        return found;
    }

    let (handle, mut data) = find_first(base, rel, mask, level, search, flags);

    if handle == INVALID_HANDLE_VALUE {
        return found;
    }

    loop {
        spell_entry(taken, rel, whiteouts, &data, &mut found, &mut remaining);

        if remaining == 0 {
            break;
        }

        crate::stat_io();
        // SAFETY: the handle came from the listing above, and `data` is writable.
        if unsafe { FindNextFileW(handle, &raw mut data) } == 0 {
            break;
        }
    }
    // SAFETY: the handle came from the listing above and is closed once.
    unsafe { FindClose(handle) };
    found
}

/// Applies one root entry to the spelling pass: marks the taken entry it matches and copies the root spelling when a
/// whiteout does not hide that copy.
fn spell_entry(
    taken: &mut Taken,
    rel: &str,
    whiteouts: &Whiteouts,
    data: &WIN32_FIND_DATAW,
    found: &mut [bool],
    remaining: &mut usize,
) {
    let name = crate::win32::find_data_name(data);

    if let Some(index) = taken.find(name)
        && taken.layers[index] != Layer::Root
        && !found[index]
    {
        found[index] = true;
        *remaining = remaining.saturating_sub(1);
        if !whiteouts.hides_child_units_in_visible_folder(rel, name) {
            taken.entries[index].cFileName = data.cFileName;
            taken.entries[index].cAlternateFileName = data.cAlternateFileName;
        }
    }
}

/// The staging spelling of an upper entry, when the root layer does not have the name.
fn staging_spelling(
    mount: &Mount,
    rel: &str,
    name: &[u16],
    whiteouts: &Whiteouts,
) -> Option<WIN32_FIND_DATAW> {
    let (_, base) = mount
        .layer_paths()
        .find(|(layer, _)| *layer == Layer::Staging)?;
    let data = find_exact(base, rel, name)?;

    if whiteouts.hides_child_units_in_visible_folder(rel, crate::win32::find_data_name(&data)) {
        return None;
    }

    Some(data)
}

/// The entry a lower layer has under the same name as an entry of `layer`, in the order of the spelling rule: the root
/// layer first (the game's names), then the staging layer. `None` for the root layer itself, for a folder the whiteouts
/// hide, and when no lower layer has the name, or the lower entry is hidden by a whiteout.
fn lower_spelling(
    mount: &Mount,
    rel: &str,
    layer: Layer,
    name: &[u16],
    whiteouts: &Whiteouts,
    hidden_folder: bool,
) -> Option<WIN32_FIND_DATAW> {
    let order: &[Layer] = match layer {
        Layer::Upper => &[Layer::Root, Layer::Staging],
        Layer::Staging => &[Layer::Root],
        Layer::Root => return None,
    };

    if hidden_folder {
        return None;
    }

    for wanted in order {
        let Some((_, base)) = mount.layer_paths().find(|(found, _)| found == wanted) else {
            continue;
        };
        let Some(data) = find_exact(base, rel, name) else {
            continue;
        };
        // Both lower layers are hidden by a whiteout alike: a whiteout hides the name in the staging layer too.
        if whiteouts.hides_child_units_in_visible_folder(rel, crate::win32::find_data_name(&data)) {
            continue;
        }

        return Some(data);
    }

    None
}

/// The `.` and `..` entries that a listing of a game-relative folder starts with, as the system lists them: `.` has the
/// folder's own details, and `..` the folder above. The game folder's `..` is the folder that holds it. A mask that does
/// not match a name leaves that entry out. The layers never add these entries (see `is_dot_name`).
pub(crate) fn dot_entries(
    mount: &Mount,
    rel: &str,
    mask: &str,
    own: Option<&Path>,
) -> Vec<WIN32_FIND_DATAW> {
    let parent_rel = rel.rsplit_once('\\').map_or("", |(parent, _)| parent);
    // The caller has usually probed the folder already (`shown`); reuse that path instead of probing it again.
    let own = own.map(Path::to_path_buf).or_else(|| {
        if rel.is_empty() {
            Some(mount.guest.clone())
        } else {
            visible(mount, rel).map(|(_, path)| path)
        }
    });
    let above = if rel.is_empty() {
        mount.guest.parent().map(Path::to_path_buf)
    } else if parent_rel.is_empty() {
        Some(mount.guest.clone())
    } else {
        visible(mount, parent_rel).map(|(_, path)| path)
    };
    let mut entries = Vec::with_capacity(2);

    for (dot, source) in [(".", own), ("..", above)] {
        if let Some(source) = source
            && wildcard_match(mask, dot)
            && let Ok(meta) = fs::metadata(&source)
        {
            entries.push(find_data(dot, &meta));
        }
    }

    entries
}

/// Whether a name is `.` or `..`. The system listing of a layer returns them for its folder; a listing takes them only
/// from `dot_entries`, once, so the layers do not add them again.
fn is_dot_name(name: &[u16]) -> bool {
    name == [u16::from(b'.')] || name == [u16::from(b'.'), u16::from(b'.')]
}

/// A hash of a name with each unit folded as `fold_unit` folds it, so it agrees with `same_folded_wide`. No `String` is built.
fn name_hash(name: &[u16]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;

    for unit in name {
        hash ^= u64::from(fold_unit(*unit));
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    hash
}

/// The `WIN32_FIND_DATAW` of one entry, from its metadata.
pub(super) fn find_data(name: &str, meta: &fs::Metadata) -> WIN32_FIND_DATAW {
    use std::os::windows::fs::MetadataExt;

    // SAFETY: `WIN32_FIND_DATAW` is plain data; all zeros is a valid value that the fields below then fill in.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    data.dwFileAttributes = meta.file_attributes();
    data.ftCreationTime = filetime(meta.creation_time());
    data.ftLastAccessTime = filetime(meta.last_access_time());
    data.ftLastWriteTime = filetime(meta.last_write_time());
    data.nFileSizeHigh = (meta.file_size() >> 32) as u32;
    data.nFileSizeLow = meta.file_size() as u32;
    // At most 259 units: the last slot of the 260 is the terminator, which stays zero.
    for (slot, unit) in data.cFileName.iter_mut().zip(name.encode_utf16().take(259)) {
        *slot = unit;
    }

    data
}

/// A file time as the two halves of `FILETIME`.
fn filetime(value: u64) -> FILETIME {
    FILETIME {
        dwLowDateTime: value as u32,
        dwHighDateTime: (value >> 32) as u32,
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::Storage::FileSystem::{FindExInfoStandard, FindExSearchNameMatch};

    use super::*;

    fn entry(name: &str) -> WIN32_FIND_DATAW {
        // SAFETY: `WIN32_FIND_DATAW` is plain data; all zeros is a valid value.
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

        for (slot, unit) in data.cFileName.iter_mut().zip(name.encode_utf16()) {
            *slot = unit;
        }

        data
    }

    #[test]
    fn a_plain_folder_returns_the_real_find_handle() {
        let base =
            std::env::temp_dir().join(format!("gobbo-listing-direct-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("game");
        let mods = base.join("mods");
        let upper = base.join("upper");

        for dir in [&root, &mods, &upper] {
            fs::create_dir_all(dir.join("archive")).unwrap();
        }

        for name in ["a.archive", "b.archive", "c.archive"] {
            fs::write(root.join("archive").join(name), b"game").unwrap();
        }

        let mount = Mount::new(
            "game".to_string(),
            root.clone(),
            gobbo_overlay_core::Layers::new(root.clone(), Some(mods), upper),
            Whiteouts::default(),
        );

        let layers = find_layers(
            &mount,
            "archive",
            "*",
            FindExInfoStandard,
            FindExSearchNameMatch,
            0,
            true,
            Vec::new(),
        );
        let (handle, first) = layers.direct.unwrap();
        assert!(layers.root.is_none());
        let mut names = vec![String::from_utf16(crate::win32::find_data_name(&first)).unwrap()];
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

        while unsafe { FindNextFileW(handle, &raw mut data) } != 0 {
            names.push(String::from_utf16(crate::win32::find_data_name(&data)).unwrap());
        }
        // SAFETY: the handle came from `find_layers` and is closed once.
        unsafe { FindClose(handle) };

        for name in ["a.archive", "b.archive", "c.archive"] {
            assert_eq!(
                names.iter().filter(|found| *found == name).count(),
                1,
                "{names:?}"
            );
        }

        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_batch_reader_reads_a_real_folder_for_spelling() {
        let base = std::env::temp_dir().join(format!("gobbo-listing-batch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        for number in 0..40 {
            fs::write(base.join(format!("file{number:02}.txt")), b"x").unwrap();
        }

        fs::write(base.join("note.bin"), b"x").unwrap();

        let mut reader = BatchReader::open(&base, "", "*.txt", FindExInfoStandard, 0).unwrap();
        let mut names = Vec::new();

        while let Some(data) = reader.next() {
            names.push(String::from_utf16(crate::win32::find_data_name(&data)).unwrap());
        }

        drop(reader);
        assert_eq!(names.len(), 40, "{names:?}");
        assert!(names.iter().any(|name| name == "file00.txt"), "{names:?}");
        assert!(!names.iter().any(|name| name == "note.bin"), "{names:?}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_batch_passes_the_same_names_as_the_find_stream() {
        let base =
            std::env::temp_dir().join(format!("gobbo-listing-batch-eq-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        for number in 0..250 {
            fs::write(base.join(format!("file{number:03}.txt")), b"x").unwrap();
        }

        for number in 0..50 {
            fs::write(base.join(format!("other{number:02}.bin")), b"x").unwrap();
        }

        for (mask, level) in [
            ("*", FindExInfoStandard),
            ("*.*", FindExInfoBasic),
            ("*.txt", FindExInfoStandard),
            ("file1??.txt", FindExInfoBasic),
        ] {
            let mut batch = BatchReader::open(&base, "", mask, level, 0).unwrap();
            let mut batch_names = Vec::new();

            while let Some(data) = batch.next() {
                batch_names.push(String::from_utf16(crate::win32::find_data_name(&data)).unwrap());
            }

            drop(batch);

            let (handle, first) = find_first(&base, "", mask, level, FindExSearchNameMatch, 0);
            assert_ne!(handle, INVALID_HANDLE_VALUE, "{mask}");
            let mut find_names =
                vec![String::from_utf16(crate::win32::find_data_name(&first)).unwrap()];
            let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

            while unsafe { FindNextFileW(handle, &raw mut data) } != 0 {
                find_names.push(String::from_utf16(crate::win32::find_data_name(&data)).unwrap());
            }
            // SAFETY: the handle came from `find_first` and is closed once.
            unsafe { FindClose(handle) };

            batch_names.sort_unstable();
            find_names.sort_unstable();
            assert_eq!(batch_names, find_names, "{mask} / {level}");
        }

        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_streamed_root_listing_keeps_each_name_once() {
        let base = std::env::temp_dir().join(format!("gobbo-listing-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("game");
        let mods = base.join("mods");
        let upper = base.join("upper");
        let folder = "archive";

        for dir in [&root, &mods, &upper] {
            fs::create_dir_all(dir.join(folder)).unwrap();
        }

        for number in 0..50 {
            fs::write(
                root.join(folder).join(format!("g{number:02}.archive")),
                b"game",
            )
            .unwrap();
        }

        fs::write(mods.join(folder).join("g03.archive"), b"mod").unwrap();
        fs::write(mods.join(folder).join("new.archive"), b"mod").unwrap();
        let mount = Mount::new(
            "game".to_string(),
            root.clone(),
            gobbo_overlay_core::Layers::new(root.clone(), Some(mods.clone()), upper.clone()),
            Whiteouts::default(),
        );

        let leading = dot_entries(&mount, folder, "*", None);
        let mut layers = find_layers(
            &mount,
            folder,
            "*",
            FindExInfoStandard,
            FindExSearchNameMatch,
            0,
            false,
            leading,
        );
        let mut names = Vec::new();
        let mut next = 0;

        loop {
            if let Some(data) = layers.taken.entries.get(next) {
                names.push(String::from_utf16(crate::win32::find_data_name(data)).unwrap());
                next += 1;
                continue;
            }

            let Some(data) = layers
                .root
                .as_mut()
                .and_then(|root| root.next_visible(&layers.taken))
            else {
                break;
            };
            names.push(String::from_utf16(crate::win32::find_data_name(&data)).unwrap());
            next += 1;
        }

        drop(layers);
        let unique: std::collections::HashSet<&String> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "a name is listed twice: {names:?}"
        );
        assert_eq!(names.len(), 53, "{names:?}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_long_name_is_cut_before_the_terminator() {
        let meta = fs::metadata(std::env::temp_dir()).unwrap();
        let data = find_data(&"a".repeat(300), &meta);
        assert_eq!(data.cFileName[258], u16::from(b'a'));
        assert_eq!(data.cFileName[259], 0);
    }

    #[test]
    fn a_file_time_is_split_into_its_two_halves() {
        let time = filetime(0x0123_4567_89AB_CDEF);
        assert_eq!(time.dwLowDateTime, 0x89AB_CDEF);
        assert_eq!(time.dwHighDateTime, 0x0123_4567);
    }

    #[test]
    fn a_hash_collision_keeps_both_entries_findable() {
        let mut taken = Taken::leading(Vec::new());
        taken.push_layer(entry("A"), Layer::Upper);
        taken.push_layer(entry("B"), Layer::Upper);
        // The state a real collision leaves: the index points at the later entry and the earlier one is in the list.
        let hash = name_hash(crate::win32::find_data_name(&taken.entries[0]));
        taken.index.insert(hash, 1);
        taken.collisions.push((hash, 0));
        assert_eq!(taken.find(&"A".encode_utf16().collect::<Vec<_>>()), Some(0));
        assert_eq!(taken.find(&"B".encode_utf16().collect::<Vec<_>>()), Some(1));
    }

    #[test]
    fn only_the_copy_prefix_reaches_the_allocating_shape_check() {
        let temp: Vec<u16> = ".gobbo-copy-7-9.tmp".encode_utf16().collect();
        assert!(is_copy_temp(&temp));

        for other in [
            ".gitignore",
            ".gobbo-copy-x.tmp",
            ".gobbo-copy-7-.tmp",
            "gobbo-copy-7-9.tmp",
        ] {
            let units: Vec<u16> = other.encode_utf16().collect();
            assert!(!is_copy_temp(&units), "{other}");
        }
    }

    #[test]
    fn a_large_upper_layer_takes_the_root_spelling_in_place() {
        let base = std::env::temp_dir().join(format!(
            "gobbo-listing-spelling-{}-{}",
            std::process::id(),
            "large"
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("game");
        let upper = base.join("upper");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&upper).unwrap();

        for number in 0..40 {
            fs::write(root.join(format!("Root{number:02}.bin")), b"root").unwrap();
            // The upper copy is spelled differently; the listing keeps the spelling of the file the game folder has.
            fs::write(upper.join(format!("root{number:02}.BIN")), b"upper").unwrap();
        }

        let mount = Mount::new(
            "game".to_string(),
            root.clone(),
            gobbo_overlay_core::Layers::new(root.clone(), None, upper.clone()),
            Whiteouts::default(),
        );

        let layers = find_layers(
            &mount,
            "",
            "*",
            FindExInfoStandard,
            FindExSearchNameMatch,
            0,
            false,
            Vec::new(),
        );
        let names: Vec<String> = layers
            .taken
            .entries
            .iter()
            .map(|data| String::from_utf16(crate::win32::find_data_name(data)).unwrap())
            .collect();
        assert!(names.iter().any(|name| name == "Root00.bin"), "{names:?}");
        assert!(!names.iter().any(|name| name == "root00.BIN"), "{names:?}");
        drop(layers);
        fs::remove_dir_all(&base).unwrap();
    }
}
