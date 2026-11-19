//! The paths a handle has: the path Windows gives it in a layer, the path the program sees (the guest path of the
//! mount that serves it), and the game folder in the two forms a program can ask for.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use gobbo_overlay_core::{Layers, fold_unit, relative_to, strip_folded_prefix};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, VOLUME_NAME_DOS, VOLUME_NAME_NT,
};

use crate::guard::Guard;
use crate::state::{self, Mount};
use crate::text::wide;
use crate::win32::PreserveLastError;

/// The volume name form is the low bits of the flags of `GetFinalPathNameByHandleW`.
pub(super) const VOLUME_NAME_MASK: u32 = 0x7;

/// The layer a handle names, when it can be told without building the path as a `String`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerKind {
    /// A file of the upper layer: a write or a delete acts there.
    Upper,
    /// A file of a lower layer: a write or a delete would reach it.
    Lower,
    /// A path outside every mount; the call passes through.
    Outside,
}

/// The cached text of one mount: its guest path and the layer folders in the forms the handle APIs return.
struct MountText {
    mount: &'static Mount,
    guest: String,
    guest_verbatim: String,
    root: String,
    root_verbatim: String,
    upper: String,
    upper_verbatim: String,
    staging: Option<String>,
    staging_verbatim: Option<String>,
    guest_units: Vec<u16>,
    root_units: Vec<u16>,
    upper_units: Vec<u16>,
    staging_units: Option<Vec<u16>>,
    guest_nt: OnceLock<Option<String>>,
}

impl MountText {
    fn new(mount: &'static Mount) -> Self {
        let plain = |path: &Path| path.to_string_lossy().into_owned();
        let units = |text: &str| text.encode_utf16().collect();
        let guest_verbatim = verbatim(&mount.guest);
        let root_verbatim = verbatim(&mount.layers.root);
        let upper_verbatim = verbatim(&mount.layers.upper);
        let staging_verbatim = mount.layers.staging.as_deref().map(verbatim);
        Self {
            mount,
            guest: plain(&mount.guest),
            guest_units: units(&guest_verbatim),
            guest_verbatim,
            root: plain(&mount.layers.root),
            root_units: units(&root_verbatim),
            root_verbatim,
            upper: plain(&mount.layers.upper),
            upper_units: units(&upper_verbatim),
            upper_verbatim,
            staging: mount.layers.staging.as_deref().map(plain),
            staging_units: staging_verbatim.as_deref().map(units),
            staging_verbatim,
            guest_nt: OnceLock::new(),
        }
    }
}

static MOUNT_TEXT: OnceLock<Option<Vec<MountText>>> = OnceLock::new();

fn mount_text() -> Option<&'static [MountText]> {
    MOUNT_TEXT
        .get_or_init(|| {
            let mounts = state::mounts()?;
            Some(mounts.all().map(MountText::new).collect())
        })
        .as_deref()
}

/// The layer a handle names, without allocating the path. `None` when the path is unavailable, too long for the stack
/// buffer, or the mounts are not stored; the caller then uses the allocating path.
pub(crate) fn layer_kind(handle: *mut c_void) -> Option<LayerKind> {
    let text = mount_text()?;
    let mut buffer = [0u16; 512];
    let length = final_path_units(handle, &mut buffer)?;
    Some(classify_units(&buffer[..length], text))
}

/// The layer a final path names, given the cached mount text, without building a `String`.
pub(crate) fn layer_kind_units(path: &[u16]) -> Option<LayerKind> {
    Some(classify_units(path, mount_text()?))
}

/// The final DOS path of a handle in `buffer`, or `None` when Windows gives none or it does not fit.
pub(crate) fn final_path_units(handle: *mut c_void, buffer: &mut [u16; 512]) -> Option<usize> {
    // SAFETY: the caller's buffer holds `buffer.len()` writable units.
    unsafe { final_path_units_with(handle, VOLUME_NAME_DOS, buffer) }
}

/// The layer and the real DOS path of a handle, from one `GetFinalPathNameByHandleW`. A directory query needs both;
/// asking `layer_kind` and then `layer_path` called the same API twice. The path of an outside handle is empty: the
/// caller passes through and never uses it, so the pass-through path does not allocate.
pub(crate) fn layer_path_kind(handle: *mut c_void) -> Option<(LayerKind, String)> {
    let text = mount_text()?;
    let mut buffer = [0u16; 512];
    let length = final_path_units(handle, &mut buffer)?;
    let kind = classify_units(&buffer[..length], text);
    if kind == LayerKind::Outside {
        return Some((kind, String::new()));
    }
    Some((kind, String::from_utf16_lossy(&buffer[..length])))
}

/// The layer a final path names, given the cached mount text, without building a `String`.
fn classify_units(path: &[u16], mounts: &[MountText]) -> LayerKind {
    for text in mounts {
        if strip_prefix_ci_units(path, &text.upper_units).is_some() {
            return LayerKind::Upper;
        }
        if strip_prefix_ci_units(path, &text.root_units).is_some()
            || text
                .staging_units
                .as_deref()
                .is_some_and(|staging| strip_prefix_ci_units(path, staging).is_some())
            || strip_prefix_ci_units(path, &text.guest_units).is_some()
        {
            return LayerKind::Lower;
        }
    }
    LayerKind::Outside
}

/// The path a handle really has, in the layer it was taken from, in the DOS form (`\\?\C:\...`). The hook's own calls
/// use it, so they see the layer and not the guest path.
pub(crate) fn layer_path(handle: *mut c_void) -> Option<String> {
    original_path(handle, VOLUME_NAME_DOS)
}

/// The path of a handle, without the `\\?\` prefix, in a caller's stack buffer, when the handle is outside every mount.
/// Nothing is allocated. `None` when the handle is inside a mount (its guest path needs the allocating mapping), when
/// Windows gives no path, or when the path does not fit the buffer; the caller then uses `program_path`.
pub(crate) fn outside_path_units(handle: *mut c_void, buffer: &mut [u16; 512]) -> Option<usize> {
    let mounts = mount_text()?;
    let length = final_path_units(handle, buffer)?;
    if classify_units(&buffer[..length], mounts) != LayerKind::Outside {
        return None;
    }
    let prefix = r"\\?\".encode_utf16().count();
    if length <= prefix || !buffer[..prefix].iter().copied().eq(r"\\?\".encode_utf16()) {
        return Some(length);
    }
    buffer.copy_within(prefix..length, 0);
    Some(length - prefix)
}

/// The path a handle names as the program sees it, without the `\\?\` prefix: the guest path for a path in a layer, or
/// else the path Windows gives. `None` when Windows gives none.
pub(crate) fn program_path(handle: *mut c_void) -> Option<String> {
    let path = layer_path(handle)?;
    let path = map_layer_path_cached(&path).unwrap_or(path);
    Some(path.strip_prefix(r"\\?\").unwrap_or(&path).to_string())
}

/// The mount and relative path when `path` is in the upper layer of a mount. `None` when it is elsewhere.
pub(crate) fn relative_in_upper(path: &str) -> Option<(&'static Mount, String)> {
    let text = mount_text()?;
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    text.iter()
        .find_map(|mount| relative_to(&mount.upper, path).map(|rel| (mount.mount, rel)))
}

/// The mount and relative path when `path` is in a lower layer. The upper layer is excluded because a handle there may
/// be changed in place.
pub(crate) fn relative_in_lower_layers(path: &str) -> Option<(&'static Mount, String)> {
    let text = mount_text()?;
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    text.iter()
        .find_map(|mount| relative_in_lower(mount, path).map(|rel| (mount.mount, rel)))
}

/// The mount and relative path when `path` is in any layer.
pub(crate) fn relative_in_layers(path: &str) -> Option<(&'static Mount, String)> {
    let text = mount_text()?;
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    text.iter().find_map(|mount| {
        relative_to(&mount.upper, path)
            .or_else(|| relative_in_lower(mount, path))
            .map(|rel| (mount.mount, rel))
    })
}

/// The mount-relative path of `path` in the lower layers of `mount`: the root, then the staging layer.
fn relative_in_lower(mount: &MountText, path: &str) -> Option<String> {
    relative_to(&mount.root, path).or_else(|| {
        mount
            .staging
            .as_deref()
            .and_then(|staging| relative_to(staging, path))
    })
}

/// The guest path of the mount whose layers contain `path`, in the `\\?\` form.
pub(crate) fn map_layer_path_cached(path: &str) -> Option<String> {
    let text = mount_text()?;
    for mount in text {
        if strip_prefix_ci(path, &mount.guest_verbatim).is_some() {
            return Some(path.to_string());
        }
        for base in [&mount.upper_verbatim]
            .into_iter()
            .chain(mount.staging_verbatim.as_ref())
            .chain(std::iter::once(&mount.root_verbatim))
        {
            if let Some(rest) = strip_prefix_ci(path, base) {
                return Some(format!("{}{rest}", mount.guest_verbatim));
            }
        }
    }
    None
}

/// The path in `path` with the spelling of a layer prefix replaced by the guest path. Both are the `\\?\` form. `None`
/// when `path` is outside the layers. The game mount is the common case; the explicit form keeps the unit tests free of
/// a stored session.
pub(crate) fn map_layer_path(path: &str, layers: &Layers) -> Option<String> {
    let root = verbatim(&layers.root);
    let upper = verbatim(&layers.upper);
    let staging = layers.staging.as_deref().map(verbatim);
    map_layer_path_text(path, &root, &upper, staging.as_deref())
}

/// The body shared by the cached and explicit forms.
fn map_layer_path_text(
    path: &str,
    root: &str,
    upper: &str,
    staging: Option<&str>,
) -> Option<String> {
    if strip_prefix_ci(path, root).is_some() {
        return Some(path.to_string());
    }
    for base in [upper].into_iter().chain(staging) {
        if let Some(rest) = strip_prefix_ci(path, base) {
            return Some(format!("{root}{rest}"));
        }
    }
    None
}

/// The path of a handle as Windows gives it, without the hook's answer.
pub(super) fn original_path(handle: *mut c_void, flags: u32) -> Option<String> {
    let original = state::hooks().and_then(|hooks| hooks.get_final_path.as_ref())?;
    // Almost every path fits here. A long one falls back to a heap buffer sized by the call; the stack keeps the
    // common delete, rename and final-path read from allocating 2 KiB first.
    let mut stack = [0u16; 512];
    // SAFETY: the buffer holds 512 units, and the call writes at most that many.
    let length = unsafe { original.call(handle, stack.as_mut_ptr(), stack.len() as u32, flags) };
    let _last_error = PreserveLastError::capture();
    let length = length as usize;
    if length == 0 {
        return None;
    }
    if length < stack.len() {
        return Some(String::from_utf16_lossy(&stack[..length]));
    }
    let mut size = length;
    loop {
        let mut buffer = vec![0u16; size];
        // SAFETY: the buffer is `size` units long, and the call writes at most that many.
        let length = unsafe { original.call(handle, buffer.as_mut_ptr(), size as u32, flags) };
        let _last_error = PreserveLastError::capture();
        if length == 0 {
            return None;
        }
        if (length as usize) < size {
            buffer.truncate(length as usize);
            return Some(String::from_utf16_lossy(&buffer));
        }
        // The call returns the size needed, including the terminator, when the buffer is too small.
        size = length as usize;
        if size > 32_768 {
            return None;
        }
    }
}

/// The path of a handle into `buffer`, in the requested form, when Windows gives one that fits. The caller captures
/// `GetLastError` immediately: this function leaves the original call's error in place. No allocation.
pub(super) fn original_path_units(
    handle: *mut c_void,
    flags: u32,
    buffer: &mut [u16; 520],
) -> Option<usize> {
    // SAFETY: the caller's buffer holds `buffer.len()` writable units.
    unsafe { final_path_units_with(handle, flags, buffer) }
}

/// The final path of `handle` in `buffer`, with the flags `flags`. `None` when Windows gives none or it does not fit.
///
/// # Safety
/// `buffer` must be valid for `buffer.len()` writable units.
unsafe fn final_path_units_with(
    handle: *mut c_void,
    flags: u32,
    buffer: &mut [u16],
) -> Option<usize> {
    let original = state::hooks().and_then(|hooks| hooks.get_final_path.as_ref())?;
    // SAFETY: the caller's buffer holds `buffer.len()` units, and the call writes at most that many.
    let length = unsafe { original.call(handle, buffer.as_mut_ptr(), buffer.len() as u32, flags) };
    let _last_error = PreserveLastError::capture();
    let length = length as usize;
    (length != 0 && length < buffer.len()).then_some(length)
}

/// The guest path of the mount whose layers contain `path`, in the `\\?\` form, without building a `String`. `None` when
/// the path is outside every layer or the guest path does not fit `out`.
pub(crate) fn map_layer_path_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    let text = mount_text()?;
    for mount in text {
        if strip_prefix_ci_units(path, &mount.guest_units).is_some() {
            return copy_units(path, out);
        }
        for base in [mount.upper_units.as_slice()]
            .into_iter()
            .chain(mount.staging_units.as_deref())
            .chain(std::iter::once(mount.root_units.as_slice()))
        {
            if let Some(rest) = strip_prefix_ci_units(path, base) {
                let needed = mount.guest_units.len() + rest.len();
                if needed > out.len() {
                    return None;
                }
                out[..mount.guest_units.len()].copy_from_slice(&mount.guest_units);
                out[mount.guest_units.len()..needed].copy_from_slice(rest);
                return Some(needed);
            }
        }
    }
    None
}

/// The game form of a module path as `GetModuleFileNameW` reports it (a plain DOS path, without `\\?\`), in `out`.
/// `None` when the module is outside the layers, the path is too long, or no session is stored.
pub(crate) fn guest_module_path_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    const VERBATIM: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const UNC: &[u16] = &[b'\\' as u16, b'\\' as u16];
    const UNC_VERBATIM: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    let mut verbatim = [0u16; 1040];
    let prefix = if path.starts_with(UNC) {
        UNC_VERBATIM
    } else {
        VERBATIM
    };
    let length = prefix.len() + path.len();
    if length > verbatim.len() {
        return None;
    }
    verbatim[..prefix.len()].copy_from_slice(prefix);
    verbatim[prefix.len()..length].copy_from_slice(path);
    let mut guest = [0u16; 1040];
    let length = map_layer_path_units(&verbatim[..length], &mut guest)?;
    strip_verbatim_units(&guest[..length], out)
}

/// Removes the `\\?\` or `\\?\UNC\` prefix from a UTF-16 path.
fn strip_verbatim_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    const PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const UNC: &[u16] = &[b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16];
    let text = path.strip_prefix(PREFIX).unwrap_or(path);
    if let Some(rest) = text.strip_prefix(UNC) {
        if rest.len() + 2 > out.len() {
            return None;
        }
        out[0] = b'\\' as u16;
        out[1] = b'\\' as u16;
        out[2..2 + rest.len()].copy_from_slice(rest);
        return Some(2 + rest.len());
    }
    copy_units(text, out)
}

/// `without_volume` for a UTF-16 path, in a caller's stack buffer. No allocation.
pub(crate) fn without_volume_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    let length = strip_verbatim_units(path, out)?;
    if out.get(1) == Some(&u16::from(b':')) {
        out.copy_within(2..length, 0);
        return Some(length - 2);
    }
    Some(length)
}

/// `guest_nt_for_path` for a UTF-16 path, in a caller's stack buffer. No allocation.
pub(crate) fn guest_nt_for_path_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    let text = mount_text()?;
    for mount in text {
        let Some(rest) = strip_prefix_ci_units(path, &mount.guest_units) else {
            continue;
        };
        let nt = guest_nt(mount)?;
        let mut length = 0;
        for unit in nt.encode_utf16().chain(rest.iter().copied()) {
            if length >= out.len() {
                return None;
            }
            out[length] = unit;
            length += 1;
        }
        return Some(length);
    }
    None
}

/// Copies `path` into `out`; `None` when it does not fit.
fn copy_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    if path.len() > out.len() {
        return None;
    }
    out[..path.len()].copy_from_slice(path);
    Some(path.len())
}

/// The `\\?\` form of an absolute path, so it compares with the answer of `GetFinalPathNameByHandleW`.
pub(super) fn verbatim(path: &Path) -> String {
    let text = path.display().to_string();
    if text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
        return text;
    }
    let text = match text.strip_prefix(r"\\") {
        Some(server) => return format!(r"\\?\UNC\{server}"),
        None => text,
    };
    format!(r"\\?\{text}")
}

/// `path` without `base`, when `path` starts with `base` under the folding rule and the next character is a separator or
/// the end. `base` is a `\\?\` path.
pub(super) fn strip_prefix_ci<'a>(path: &'a str, base: &str) -> Option<&'a str> {
    let rest = strip_folded_prefix(path, base)?;
    if rest.is_empty() || rest.starts_with('\\') {
        Some(rest)
    } else {
        None
    }
}

/// The guest path of a mount in the NT form (`\Device\HarddiskVolume...`), opened once and cached.
fn guest_nt(text: &MountText) -> Option<&str> {
    text.guest_nt
        .get_or_init(|| {
            nt_units(Path::new(&text.guest)).map(|units| String::from_utf16_lossy(&units))
        })
        .as_deref()
}

/// The NT form of a guest path, with the part below the guest folder, for the final-path translation of a layer path.
pub(crate) fn guest_nt_for_path(path: &str) -> Option<String> {
    let text = mount_text()?;
    for mount in text {
        if let Some(rest) = strip_prefix_ci(path, &mount.guest_verbatim) {
            return Some(format!("{}{rest}", guest_nt(mount)?));
        }
    }
    None
}

/// The NT prefixes of one mount's layers, with the guest path they map to.
struct MountNt {
    guest: Vec<u16>,
    guest_rel: Option<Vec<u16>>,
    upper: Vec<u16>,
    upper_rel: Option<Vec<u16>>,
    staging: Option<(Vec<u16>, Option<Vec<u16>>)>,
    root: Vec<u16>,
    root_rel: Option<Vec<u16>>,
}

static LAYER_NT: OnceLock<Option<Vec<MountNt>>> = OnceLock::new();

/// The NT prefixes of every mount, built once. The game folder comes from the cached open; the other folders are
/// opened here, under the guard, because this runs during install or on the first path query.
fn layer_nt() -> Option<&'static [MountNt]> {
    LAYER_NT
        .get_or_init(|| {
            let text = mount_text()?;
            let mut mounts = Vec::with_capacity(text.len());
            for mount in text {
                // One mount with a folder that cannot be opened must not disable the NT mapping of the others.
                let Some(guest) = guest_nt(mount) else {
                    continue;
                };
                let guest = guest.encode_utf16().collect::<Vec<_>>();
                let Some(upper) = nt_units(Path::new(&mount.upper)) else {
                    continue;
                };
                let Some(root) = nt_units(Path::new(&mount.root)) else {
                    continue;
                };
                let staging = mount.staging.as_deref().and_then(|staging| {
                    let units = nt_units(Path::new(staging))?;
                    let rel = without_device(&units).map(<[u16]>::to_vec);
                    Some((units, rel))
                });
                mounts.push(MountNt {
                    guest_rel: without_device(&guest).map(<[u16]>::to_vec),
                    upper_rel: without_device(&upper).map(<[u16]>::to_vec),
                    root_rel: without_device(&root).map(<[u16]>::to_vec),
                    guest,
                    upper,
                    staging,
                    root,
                });
            }
            Some(mounts)
        })
        .as_deref()
}

/// Maps an NT layer path to the NT path of the guest folder. `None` when the path is not in a layer.
pub(crate) fn map_layer_path_nt(units: &[u16]) -> Option<Vec<u16>> {
    let mounts = layer_nt()?;
    for mount in mounts {
        for (base, rel, guest, guest_rel) in [
            (
                Some(mount.upper.as_slice()),
                mount.upper_rel.as_deref(),
                mount.guest.as_slice(),
                mount.guest_rel.as_deref(),
            ),
            (
                mount.staging.as_ref().map(|(units, _)| units.as_slice()),
                mount.staging.as_ref().and_then(|(_, rel)| rel.as_deref()),
                mount.guest.as_slice(),
                mount.guest_rel.as_deref(),
            ),
            (
                Some(mount.root.as_slice()),
                mount.root_rel.as_deref(),
                mount.guest.as_slice(),
                mount.guest_rel.as_deref(),
            ),
        ] {
            if let Some(base) = base
                && let Some(rest) = strip_prefix_ci_units(units, base)
            {
                return Some(join_units(guest, rest));
            }
            if let (Some(base), Some(rel)) = (rel, guest_rel)
                && let Some(rest) = strip_prefix_ci_units(units, base)
            {
                return Some(join_units(rel, rest));
            }
        }
    }
    None
}

/// The part of an NT path after `\Device\<device>`, which starts with a separator. `None` for a path without a device.
fn without_device(path: &[u16]) -> Option<&[u16]> {
    let device: Vec<u16> = r"\Device".encode_utf16().collect();
    let rest = strip_prefix_ci_units(path, &device)?;
    let rest = rest.strip_prefix(&[u16::from(b'\\')])?;
    let separator = rest.iter().position(|unit| *unit == u16::from(b'\\'))?;
    Some(&rest[separator..])
}

/// The NT path of a folder, from an open of the folder itself. The guard keeps the open out of the hook.
fn nt_units(path: &Path) -> Option<Vec<u16>> {
    let _guard = Guard::enter();
    let text = wide(path.as_os_str());
    let create_file = state::hooks().and_then(|hooks| hooks.create_file.as_ref())?;
    // SAFETY: `text` is a NUL-terminated path, and the other arguments are the plain values of an open of a folder.
    let handle = unsafe {
        create_file.call(
            text.as_ptr(),
            FILE_READ_ATTRIBUTES,
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
    let units = original_path(handle, VOLUME_NAME_NT).map(|nt| nt.encode_utf16().collect());
    // SAFETY: the handle was opened above and is closed once.
    unsafe { CloseHandle(handle) };
    units
}

/// Whether `path` starts with `base` and the next unit is a separator or the end.
fn strip_prefix_ci_units<'a>(path: &'a [u16], base: &[u16]) -> Option<&'a [u16]> {
    if path.len() < base.len()
        || !path[..base.len()]
            .iter()
            .zip(base)
            .all(|(left, right)| fold_unit(*left) == fold_unit(*right))
    {
        return None;
    }
    let rest = &path[base.len()..];
    if rest.is_empty() || rest[0] == u16::from(b'\\') {
        Some(rest)
    } else {
        None
    }
}

/// Joins the guest prefix and the rest of a layer path. `rest` starts with a separator or is empty.
fn join_units(base: &[u16], rest: &[u16]) -> Vec<u16> {
    let mut units = Vec::with_capacity(base.len() + rest.len());
    units.extend_from_slice(base);
    units.extend_from_slice(rest);
    units
}

/// A path without its volume, as `VOLUME_NAME_NONE` asks: `\...`; for a server, `\\server\share\...`.
pub(crate) fn without_volume(path: &str) -> String {
    let text = path.strip_prefix(r"\\?\").unwrap_or(path);
    if let Some(server) = text.strip_prefix(r"UNC\") {
        return format!(r"\\{server}");
    }
    if text.as_bytes().get(1) == Some(&b':') {
        return text[2..].to_string();
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_is_removed_from_a_drive_and_a_server_path() {
        assert_eq!(without_volume(r"\\?\C:\Game\a.txt"), r"\Game\a.txt");
        assert_eq!(
            without_volume(r"\\?\UNC\server\share\a.txt"),
            r"\\server\share\a.txt"
        );
    }

    #[test]
    fn a_volume_is_removed_from_a_utf16_path() {
        let mut out = [0u16; 64];
        let drive: Vec<u16> = r"\\?\C:\Game\a.txt".encode_utf16().collect();
        let length = without_volume_units(&drive, &mut out).unwrap();
        assert_eq!(String::from_utf16(&out[..length]).unwrap(), r"\Game\a.txt");
        let unc: Vec<u16> = r"\\?\UNC\server\share\a.txt".encode_utf16().collect();
        let length = without_volume_units(&unc, &mut out).unwrap();
        assert_eq!(
            String::from_utf16(&out[..length]).unwrap(),
            r"\\server\share\a.txt"
        );
    }

    #[test]
    fn a_layer_path_maps_back_to_the_guest_path() {
        let base = std::env::temp_dir().join(format!("gobbo-handles-{}", std::process::id()));
        let root = base.join("game");
        let upper = base.join("state/overwrite");
        let staging = base.join("state/mods");
        let layers = Layers {
            root: root.clone(),
            staging: Some(staging.clone()),
            upper: upper.clone(),
        };
        assert_eq!(
            map_layer_path(
                &format!(r"\\?\{}", upper.join("data/a.txt").display()),
                &layers
            ),
            Some(format!(r"\\?\{}", root.join("data/a.txt").display()))
        );
        assert_eq!(
            map_layer_path(
                &format!(r"\\?\{}", staging.join("data/a.txt").display()),
                &layers
            ),
            Some(format!(r"\\?\{}", root.join("data/a.txt").display()))
        );
        assert_eq!(
            map_layer_path(
                &format!(r"\\?\{}", root.join("data/a.txt").display()),
                &layers
            ),
            Some(format!(r"\\?\{}", root.join("data/a.txt").display()))
        );
        assert_eq!(map_layer_path(r"\\?\C:\Other\a.txt", &layers), None);
    }

    #[test]
    fn a_sibling_with_the_same_prefix_is_outside() {
        let base =
            std::env::temp_dir().join(format!("gobbo-handles-sibling-{}", std::process::id()));
        let root = base.join("game");
        let layers = Layers {
            root: root.clone(),
            staging: None,
            upper: base.join("state/overwrite"),
        };
        assert_eq!(
            map_layer_path(&format!(r"\\?\{}s", root.display()), &layers),
            None
        );
    }
}
