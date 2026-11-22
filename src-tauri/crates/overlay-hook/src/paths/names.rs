//! The full path of a name the program gave, and whether a name is inside the game folder.

use std::ffi::OsStr;
use std::ptr::null_mut;
use std::slice;

use gobbo_overlay_core::{fold_ascii_unit, relative_to, wide};
use windows_sys::Win32::Storage::FileSystem::GetFullPathNameW;

use super::session;
use super::target::visible;
use crate::state::{Mount, mounts};
use crate::text::same_folded_wide;

/// A path classified without the allocating full-path code.
enum Fast {
    /// The name is inside the game folder; the value is the game-relative path.
    Inside(String),
    /// The name is an absolute path outside the game folder.
    Outside,
    /// A relative, rooted, UNC-verbatim or dotted name; the caller uses the full path code.
    Fallback,
}

/// The full path of a name the program gave, as Windows resolves it. A name without a drive or a server
/// (`.\file.txt`, `file.txt`) is relative to the program's current folder; a name that starts with one backslash is
/// rooted on the current drive; a drive-relative name (`C:file.txt`) is relative to the current folder on that drive.
/// `None` for a null or empty name, or when Windows cannot resolve it.
pub(crate) fn absolute(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    // An absolute path already names its file; the layers normalize `.`/`..` themselves. Keeping it also keeps long
    // paths and verbatim (`\\?\`) paths that `GetFullPathNameW` would refuse without a prefix.
    let bytes = name.as_bytes();
    let drive_absolute = bytes.get(1) == Some(&b':') && matches!(bytes.get(2), Some(b'\\' | b'/'));

    if name.starts_with("\\\\") || drive_absolute {
        return Some(name.to_string());
    }

    let name = wide(OsStr::new(name));
    let mut size = 260;

    loop {
        let mut buffer = vec![0u16; size];
        // SAFETY: `name` is a NUL-terminated path and `buffer` holds `size` units. The call writes at most that many.
        let length = unsafe {
            GetFullPathNameW(name.as_ptr(), size as u32, buffer.as_mut_ptr(), null_mut())
        };

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

/// One name the program gave, assigned to the mount that serves it.
pub(crate) struct Place {
    pub(crate) mount: &'static Mount,
    pub(crate) rel: String,
}

/// The mount and its relative path for a name the program gave, when the name is inside one of the session's guest
/// paths. A name outside every mount gives `None` without allocating on the fast path.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn place(name: *const u16) -> Option<Place> {
    place_units(unsafe { units(name) }?)
}

/// The same as `place`, for a full path the hook already built as text.
pub(crate) fn place_path(path: &str) -> Option<Place> {
    let units: Vec<u16> = path.encode_utf16().collect();
    place_units(&units)
}

/// The same as `place`, for a name already in UTF-16 units.
pub(crate) fn place_units(units: &[u16]) -> Option<Place> {
    let mounts = mounts()?;
    let paths = session::paths()?;
    let mut fallback = false;

    for (mount, path) in mounts.all().zip(paths.all()) {
        match fast_relative(&path.guest_utf16, &path.short_guest_utf16, units) {
            Fast::Inside(rel) => return Some(Place { mount, rel }),
            Fast::Outside => {}
            Fast::Fallback => fallback = true,
        }
    }

    if !fallback {
        return None;
    }
    // A relative, rooted or drive-relative name is common in C/C++ games. Resolve it into a stack buffer and classify
    // the units against the cached prefixes; only a name inside a mount allocates the relative path.
    const STACK: usize = 512;
    let mut input = [0_u16; STACK];
    let mut output = [0_u16; STACK];
    let full: Option<&[u16]> = if units.len() < STACK {
        input[..units.len()].copy_from_slice(units);
        input[units.len()] = 0;
        full_path_units(&input[..=units.len()], &mut output)
    } else {
        None
    };

    if let Some(full) = full {
        for (mount, path) in mounts.all().zip(paths.all()) {
            match fast_relative(&path.guest_utf16, &path.short_guest_utf16, full) {
                Fast::Inside(rel) => return Some(Place { mount, rel }),
                Fast::Outside => {}
                Fast::Fallback => {
                    let text = String::from_utf16_lossy(full);

                    if let Some(rel) = relative_to(&path.guest_text, &text) {
                        return Some(Place { mount, rel });
                    }
                }
            }
        }

        return None;
    }
    // A name too long for the stack buffers keeps the old allocating path.
    let text = String::from_utf16_lossy(units);
    let path = absolute(&text)?;

    for (mount, paths) in mounts.all().zip(paths.all()) {
        if let Some(rel) = relative_to(&paths.guest_text, &path) {
            return Some(Place { mount, rel });
        }
    }

    None
}

/// Calls `GetFullPathNameW` into a caller buffer. `name` includes its NUL terminator.
fn full_path_units<'a>(name: &[u16], output: &'a mut [u16]) -> Option<&'a [u16]> {
    // SAFETY: `name` is NUL-terminated and `output` holds `output.len()` writable units. The call writes at most that
    // many and returns the length without the terminator when it fits.
    let length = unsafe {
        GetFullPathNameW(
            name.as_ptr(),
            output.len() as u32,
            output.as_mut_ptr(),
            null_mut(),
        )
    };

    if length == 0 || length as usize >= output.len() {
        return None;
    }

    Some(&output[..length as usize])
}

/// Whether a name uses a device namespace that can alias a file in the game folder. Such a name must not be treated as
/// "outside" by a write, delete or rename: `\\.\C:`, `\\.\UNC`, `\\.\Volume{...}`, `\\?\GLOBALROOT`, `\\?\Volume{...}`,
/// `\??\GLOBALROOT`, `\??\Volume{...}` and `\Device\`. A named pipe or mailslot is not a file and is not flagged.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn unsafe_name(name: *const u16) -> bool {
    unsafe { units(name) }.is_some_and(unsafe_units)
}

/// The UTF-16 form of `unsafe_name`. It is also used for NT object names after `\??\` has been kept: the caller passes
/// the units the name really has.
pub(crate) fn unsafe_units(units: &[u16]) -> bool {
    let rest = if let Some(rest) = units.strip_prefix(&[92, 92, 63, 92]) {
        rest // \\?\
    } else if let Some(rest) = units.strip_prefix(&[92, 63, 63, 92]) {
        rest // \??\
    } else if let Some(rest) = units.strip_prefix(&[92, 92, 46, 92]) {
        // \\.\
        return is_drive(rest) || starts_ci(rest, b"unc\\") || starts_ci(rest, b"volume{");
    } else if starts_ci(units, b"\\device\\") {
        // A device path is a file risk only for a volume or a disk, not for a pipe, mailslot or console.
        return is_disk_device(&units[8..]);
    } else {
        return false;
    };

    if word_ci(rest, b"globalroot") {
        let tail = &rest[10..];
        let tail = tail.strip_prefix(&[92]).unwrap_or(tail);
        return !is_named_device(tail);
    }

    starts_ci(rest, b"volume{")
}

/// Whether a `\Device\...` tail names a disk or a volume that can alias a game file.
fn is_disk_device(tail: &[u16]) -> bool {
    [
        &b"harddiskvolume"[..],
        &b"volume"[..],
        &b"physicaldrive"[..],
        &b"cdrom"[..],
        &b"floppy"[..],
    ]
    .iter()
    .any(|prefix| starts_ci(tail, prefix))
}

/// Whether a `GLOBALROOT` tail names a device that is not a file (a pipe, a mailslot or the console).
fn is_named_device(tail: &[u16]) -> bool {
    [
        &b"device\\namedpipe"[..],
        &b"device\\mailslot"[..],
        &b"device\\condrv"[..],
    ]
    .iter()
    .any(|prefix| starts_ci(tail, prefix))
}

/// A drive-relative prefix such as `C:`.
fn is_drive(units: &[u16]) -> bool {
    units.len() >= 2
        && (0x61..=0x7A).contains(&fold_ascii_unit(units[0]))
        && units[1] == u16::from(b':')
}

/// Whether `units` starts with the ASCII text, ignoring ASCII case.
fn starts_ci(units: &[u16], ascii: &[u8]) -> bool {
    units.len() >= ascii.len()
        && units[..ascii.len()]
            .iter()
            .zip(ascii)
            .all(|(unit, ascii)| fold_ascii_unit(*unit) == u16::from(*ascii))
}

/// Whether `units` starts with the ASCII word, followed by `\` or the end.
fn word_ci(units: &[u16], ascii: &[u8]) -> bool {
    starts_ci(units, ascii)
        && (units.len() == ascii.len() || units[ascii.len()] == u16::from(b'\\'))
}

/// The UTF-16 units of a NUL-terminated name, without allocating. The caller keeps the name alive for the returned
/// slice.
///
/// # Safety
/// `name` must be null or point to a NUL-terminated UTF-16 string that outlives the returned slice.
unsafe fn units<'a>(name: *const u16) -> Option<&'a [u16]> {
    if name.is_null() {
        return None;
    }

    let mut length = 0;

    while unsafe { *name.add(length) } != 0 {
        length += 1;

        if length > 32_768 {
            return None;
        }
    }

    Some(unsafe { slice::from_raw_parts(name, length) })
}

/// Classifies an absolute name against the cached long root and, when 8.3 names exist, its short form. Nothing is
/// allocated when the name is outside both.
fn fast_relative(root: &[u16], short: &[u16], units: &[u16]) -> Fast {
    if root.is_empty() {
        return Fast::Fallback;
    }
    // `/` is legal in some Windows APIs but `normal` rewrites it; leave those names to the slow path.
    if units.contains(&u16::from(b'/')) {
        return Fast::Fallback;
    }

    let units = match strip_verbatim(units) {
        Some(units) => units,
        None => return Fast::Fallback,
    };

    if !is_drive_absolute(units) && !is_unc_absolute(units) {
        return Fast::Fallback;
    }

    if let Some(rel) = under_root(root, units) {
        return relative_fast(rel);
    }

    if !short.is_empty()
        && let Some(rel) = under_root(short, units)
    {
        return relative_fast(rel);
    }

    Fast::Outside
}

/// The game-relative units when `units` is under `root`, or `None` when it is not.
fn under_root<'a>(root: &[u16], units: &'a [u16]) -> Option<&'a [u16]> {
    if units.len() < root.len() || !same_folded_wide(&units[..root.len()], root) {
        return None;
    }

    let rest = &units[root.len()..];

    if rest.is_empty() {
        return Some(&[]);
    }

    (rest[0] == u16::from(b'\\')).then(|| &rest[1..])
}

/// The relative path as `Fast::Inside`, or the slow path for a spelling that needs normalizing.
fn relative_fast(rel: &[u16]) -> Fast {
    if rel.is_empty() {
        return Fast::Inside(String::new());
    }
    // The slow path normalizes empty components, `.`, `..` and a trailing separator. A name with any of those takes
    // it too, so the fast path never answers with a spelling the layers would not see.
    if !is_plain(rel) {
        return Fast::Fallback;
    }

    Fast::Inside(String::from_utf16_lossy(rel))
}

/// The name without a `\\?\` prefix. `None` for `\\?\UNC\...`, which the slow path handles.
pub(super) fn strip_verbatim(units: &[u16]) -> Option<&[u16]> {
    if units.len() >= 4
        && units[0] == u16::from(b'\\')
        && units[1] == u16::from(b'\\')
        && units[2] == u16::from(b'?')
        && units[3] == u16::from(b'\\')
    {
        if is_unc_verbatim(units) {
            return None;
        }

        return Some(&units[4..]);
    }

    Some(units)
}

/// Whether the name starts with `\\?\UNC\`.
fn is_unc_verbatim(units: &[u16]) -> bool {
    units.len() >= 8
        && units[4..8]
            .iter()
            .zip(*b"unc\\")
            .all(|(unit, ascii)| fold_ascii_unit(*unit) == u16::from(ascii))
}

/// A drive-absolute name: `C:\...`.
fn is_drive_absolute(units: &[u16]) -> bool {
    units.len() >= 3 && is_drive(units) && units[2] == u16::from(b'\\')
}

/// A UNC name: `\\server\share\...`.
fn is_unc_absolute(units: &[u16]) -> bool {
    units.len() >= 3 && units[0] == u16::from(b'\\') && units[1] == u16::from(b'\\')
}

/// Whether a relative path is already in the form `normal` gives it: no empty component, no `.` or `..`, and no trailing
/// separator. Such a path can be compared and returned as it is.
fn is_plain(rel: &[u16]) -> bool {
    let mut start = 0;

    for index in 0..=rel.len() {
        if index == rel.len() || rel[index] == u16::from(b'\\') {
            let component = &rel[start..index];

            if component.is_empty()
                || component == [u16::from(b'.')]
                || component == [u16::from(b'.'), u16::from(b'.')]
            {
                return false;
            }

            start = index + 1;
        }
    }

    true
}

/// Whether the layers show the file that `name` names, when the name is in one of the session's mounts. The caller is
/// inside the hook.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn shown_name(name: *const u16) -> bool {
    unsafe { place(name) }.is_some_and(|place| visible(place.mount, &place.rel).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_name_has_no_full_path() {
        assert_eq!(absolute(""), None);
    }

    #[test]
    fn a_relative_name_is_resolved_against_the_current_folder() {
        let current = std::env::current_dir()
            .expect("the test process has a current folder")
            .display()
            .to_string()
            .to_lowercase();
        let full = absolute("relative.txt")
            .expect("a relative name resolves")
            .to_lowercase();
        assert!(full.ends_with("relative.txt"), "{full}");
        assert!(
            full.starts_with(&current),
            "{full} does not start with {current}"
        );
    }

    #[test]
    fn a_rooted_name_keeps_the_drive_of_the_current_folder() {
        let full = absolute(r"\Windows").expect("a rooted name resolves");
        assert!(full.ends_with(r"\Windows"), "{full}");
        assert_eq!(&full[1..3], r":\", "{full}");
    }

    #[test]
    fn an_absolute_name_is_kept_as_it_is() {
        assert_eq!(
            absolute(r"C:\Game\file.txt").as_deref(),
            Some(r"C:\Game\file.txt")
        );
        assert_eq!(
            absolute(r"\\server\share\file.txt").as_deref(),
            Some(r"\\server\share\file.txt")
        );
    }

    fn root() -> Vec<u16> {
        session::root_units(std::path::Path::new(r"C:\Game")).expect("a drive root")
    }

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn short() -> Vec<u16> {
        units(r"C:\PROGRA~1\GAME")
    }

    #[test]
    fn the_fast_path_classifies_absolute_paths_without_a_fallback() {
        let root = root();
        let short = short();
        assert!(matches!(
            fast_relative(&root, &short, &units(r"C:\Game\Data\A.txt")),
            Fast::Inside(rel) if rel == r"Data\A.txt"
        ));
        assert!(matches!(
            fast_relative(&root, &short, &units(r"c:\game\Data\A.txt")),
            Fast::Inside(rel) if rel == r"Data\A.txt"
        ));
        assert!(matches!(
            fast_relative(&root, &short, &units(r"\\?\C:\Game\Data\A.txt")),
            Fast::Inside(rel) if rel == r"Data\A.txt"
        ));
        assert!(matches!(
            fast_relative(&root, &short, &units(r"C:\Game2\Data\A.txt")),
            Fast::Outside
        ));
    }

    #[test]
    fn a_device_namespace_is_flagged_and_a_drive_or_unc_path_is_not() {
        for name in [
            r"\\.\C:\Game\a.txt",
            r"\\.\UNC\server\share\a.txt",
            r"\\.\Volume{00000000-0000-0000-0000-000000000000}\a.txt",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\a.txt",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\a.txt",
            r"\??\GLOBALROOT\Device\HarddiskVolume1\a.txt",
            r"\??\Volume{00000000-0000-0000-0000-000000000000}\a.txt",
            r"\Device\HarddiskVolume1\a.txt",
        ] {
            assert!(unsafe_units(&units(name)), "{name}");
        }

        for name in [
            r"C:\Game\a.txt",
            r"\\server\share\a.txt",
            r"\\?\C:\Game\a.txt",
            r"\??\C:\Game\a.txt",
            r"\\.\PIPE\name",
            r"\Device\NamedPipe\name",
            r"\\?\GLOBALROOT\Device\NamedPipe\name",
        ] {
            assert!(!unsafe_units(&units(name)), "{name}");
        }
    }

    #[test]
    fn the_short_name_of_the_game_folder_is_inside_too() {
        let root = root();
        let short = short();
        assert!(matches!(
            fast_relative(&root, &short, &units(r"C:\PROGRA~1\GAME\Data\A.txt")),
            Fast::Inside(rel) if rel == r"Data\A.txt"
        ));
        assert!(matches!(
            fast_relative(&root, &short, &units(r"c:\progra~1\game\a.txt")),
            Fast::Inside(rel) if rel == "a.txt"
        ));
    }

    #[test]
    fn a_dotted_verbatim_unc_or_unplain_path_uses_the_slow_path() {
        let root = root();
        let short = short();

        for name in [
            r"C:\Game\..\Other\a.txt",
            r"C:\Game\.\a.txt",
            r"C:\Game\Data\\a.txt",
            r"C:\Game\Data\",
            r"C:\Game\\",
            r"\\?\UNC\server\share\a.txt",
            r"\Game\a.txt",
        ] {
            assert!(
                matches!(fast_relative(&root, &short, &units(name)), Fast::Fallback),
                "{name}"
            );
        }
    }

    #[test]
    fn a_name_spelled_in_another_case_outside_ascii_is_inside() {
        // The mount is configured as `ś`; the program spells the same folder `Ś`, as NTFS accepts.
        let root = units(r"C:\Gra\ś");
        assert!(matches!(
            fast_relative(&root, &[], &units(r"C:\Gra\Ś\save.dat")),
            Fast::Inside(rel) if rel == "save.dat"
        ));
        assert!(matches!(
            fast_relative(&root, &[], &units(r"C:\GRA\Ś")),
            Fast::Inside(rel) if rel.is_empty()
        ));
        assert!(matches!(
            fast_relative(&root, &[], &units(r"C:\Gra\Ź\save.dat")),
            Fast::Outside
        ));
    }

    #[test]
    fn a_letter_outside_ascii_is_not_a_drive_letter() {
        // `K` (Kelvin sign) folds to `k`, but a drive is ASCII syntax: this is not drive `K:`.
        let root = units(r"K:\Game");
        assert!(!matches!(
            fast_relative(&root, &[], &units("\u{212A}:\\Game\\a.txt")),
            Fast::Inside(_)
        ));
    }

    #[test]
    fn the_game_folder_itself_is_an_empty_relative_path() {
        let root = root();
        let short = short();
        assert!(matches!(
            fast_relative(&root, &short, &units(r"C:\Game")),
            Fast::Inside(rel) if rel.is_empty()
        ));
        assert!(matches!(
            fast_relative(&root, &short, &units(r"C:\Game\")),
            Fast::Inside(rel) if rel.is_empty()
        ));
    }
}
