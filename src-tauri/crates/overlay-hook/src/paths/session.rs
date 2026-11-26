//! The cached guest paths of a session: the game folder and every user-data redirect, each with the UTF-16 prefixes
//! the hook compares against and its 8.3 alias. Built once from the session mounts; readers never rebuild a prefix for
//! each call.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gobbo_overlay_core::wide;
use windows_sys::Win32::Storage::FileSystem::{GetLongPathNameW, GetShortPathNameW};

use crate::state::mounts;

/// One real layer path and its cached spellings. A program that learned a layer path (a loader's own module path, a
/// config override) is classified as if it had used the guest path, so its reads and writes stay in the virtual view.
pub(crate) struct LayerPrefix {
    /// The layer path as text, built once for the slow path.
    pub(crate) text: String,
    /// The prefix the fast path compares against, without a trailing separator.
    pub(crate) utf16: Vec<u16>,
    /// The 8.3 form, when Windows gives one that differs from the long form.
    pub(crate) short_utf16: Vec<u16>,
}

impl LayerPrefix {
    fn new(path: &Path) -> Self {
        Self {
            text: path.to_string_lossy().into_owned(),
            utf16: root_units(path).unwrap_or_default(),
            short_utf16: short_path_units(path).unwrap_or_default(),
        }
    }
}

/// One guest path of the session and the layers that serve it.
pub(crate) struct MountPaths {
    /// The guest path as text, built once for the slow path.
    pub(crate) guest_text: String,
    /// The guest prefix the fast path compares against, without a trailing separator.
    pub(crate) guest_utf16: Vec<u16>,
    /// The 8.3 form of the guest, when Windows gives one that differs from the long form.
    pub(crate) short_guest_utf16: Vec<u16>,
    /// The real lower layer the game folder reads.
    pub(crate) root: LayerPrefix,
    /// The real mod layer, when the mount has one.
    pub(crate) staging: Option<LayerPrefix>,
    /// The real layer the game's writes go to.
    pub(crate) upper: LayerPrefix,
}

/// The guest paths of one overlay session. The game is always first.
pub(crate) struct SessionPaths {
    pub(crate) game: MountPaths,
    pub(crate) redirects: Vec<MountPaths>,
}

impl SessionPaths {
    /// The game followed by the redirects, in the order the hook checks them.
    pub(crate) fn all(&self) -> impl Iterator<Item = &MountPaths> {
        std::iter::once(&self.game).chain(self.redirects.iter())
    }
}

static SESSION: OnceLock<Option<SessionPaths>> = OnceLock::new();

/// The cached paths of the session, or `None` when the environment named no layers.
pub(crate) fn paths() -> Option<&'static SessionPaths> {
    SESSION
        .get_or_init(|| {
            let mounts = mounts()?;
            Some(SessionPaths {
                game: MountPaths::new(&mounts.game),
                redirects: mounts.redirects.iter().map(MountPaths::new).collect(),
            })
        })
        .as_ref()
}

impl MountPaths {
    fn new(mount: &crate::state::Mount) -> Self {
        Self {
            guest_text: mount.guest.to_string_lossy().into_owned(),
            guest_utf16: root_units(&mount.guest).unwrap_or_default(),
            short_guest_utf16: short_path_units(&mount.guest).unwrap_or_default(),
            root: LayerPrefix::new(&mount.layers.root),
            staging: mount.layers.staging.as_deref().map(LayerPrefix::new),
            upper: LayerPrefix::new(&mount.layers.upper),
        }
    }

    /// The real lower and write layers of this mount, in the order a name is checked after the guest prefix.
    pub(crate) fn alias_layers(&self) -> impl Iterator<Item = &LayerPrefix> {
        std::iter::once(&self.root)
            .chain(self.staging.iter())
            .chain(std::iter::once(&self.upper))
    }
}

/// The UTF-16 prefix for a path: the `\\?\` prefix is removed, a trailing separator is dropped, and a form the fast path
/// does not compare (`\\?\UNC\`, `/`) gives `None`.
pub(crate) fn root_units(root: &Path) -> Option<Vec<u16>> {
    let mut units = wide(root.as_os_str());
    units.pop();
    let stripped = super::names::strip_verbatim(&units)?;
    let mut units = stripped.to_vec();

    if units.contains(&u16::from(b'/')) {
        return None;
    }

    while units.len() > 2 && units.last() == Some(&u16::from(b'\\')) {
        units.pop();
    }

    Some(units)
}

/// The long-name form of a path. Programs may spell a folder with 8.3 names (`RUNNER~1`) while the Windows APIs that
/// name handles and modules report the long form (`runneradmin`). Mount prefixes must use that same form or no handle
/// is recognized as a layer handle. A leaf that does not exist yet is resolved as far as its nearest existing parent
/// and the missing tail is appended; a path Windows cannot resolve at all is kept as it came.
pub(crate) fn long_path(path: &Path) -> PathBuf {
    long_name(path).unwrap_or_else(|| path.to_path_buf())
}

/// Resolves `path` with `GetLongPathNameW`, walking up over missing leaves. `None` when even the volume is unusable.
fn long_name(path: &Path) -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    let mut missing: Vec<PathBuf> = Vec::new();
    let mut current = path;

    loop {
        let name = wide(current.as_os_str());
        // SAFETY: `name` is NUL-terminated, and the buffer holds 1024 writable units.
        let length =
            unsafe { GetLongPathNameW(name.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        if length > 0 && (length as usize) < buffer.len() {
            let mut result = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));

            for part in missing.iter().rev() {
                result.push(part);
            }

            return Some(result);
        }

        missing.push(PathBuf::from(current.file_name()?));
        current = current.parent()?;
    }
}

/// The 8.3 form of a path as plain UTF-16 units, when it exists and differs from the long form. `None` when the volume
/// has 8.3 names disabled (Windows returns the long form) or the path cannot be shortened. Both this module's name
/// classification and the handle mapping cache it at install so neither has to ask Windows during a call.
pub(crate) fn short_path_units(root: &Path) -> Option<Vec<u16>> {
    let mut buffer = [0u16; 1024];
    let name = wide(root.as_os_str());
    // SAFETY: `name` is NUL-terminated, and the buffer holds 1024 writable units.
    let length =
        unsafe { GetShortPathNameW(name.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        return None;
    }

    let path = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
    let short = root_units(&path)?;
    (short != root_units(root)?).then_some(short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drive_root_loses_its_trailing_separator_for_comparison() {
        assert_eq!(
            root_units(Path::new(r"C:\Game\")).unwrap(),
            r"C:\Game".encode_utf16().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_verbatim_drive_path_is_compared_without_the_prefix() {
        assert_eq!(
            root_units(Path::new(r"\\?\C:\Game")).unwrap(),
            r"C:\Game".encode_utf16().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_verbatim_unc_or_slash_path_uses_the_slow_path() {
        assert!(root_units(Path::new(r"\\?\UNC\server\share")).is_none());
        assert!(root_units(Path::new("C:/Game")).is_none());
    }
}
