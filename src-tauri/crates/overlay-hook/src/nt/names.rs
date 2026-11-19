//! The object names of the NT calls: the text of an `OBJECT_ATTRIBUTES` name, the full path it names, and the attributes
//! that point at the redirected name a call passes on.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

use crate::handles;
use crate::paths::{
    Failure, Place, Redirected, Target, place_path, place_units, redirect_relative_on,
    unsafe_units, visible,
};
use crate::types::{ObjectAttributes, UnicodeString};

/// The name of an object that is a file at `path`, in the `\??\` form the NT calls take, as UTF-16.
pub(super) fn nt_name_units(path: &Path) -> Vec<u16> {
    let mut units = Vec::with_capacity(4 + path.as_os_str().len());
    units.extend(r"\??\".encode_utf16());
    units.extend(path.as_os_str().encode_wide());
    units
}

/// The path of a name that is relative to a directory: the directory's path, a backslash, and the name without leading
/// backslashes.
pub(super) fn join_root(base: &str, name: &str) -> String {
    let name = name.trim_start_matches('\\');
    let mut path = String::with_capacity(base.len() + 1 + name.len());
    path.push_str(base);
    path.push('\\');
    path.push_str(name);
    path
}

/// The UTF-16 units of an NT name. A name of length zero may have a null buffer, and a slice of a null pointer is not
/// allowed even with length zero: a debug build stops the game on it.
///
/// # Safety
/// `name.buffer` must hold `name.length` bytes when it is not null.
pub(super) unsafe fn name_units(name: &UnicodeString) -> &[u16] {
    let count = usize::from(name.length) / 2;
    if name.buffer.is_null() || count == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(name.buffer, count) }
}

/// The object name of the attributes as text, or `<none>`.
///
/// # Safety
/// `attributes` must point to an `OBJECT_ATTRIBUTES` of the program, or be null.
pub(super) unsafe fn object_name(attributes: *const ObjectAttributes) -> String {
    unsafe { attributes.as_ref() }
        .and_then(|attributes| unsafe { attributes.object_name.as_ref() })
        .map(|name| String::from_utf16_lossy(unsafe { name_units(name) }))
        .unwrap_or_else(|| "<none>".to_string())
}

/// The full path an object name names, without the `\??\` prefix. A name relative to a directory handle is joined onto the
/// path of that handle as the program sees it (see `handles::program_path`). `None` for a null pointer, for a name that
/// is neither of those, and for a handle whose path Windows does not give.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`, with a valid name string.
pub(super) unsafe fn object_path(attributes: *const ObjectAttributes) -> Option<String> {
    let attributes = unsafe { attributes.as_ref()? };
    let name = unsafe { attributes.object_name.as_ref()? };
    let text = String::from_utf16_lossy(unsafe { name_units(name) });
    if let Some(path) = text.strip_prefix(r"\??\") {
        return Some(path.to_string());
    }
    if attributes.root_directory.is_null() {
        return None;
    }
    let base = handles::program_path(attributes.root_directory)?;
    Some(join_root(&base, &text))
}

/// The name without the `\??\` prefix that an absolute NT object name uses.
fn strip_nt_prefix(units: &[u16]) -> Option<&[u16]> {
    let prefix = [
        u16::from(b'\\'),
        u16::from(b'?'),
        u16::from(b'?'),
        u16::from(b'\\'),
    ];
    units.strip_prefix(&prefix)
}

/// The mount and relative path of an object name, when the name is inside one of the session's mounts. An absolute name
/// is classified straight from its UTF-16 units, so a name outside every mount costs no allocation.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`, with a valid name string.
pub(super) unsafe fn place_name(attributes: *const ObjectAttributes) -> Option<Place> {
    let attributes = unsafe { attributes.as_ref() }?;
    let name = unsafe { attributes.object_name.as_ref() }?;
    let units = unsafe { name_units(name) };
    if let Some(absolute) = strip_nt_prefix(units) {
        return place_units(absolute);
    }
    // A name relative to a directory handle outside every mount is joined onto the handle's path in stack buffers, so it
    // costs no allocation. Any other case takes the allocating path, which also maps a handle inside a mount.
    match unsafe { relative_units(attributes.root_directory, units) } {
        Some(joined) => place_units(joined.as_slice()),
        None => {
            let path = unsafe { object_path(attributes) }?;
            place_path(&path)
        }
    }
}

/// A joined path in a stack buffer. The buffer is as large as the longest path the stack classification takes.
struct Joined {
    units: [u16; JOINED],
    length: usize,
}

impl Joined {
    fn as_slice(&self) -> &[u16] {
        &self.units[..self.length]
    }
}

/// The largest joined path built on the stack.
const JOINED: usize = 512;

/// The path of an object name relative to a directory handle, joined as `join_root` joins it, in a stack buffer. `None`
/// when the handle's path is not available on the stack (`handles::outside_path_units`) or does not fit.
///
/// # Safety
/// `root` must be null or a handle of the program. `name` is the name the attributes carry.
unsafe fn relative_units(root: *mut c_void, name: &[u16]) -> Option<Joined> {
    if root.is_null() {
        return None;
    }
    let mut base = [0u16; 512];
    let base_length = handles::outside_path_units(root, &mut base)?;
    let name = trim_leading_backslashes(name);
    let length = base_length + 1 + name.len();
    if length > JOINED {
        return None;
    }
    let mut units = [0u16; JOINED];
    units[..base_length].copy_from_slice(&base[..base_length]);
    units[base_length] = u16::from(b'\\');
    units[base_length + 1..length].copy_from_slice(name);
    Some(Joined { units, length })
}

/// The name without the backslashes it starts with, as `join_root` removes them.
fn trim_leading_backslashes(name: &[u16]) -> &[u16] {
    let start = name
        .iter()
        .position(|unit| *unit != u16::from(b'\\'))
        .unwrap_or(name.len());
    &name[start..]
}

/// Whether the layers show the file that an object name names, when the name is in the game folder. The caller is
/// inside the hook.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`, with a valid name string.
pub(super) unsafe fn shown_object(attributes: *const ObjectAttributes) -> bool {
    let Some(place) = (unsafe { place_name(attributes) }) else {
        return false;
    };
    visible(place.mount, &place.rel).is_some()
}

/// The new name of an NT object, when it is in the game folder: `\??\` plus the full path of the file the layers pick.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`, with a valid name string.
pub(super) unsafe fn redirect_nt(
    attributes: *const ObjectAttributes,
    writing: bool,
    keeps_contents: bool,
) -> Option<Result<Redirected, Failure>> {
    if writing && unsafe { object_unsafe(attributes) } {
        return Some(Err(Failure::Denied(ERROR_ACCESS_DENIED)));
    }
    let place = unsafe { place_name(attributes) }?;
    let target = redirect_relative_on(place.mount, &place.rel, writing, keeps_contents);
    Some(target.map(|target| match target {
        Target::Layer { path } => Redirected::Layer {
            name: nt_name_units(&path),
        },
        // `redirect_relative` passes `same_root = false`, so this variant is not produced for NT names.
        Target::Same => Redirected::Same,
    }))
}

/// Whether an object name uses a device namespace that can alias a game file.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`, with a valid name string.
unsafe fn object_unsafe(attributes: *const ObjectAttributes) -> bool {
    let Some(attributes) = (unsafe { attributes.as_ref() }) else {
        return false;
    };
    let Some(name) = (unsafe { attributes.object_name.as_ref() }) else {
        return false;
    };
    unsafe_units(unsafe { name_units(name) })
}

/// Makes an NT call with the name the layers pick. `redirected` is that name when a layer has it, or the `Missing` error
/// when no layer has a file the call reads. `call` gets the program's attributes, or a copy of them with the new name.
/// Returns the status of the call.
///
/// # Safety
/// `attributes` must be null or point to a valid `OBJECT_ATTRIBUTES`.
pub(super) unsafe fn call_redirected(
    attributes: *const ObjectAttributes,
    redirected: Option<Result<Redirected, Failure>>,
    call: impl FnOnce(*const ObjectAttributes) -> i32,
) -> i32 {
    match redirected {
        Some(Ok(Redirected::Layer { name })) => unsafe { call_moved(attributes, &name, call) },
        // A root read from the NT path keeps the original attributes; `redirect_relative` does not produce this for NT
        // names today, but passing the call through is the safe behavior if it ever does.
        Some(Ok(Redirected::Same)) | None => call(attributes),
        Some(Err(failure)) => failure.status(),
    }
}

/// Calls `call` with the attributes whose object name is `name`. The `UnicodeString` lives on this stack frame, so no
/// heap allocation is needed for a redirect; the call is synchronous and cannot outlive it.
///
/// # Safety
/// `attributes` must point to a valid `OBJECT_ATTRIBUTES`.
unsafe fn call_moved(
    attributes: *const ObjectAttributes,
    name: &[u16],
    call: impl FnOnce(*const ObjectAttributes) -> i32,
) -> i32 {
    let bytes = (name.len() * 2) as u16;
    let unicode = UnicodeString {
        length: bytes,
        maximum_length: bytes,
        buffer: name.as_ptr(),
    };
    let mut moved = unsafe { *attributes };
    moved.object_name = &unicode;
    // The new name is absolute: the directory handle of the program must not come with it, or NT refuses the call.
    moved.root_directory = std::ptr::null_mut();
    call(&raw const moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_name_joins_onto_its_directory_without_leading_backslashes() {
        assert_eq!(
            join_root("C:\\game", "\\data\\a.txt"),
            "C:\\game\\data\\a.txt"
        );
        assert_eq!(join_root("C:\\game", "a"), "C:\\game\\a");
    }

    #[test]
    fn a_joined_name_drops_the_backslashes_it_starts_with_as_join_root_does() {
        let name: Vec<u16> = r"\\data\a.txt".encode_utf16().collect();
        assert_eq!(
            trim_leading_backslashes(&name),
            r"data\a.txt"
                .encode_utf16()
                .collect::<Vec<u16>>()
                .as_slice()
        );
        assert!(trim_leading_backslashes(&[]).is_empty());
        assert_eq!(trim_leading_backslashes(&[u16::from(b'\\')]), &[] as &[u16]);
    }

    #[test]
    fn a_path_becomes_the_nt_name_with_the_prefix() {
        assert_eq!(
            nt_name_units(Path::new("C:\\x")),
            "\\??\\C:\\x".encode_utf16().collect::<Vec<u16>>()
        );
    }
}
