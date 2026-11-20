//! One attribute query per path: found with its attributes, missing, or denied. The error code is kept, so a write can
//! fail with the code Windows gave instead of pretending the file does not exist.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use gobbo_overlay_core::join;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, GetLastError};
use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, INVALID_FILE_ATTRIBUTES};

use crate::text::wide;

/// Why a path has no file: the file is missing in a folder that exists, or a folder in the path is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Missing {
    File,
    Folder,
}

impl Missing {
    /// The error a Win32 call reports for it.
    pub(crate) const fn win32(self) -> u32 {
        match self {
            Self::File => ERROR_FILE_NOT_FOUND,
            Self::Folder => ERROR_PATH_NOT_FOUND,
        }
    }

    /// The status an NT call reports for it.
    pub(crate) const fn status(self) -> i32 {
        match self {
            Self::File => windows_sys::Win32::Foundation::STATUS_OBJECT_NAME_NOT_FOUND,
            Self::Folder => windows_sys::Win32::Foundation::STATUS_OBJECT_PATH_NOT_FOUND,
        }
    }
}

/// The result of one attribute query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Probe {
    /// The path exists; the value is the `FILE_ATTRIBUTE_*` mask.
    Found { attributes: u32 },
    /// The path or one of its folders is missing.
    Missing(Missing),
    /// Windows refused the query; the value is the Win32 error.
    Denied(u32),
}

impl Probe {
    pub(crate) const fn is_found(self) -> bool {
        matches!(self, Self::Found { .. })
    }
}

/// Queries the attributes of `path`. This is the hook's own call, so it never recurses through the hook.
pub(crate) fn probe(path: &Path) -> Probe {
    // The common path fits in one stack buffer. A long path takes the allocating `verbatim_wide` fallback, because it
    // needs the `\\?\` prefix that the plain Win32 call does not add itself.
    let mut stack = [0u16; MAX_PATH_UNITS];

    if let Some(length) = encode_units(path.as_os_str(), &mut stack) {
        return probe_units(&stack[..=length]);
    }

    let wide = verbatim_wide(path);
    probe_units(&wide)
}

/// Queries the attributes of `base\rel`. The common path is assembled in a stack buffer; a path too long for it falls
/// back to the allocating `join` and `probe`.
pub(crate) fn probe_child(base: &Path, rel: &str) -> Probe {
    let mut units = [0u16; CHILD_PATH_UNITS];

    if let Some(length) = encode_child(base.as_os_str(), rel, &mut units) {
        return probe_units(&units[..=length]);
    }

    probe(&join(base, rel))
}

/// Queries the attributes of a NUL-terminated UTF-16 path.
pub(crate) fn probe_units(units: &[u16]) -> Probe {
    crate::stat_probe();
    // SAFETY: `units` is NUL-terminated and lives until the call returns.
    let attributes = unsafe { GetFileAttributesW(units.as_ptr()) };

    if attributes != INVALID_FILE_ATTRIBUTES {
        return Probe::Found { attributes };
    }
    // SAFETY: `GetLastError` only reads the error of the calling thread.
    classify(attributes, unsafe { GetLastError() })
}

/// Writes `text` and its terminator into `buffer`, or returns `None` when it does not fit.
fn encode_units(text: &OsStr, buffer: &mut [u16]) -> Option<usize> {
    let mut length = 0;

    for unit in text.encode_wide() {
        if length >= buffer.len() - 1 {
            return None;
        }

        buffer[length] = unit;
        length += 1;
    }

    buffer[length] = 0;
    Some(length)
}

/// Writes `base\rel` and its terminator into `buffer`, or returns `None` when it does not fit. `base` keeps its own
/// trailing separator, and an empty `rel` leaves `base` as it is.
fn encode_child(base: &OsStr, rel: &str, buffer: &mut [u16]) -> Option<usize> {
    let mut length = 0;

    for unit in base.encode_wide() {
        if length >= buffer.len() - 2 {
            return None;
        }

        buffer[length] = unit;
        length += 1;
    }

    if !rel.is_empty() {
        if length > 0 && buffer[length - 1] != u16::from(b'\\') {
            if length >= buffer.len() - 2 {
                return None;
            }

            buffer[length] = u16::from(b'\\');
            length += 1;
        }

        for unit in rel.encode_utf16() {
            if length >= buffer.len() - 1 {
                return None;
            }

            buffer[length] = unit;
            length += 1;
        }
    }

    buffer[length] = 0;
    Some(length)
}

/// The answer of an attribute query, from its attributes and the error it left. A missing answer is told apart from a
/// refusal, so a write can report `ERROR_ACCESS_DENIED` instead of pretending the file is not there.
fn classify(attributes: u32, error: u32) -> Probe {
    if attributes != INVALID_FILE_ATTRIBUTES {
        return Probe::Found { attributes };
    }

    match error {
        ERROR_FILE_NOT_FOUND => Probe::Missing(Missing::File),
        ERROR_PATH_NOT_FOUND => Probe::Missing(Missing::Folder),
        _ => Probe::Denied(error),
    }
}

/// The longest path the plain Win32 calls take, counted in UTF-16 units with the terminator.
const MAX_PATH_UNITS: usize = 260;

/// The stack buffer of `probe_child`: room for the base path, the relative path and their separator.
const CHILD_PATH_UNITS: usize = MAX_PATH_UNITS * 2;

/// The path as a NUL-terminated UTF-16 string for a call that takes long paths. The attribute query does not add the
/// `\\?\` prefix itself, so a long absolute path gets it here (`\\?\UNC\` for a server), as `Path::exists` gets it.
fn verbatim_wide(path: &Path) -> Vec<u16> {
    let plain = wide(path.as_os_str());

    if plain.len() <= MAX_PATH_UNITS || !path.is_absolute() {
        return plain;
    }

    let text = path.to_string_lossy();

    if text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
        return plain;
    }

    let verbatim = match text.strip_prefix(r"\\") {
        Some(server) => format!(r"\\?\UNC\{server}"),
        None => format!(r"\\?\{text}"),
    };
    wide(OsStr::new(&verbatim))
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

    use super::*;

    #[test]
    fn an_access_refusal_keeps_the_code_windows_gave() {
        assert_eq!(
            classify(INVALID_FILE_ATTRIBUTES, ERROR_ACCESS_DENIED),
            Probe::Denied(ERROR_ACCESS_DENIED)
        );
        assert_eq!(classify(INVALID_FILE_ATTRIBUTES, 1234), Probe::Denied(1234));
        assert_eq!(
            classify(INVALID_FILE_ATTRIBUTES, ERROR_FILE_NOT_FOUND),
            Probe::Missing(Missing::File)
        );
        assert_eq!(
            classify(INVALID_FILE_ATTRIBUTES, ERROR_PATH_NOT_FOUND),
            Probe::Missing(Missing::Folder)
        );
    }

    #[test]
    fn a_child_path_is_assembled_without_a_pathbuf() {
        let base = std::env::temp_dir().join(format!("gobbo-probe-child-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        assert!(matches!(probe_child(&base, "a.txt"), Probe::Found { .. }));
        assert!(matches!(
            probe_child(&base, "missing.txt"),
            Probe::Missing(Missing::File)
        ));
        // An empty relative path asks about the folder itself.
        assert!(matches!(probe_child(&base, ""), Probe::Found { .. }));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_missing_folder_is_missing_not_denied() {
        let missing = std::env::temp_dir().join(format!(
            "gobbo-probe-missing-{}-{}",
            std::process::id(),
            "not-there"
        ));
        assert!(matches!(
            probe(&missing.join("file.txt")),
            Probe::Missing(Missing::Folder)
        ));
    }

    #[test]
    fn a_long_path_is_queried_with_the_verbatim_prefix() {
        let base = std::env::temp_dir().join(format!("gobbo-probe-long-{}", std::process::id()));
        let long = base
            .join("x".repeat(100))
            .join("y".repeat(100))
            .join("z".repeat(100));
        std::fs::create_dir_all(&long).unwrap();
        assert!(matches!(probe(&long), Probe::Found { .. }));
        let _ = std::fs::remove_dir_all(&base);
    }
}
