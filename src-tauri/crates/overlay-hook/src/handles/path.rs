//! The paths a handle has: the path Windows gives it in a layer, the path the program sees (the guest path of the
//! mount that serves it), and the game folder in the two forms a program can ask for.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use gobbo_overlay_core::{Layers, fold_unit, relative_to, strip_folded_prefix, wide};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, VOLUME_NAME_DOS, VOLUME_NAME_NT,
};

use crate::guard::Guard;
use crate::paths::short_path_units;
use crate::state::{self, Mount};
use crate::win32::PreserveLastError;

/// The volume name form is the low bits of the flags of `GetFinalPathNameByHandleW`.
pub(super) const VOLUME_NAME_MASK: u32 = 0x7;

/// Whether a path lies inside a mount's layers or outside every mount, the only distinction callers ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerKind {
    /// A path inside at least one mount's layers; the layers decide the answer.
    InLayer,
    /// A path outside every mount; the call passes through.
    Outside,
}

/// The 8.3 spelling of a cached path in the three shapes the comparisons use. Built at install, never rebuilt.
struct CachedShort {
    plain: String,
    verbatim: String,
    units: Vec<u16>,
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
    /// The same prefixes under their 8.3 spelling. A short incoming path matches these; the answer is still built from
    /// the long guest above, so the caller sees one canonical path.
    guest_short: Option<CachedShort>,
    root_short: Option<CachedShort>,
    upper_short: Option<CachedShort>,
    staging_short: Option<CachedShort>,
    guest_nt: OnceLock<Option<String>>,
}

impl MountText {
    fn new(mount: &'static Mount) -> Self {
        let plain = |path: &Path| path.to_string_lossy().into_owned();
        let units = |text: &str| text.encode_utf16().collect();
        let short = |path: &Path| {
            let short_units = short_path_units(path)?;
            let plain = String::from_utf16_lossy(&short_units);
            let verbatim = verbatim(Path::new(&plain));
            Some(CachedShort {
                units: verbatim.encode_utf16().collect(),
                verbatim,
                plain,
            })
        };
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
            guest_short: short(&mount.guest),
            root_short: short(&mount.layers.root),
            upper_short: short(&mount.layers.upper),
            staging_short: mount.layers.staging.as_deref().and_then(short),
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

/// Whether a handle's path is inside a mount, without allocating the path. `None` when the path is unavailable, too long
/// for the stack buffer, or the mounts are not stored; the caller then uses the allocating path.
pub(crate) fn layer_kind(handle: *mut c_void) -> Option<LayerKind> {
    let text = mount_text()?;
    let mut buffer = [0u16; 512];
    let length = final_path_units(handle, &mut buffer)?;
    Some(classify_kind(&buffer[..length], text))
}

/// Whether a final path is inside a mount, given the cached mount text, without building a `String`.
pub(crate) fn layer_kind_units(path: &[u16]) -> Option<LayerKind> {
    Some(classify_kind(path, mount_text()?))
}

/// The final DOS path of a handle in `buffer`, or `None` when Windows gives none or it does not fit.
pub(crate) fn final_path_units(handle: *mut c_void, buffer: &mut [u16; 512]) -> Option<usize> {
    // SAFETY: the caller's buffer holds `buffer.len()` writable units.
    unsafe { final_path_units_with(handle, VOLUME_NAME_DOS, buffer) }
}

/// Whether a handle is inside a mount and its real DOS path, from one `GetFinalPathNameByHandleW`. A directory query
/// needs both; asking `layer_kind` and then `layer_path` called the same API twice. The path of an outside handle is
/// empty: the caller passes through and never uses it, so the pass-through path does not allocate.
pub(crate) fn layer_path_kind(handle: *mut c_void) -> Option<(LayerKind, String)> {
    let text = mount_text()?;
    let mut buffer = [0u16; 512];
    let length = final_path_units(handle, &mut buffer)?;
    let kind = classify_kind(&buffer[..length], text);

    if kind == LayerKind::Outside {
        return Some((kind, String::new()));
    }

    Some((kind, String::from_utf16_lossy(&buffer[..length])))
}

/// Tries a cached long prefix first, then its 8.3 spelling. No allocation; the primitives only fold case, so the two
/// spellings stay a call-site decision. `None` for a layer the mount does not have.
fn strip_layer_units<'a>(
    path: &'a [u16],
    long: Option<&[u16]>,
    short: Option<&[u16]>,
) -> Option<&'a [u16]> {
    long.and_then(|long| strip_prefix_ci_units(path, long))
        .or_else(|| short.and_then(|short| strip_prefix_ci_units(path, short)))
}

/// The string form of `strip_layer_units`, for the answers the hook already holds as text.
fn strip_layer_str<'a>(path: &'a str, long: Option<&str>, short: Option<&str>) -> Option<&'a str> {
    long.and_then(|long| strip_prefix_ci(path, long))
        .or_else(|| short.and_then(|short| strip_prefix_ci(path, short)))
}

/// Whether a final path is inside any mount's layers, given the cached mount text.
fn classify_kind(path: &[u16], mounts: &[MountText]) -> LayerKind {
    for text in mounts {
        let in_layer = |long: Option<&[u16]>, short: Option<&CachedShort>| {
            strip_layer_units(path, long, short.map(|short| short.units.as_slice())).is_some()
        };
        if in_layer(Some(&text.upper_units), text.upper_short.as_ref())
            || in_layer(Some(&text.root_units), text.root_short.as_ref())
            || in_layer(text.staging_units.as_deref(), text.staging_short.as_ref())
            || in_layer(Some(&text.guest_units), text.guest_short.as_ref())
        {
            return LayerKind::InLayer;
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

    if classify_kind(&buffer[..length], mounts) != LayerKind::Outside {
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
    Some(strip_verbatim_str(&path).to_string())
}

/// `relative_to` on the long layer path, then on its 8.3 spelling. Both produce the same relative tail.
fn relative_to_either(base: &str, short: Option<&str>, path: &str) -> Option<String> {
    relative_to(base, path).or_else(|| short.and_then(|short| relative_to(short, path)))
}

/// The mount-relative path of `path` in the upper layer of `mount`: long spelling, then its 8.3 form.
fn relative_in_upper_of(mount: &MountText, path: &str) -> Option<String> {
    relative_to_either(
        &mount.upper,
        mount.upper_short.as_ref().map(|short| short.plain.as_str()),
        path,
    )
}

/// The mount-relative path of `path` in any layer of `mount`: the upper layer, then the lower layers.
fn relative_in_any_of(mount: &MountText, path: &str) -> Option<String> {
    relative_in_upper_of(mount, path).or_else(|| relative_in_lower(mount, path))
}

/// Shared body of the three relative-path queries: strip the verbatim prefix once, then ask `from` about each mount in
/// session order. The upper and lower filters remain the callers' choice; this adds no allocation or lock.
fn relative_in_layers_in(
    path: &str,
    from: impl Fn(&MountText, &str) -> Option<String>,
) -> Option<(&'static Mount, String)> {
    relative_in_layers_from(mount_text()?, path, from)
}

/// The pure iterate-and-relative part, so the layer filters can be tested without a stored session.
fn relative_in_layers_from(
    text: &[MountText],
    path: &str,
    from: impl Fn(&MountText, &str) -> Option<String>,
) -> Option<(&'static Mount, String)> {
    let path = strip_verbatim_str(path);
    text.iter()
        .find_map(|mount| from(mount, path).map(|rel| (mount.mount, rel)))
}

/// The mount and relative path when `path` is in the upper layer of a mount. `None` when it is elsewhere.
pub(crate) fn relative_in_upper(path: &str) -> Option<(&'static Mount, String)> {
    relative_in_layers_in(path, relative_in_upper_of)
}

/// The mount and relative path when `path` is in a lower layer. The upper layer is excluded because a handle there may
/// be changed in place.
pub(crate) fn relative_in_lower_layers(path: &str) -> Option<(&'static Mount, String)> {
    relative_in_layers_in(path, relative_in_lower)
}

/// The mount and relative path when `path` is in any layer.
pub(crate) fn relative_in_layers(path: &str) -> Option<(&'static Mount, String)> {
    relative_in_layers_in(path, relative_in_any_of)
}

/// The mount-relative path of `path` in the lower layers of `mount`: the root, then the staging layer.
fn relative_in_lower(mount: &MountText, path: &str) -> Option<String> {
    relative_to_either(
        &mount.root,
        mount.root_short.as_ref().map(|short| short.plain.as_str()),
        path,
    )
    .or_else(|| {
        mount.staging.as_deref().and_then(|staging| {
            relative_to_either(
                staging,
                mount
                    .staging_short
                    .as_ref()
                    .map(|short| short.plain.as_str()),
                path,
            )
        })
    })
}

/// The guest path of the mount whose layers contain `path`, in the `\\?\` form.
pub(crate) fn map_layer_path_cached(path: &str) -> Option<String> {
    let text = mount_text()?;

    for mount in text {
        if strip_prefix_ci(path, &mount.guest_verbatim).is_some() {
            return Some(path.to_string());
        }

        if let Some(rest) = mount
            .guest_short
            .as_ref()
            .and_then(|short| strip_prefix_ci(path, &short.verbatim))
        {
            return Some(format!("{}{rest}", mount.guest_verbatim));
        }

        for (long, short) in [
            (
                Some(mount.upper_verbatim.as_str()),
                mount.upper_short.as_ref(),
            ),
            (
                mount.staging_verbatim.as_deref(),
                mount.staging_short.as_ref(),
            ),
            (
                Some(mount.root_verbatim.as_str()),
                mount.root_short.as_ref(),
            ),
        ] {
            if let Some(rest) = strip_layer_str(path, long, short.map(|s| s.verbatim.as_str())) {
                return Some(format!("{}{rest}", mount.guest_verbatim));
            }
        }
    }

    None
}

/// The `\\?\` form of a path's 8.3 spelling, for the explicit `Layers` form that has no session cache. Only the cold
/// growing-buffer fallback calls this; the cached mapping uses the prefixes built at install.
fn short_verbatim(path: &Path) -> Option<String> {
    let units = short_path_units(path)?;
    Some(verbatim(Path::new(&String::from_utf16_lossy(&units))))
}

/// The path in `path` with the spelling of a layer prefix replaced by the guest path. Both are the `\\?\` form. `None`
/// when `path` is outside the layers. The game mount is the common case; the explicit form keeps the unit tests free of
/// a stored session.
pub(crate) fn map_layer_path(path: &str, layers: &Layers) -> Option<String> {
    let root = verbatim(&layers.root);
    let upper = verbatim(&layers.upper);
    let staging = layers.staging.as_deref().map(verbatim);
    let root_short = short_verbatim(&layers.root);
    let upper_short = short_verbatim(&layers.upper);
    let staging_short = layers.staging.as_deref().and_then(short_verbatim);
    map_layer_path_text(
        path,
        &root,
        &upper,
        staging.as_deref(),
        root_short.as_deref(),
        upper_short.as_deref(),
        staging_short.as_deref(),
    )
}

/// The explicit form's body. Each base is tried long first, then under its 8.3 spelling; the answer always uses the
/// long root, which is the path the game should see.
fn map_layer_path_text(
    path: &str,
    root: &str,
    upper: &str,
    staging: Option<&str>,
    root_short: Option<&str>,
    upper_short: Option<&str>,
    staging_short: Option<&str>,
) -> Option<String> {
    if strip_prefix_ci(path, root).is_some() {
        return Some(path.to_string());
    }

    if let Some(rest) = root_short.and_then(|short| strip_prefix_ci(path, short)) {
        return Some(format!("{root}{rest}"));
    }

    for (long, short) in [(Some(upper), upper_short), (staging, staging_short)] {
        if let Some(rest) = strip_layer_str(path, long, short) {
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
        // The call returns the size needed, including the terminator, when the buffer is too small. Doubling makes every
        // retry grow, even when that size equals the buffer just tried.
        size = size.saturating_mul(2);

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

/// Writes the long guest prefix and the tail of a matched layer path. A short layer spelling still answers long, so
/// callers get one canonical guest path whatever spelling the process used. No allocation.
fn write_guest_rest(guest: &[u16], rest: &[u16], out: &mut [u16]) -> Option<usize> {
    let needed = guest.len() + rest.len();

    if needed > out.len() {
        return None;
    }

    out[..guest.len()].copy_from_slice(guest);
    out[guest.len()..needed].copy_from_slice(rest);
    Some(needed)
}

/// The guest path of the mount whose layers contain `path`, in the `\\?\` form, without building a `String`. `None` when
/// the path is outside every layer or the guest path does not fit `out`.
pub(crate) fn map_layer_path_units(path: &[u16], out: &mut [u16]) -> Option<usize> {
    let text = mount_text()?;

    for mount in text {
        // A path already in the guest's own spelling is returned as it came. The 8.3 alias takes the branch below so the
        // answer is always the long guest path, whatever the loader's spelling was.
        if strip_prefix_ci_units(path, &mount.guest_units).is_some() {
            return copy_units(path, out);
        }

        if let Some(rest) = mount
            .guest_short
            .as_ref()
            .and_then(|short| strip_prefix_ci_units(path, &short.units))
        {
            return write_guest_rest(&mount.guest_units, rest, out);
        }

        for (long, short) in [
            (
                Some(mount.upper_units.as_slice()),
                mount.upper_short.as_ref(),
            ),
            (mount.staging_units.as_deref(), mount.staging_short.as_ref()),
            (Some(mount.root_units.as_slice()), mount.root_short.as_ref()),
        ] {
            if let Some(rest) =
                strip_layer_units(path, long, short.map(|short| short.units.as_slice()))
            {
                return write_guest_rest(&mount.guest_units, rest, out);
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

/// The string form of `strip_verbatim_units`: a path without the `\\?\` prefix. A UNC volume keeps its `UNC\` marker
/// for the caller that needs the `\\` spelling.
fn strip_verbatim_str(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
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
        let Some(rest) = strip_layer_units(
            path,
            Some(&mount.guest_units),
            mount
                .guest_short
                .as_ref()
                .map(|short| short.units.as_slice()),
        ) else {
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
        if let Some(rest) = strip_layer_str(
            path,
            Some(&mount.guest_verbatim),
            mount
                .guest_short
                .as_ref()
                .map(|short| short.verbatim.as_str()),
        ) {
            return Some(format!("{}{rest}", guest_nt(mount)?));
        }
    }

    None
}

/// The NT prefixes of one mount's layers, with the guest path they map to. A layer also has its 8.3 NT spelling, because
/// a module the loader mapped by a short path can report a short NT name; the guest answer stays long in both cases.
struct MountNt {
    guest: Vec<u16>,
    guest_rel: Option<Vec<u16>>,
    upper: Vec<u16>,
    upper_rel: Option<Vec<u16>>,
    staging: Option<(Vec<u16>, Option<Vec<u16>>)>,
    root: Vec<u16>,
    root_rel: Option<Vec<u16>>,
    upper_short: Option<(Vec<u16>, Option<Vec<u16>>)>,
    staging_short: Option<(Vec<u16>, Option<Vec<u16>>)>,
    root_short: Option<(Vec<u16>, Option<Vec<u16>>)>,
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
                // The short spellings are opened here, with the other layer folders, so no call pays for them. A volume
                // without distinct 8.3 names simply has no short form to compare.
                let short_nt =
                    |short: Option<&CachedShort>| -> Option<(Vec<u16>, Option<Vec<u16>>)> {
                        let units = nt_units(Path::new(&short?.plain))?;
                        let rel = without_device(&units).map(<[u16]>::to_vec);
                        Some((units, rel))
                    };
                mounts.push(MountNt {
                    guest_rel: without_device(&guest).map(<[u16]>::to_vec),
                    upper_rel: without_device(&upper).map(<[u16]>::to_vec),
                    root_rel: without_device(&root).map(<[u16]>::to_vec),
                    guest,
                    upper,
                    staging,
                    root,
                    upper_short: short_nt(mount.upper_short.as_ref()),
                    staging_short: short_nt(mount.staging_short.as_ref()),
                    root_short: short_nt(mount.root_short.as_ref()),
                });
            }

            Some(mounts)
        })
        .as_deref()
}

/// Maps one layer's NT prefixes, long first then 8.3, to the guest NT form. No allocation for the comparison; only a
/// match joins the cached guest prefix with the tail.
fn map_nt_layer(
    units: &[u16],
    long: Option<(&[u16], Option<&[u16]>)>,
    short: Option<(&[u16], Option<&[u16]>)>,
    guest: &[u16],
    guest_rel: Option<&[u16]>,
) -> Option<Vec<u16>> {
    for (base, rel) in [long, short].into_iter().flatten() {
        if let Some(rest) = strip_prefix_ci_units(units, base) {
            return Some(join_units(guest, rest));
        }

        if let (Some(rel), Some(guest_rel)) = (rel, guest_rel)
            && let Some(rest) = strip_prefix_ci_units(units, rel)
        {
            return Some(join_units(guest_rel, rest));
        }
    }

    None
}

/// Maps an NT layer path to the NT path of the guest folder. `None` when the path is not in a layer.
pub(crate) fn map_layer_path_nt(units: &[u16]) -> Option<Vec<u16>> {
    let mounts = layer_nt()?;

    for mount in mounts {
        for (long, short) in [
            (
                Some((mount.upper.as_slice(), mount.upper_rel.as_deref())),
                mount
                    .upper_short
                    .as_ref()
                    .map(|(units, rel)| (units.as_slice(), rel.as_deref())),
            ),
            (
                mount
                    .staging
                    .as_ref()
                    .map(|(units, rel)| (units.as_slice(), rel.as_deref())),
                mount
                    .staging_short
                    .as_ref()
                    .map(|(units, rel)| (units.as_slice(), rel.as_deref())),
            ),
            (
                Some((mount.root.as_slice(), mount.root_rel.as_deref())),
                mount
                    .root_short
                    .as_ref()
                    .map(|(units, rel)| (units.as_slice(), rel.as_deref())),
            ),
        ] {
            if let Some(mapped) =
                map_nt_layer(units, long, short, &mount.guest, mount.guest_rel.as_deref())
            {
                return Some(mapped);
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
    let text = strip_verbatim_str(path);

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
    use gobbo_overlay_core::Whiteouts;
    use std::path::PathBuf;

    #[test]
    fn a_path_matched_by_upper_or_lower_is_matched_by_the_combined_filter() {
        let mount: &'static Mount = Box::leak(Box::new(Mount::new(
            "game".to_string(),
            PathBuf::from(r"C:\Game"),
            Layers::new(
                PathBuf::from(r"C:\Game"),
                Some(PathBuf::from(r"C:\Mods")),
                PathBuf::from(r"C:\OverlayUpper"),
            ),
            Whiteouts::default(),
        )));
        let mounts = [MountText::new(mount)];
        let rel = r"data\a.txt";
        let upper_path = format!(r"{}\data\a.txt", mounts[0].upper);
        let root_path = format!(r"{}\data\a.txt", mounts[0].root);
        let staging_path = format!(r"{}\data\a.txt", mounts[0].staging.as_deref().unwrap());

        let same = |left: &Option<(&'static Mount, String)>,
                    right: &Option<(&'static Mount, String)>| match (left, right)
        {
            (Some((lm, lr)), Some((rm, rr))) => std::ptr::eq(*lm, *rm) && lr == rr,
            (None, None) => true,
            _ => false,
        };
        let rel_of =
            |found: &Option<(&'static Mount, String)>| found.as_ref().map(|(_, rel)| rel.clone());

        let from_upper = relative_in_layers_from(&mounts, &upper_path, relative_in_upper_of);
        assert_eq!(rel_of(&from_upper), Some(rel.to_string()));
        assert!(
            from_upper
                .as_ref()
                .is_some_and(|(found, _)| std::ptr::eq(*found, mount))
        );
        assert!(same(
            &relative_in_layers_from(&mounts, &upper_path, relative_in_any_of),
            &from_upper
        ));
        assert!(relative_in_layers_from(&mounts, &upper_path, relative_in_lower).is_none());

        let from_root = relative_in_layers_from(&mounts, &root_path, relative_in_lower);
        assert_eq!(rel_of(&from_root), Some(rel.to_string()));
        assert!(
            from_root
                .as_ref()
                .is_some_and(|(found, _)| std::ptr::eq(*found, mount))
        );
        assert!(same(
            &relative_in_layers_from(&mounts, &root_path, relative_in_any_of),
            &from_root
        ));
        assert!(relative_in_layers_from(&mounts, &root_path, relative_in_upper_of).is_none());

        let from_staging = relative_in_layers_from(&mounts, &staging_path, relative_in_lower);
        assert_eq!(rel_of(&from_staging), Some(rel.to_string()));
        assert!(
            from_staging
                .as_ref()
                .is_some_and(|(found, _)| std::ptr::eq(*found, mount))
        );
        assert!(same(
            &relative_in_layers_from(&mounts, &staging_path, relative_in_any_of),
            &from_staging
        ));
    }

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
