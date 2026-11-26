//! The information structure of a rename, `FILE_RENAME_INFORMATION` and its `_EX` form: what the hook reads from the bytes
//! the program passes, and the same structure with the upper-layer name in it.

use super::layout::{
    FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_EX, FILE_RENAME_REPLACE_IF_EXISTS,
    RENAME_LENGTH_AT, RENAME_NAME_AT, RENAME_ROOT_AT, RENAME_ROOT_SIZE,
};

/// The parts of a rename that the hook reads: whether it replaces an existing name, the root directory handle the new
/// name is relative to (`0` when there is none), and the new name.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RenameInfo {
    pub(super) replace: bool,
    pub(super) root: usize,
    pub(super) name: String,
}

/// Reads a rename from the bytes the program passed for `class`. `None` when the class is not a rename, or when the bytes
/// are too short for the name they give.
pub(super) fn parse_rename(bytes: &[u8], class: u32) -> Option<RenameInfo> {
    if !matches!(class, FILE_RENAME_INFORMATION | FILE_RENAME_INFORMATION_EX)
        || bytes.len() < RENAME_NAME_AT
    {
        return None;
    }

    let flags = u32::from_ne_bytes(bytes[..4].try_into().ok()?);
    // In the plain form the first byte is `ReplaceIfExists`; in the `_EX` form it is a flag of the flags word.
    let replace = if class == FILE_RENAME_INFORMATION_EX {
        flags & FILE_RENAME_REPLACE_IF_EXISTS != 0
    } else {
        bytes[0] != 0
    };
    let root_bytes: [u8; std::mem::size_of::<usize>()] = bytes
        [RENAME_ROOT_AT..RENAME_ROOT_AT + RENAME_ROOT_SIZE]
        .try_into()
        .ok()?;
    let root = usize::from_ne_bytes(root_bytes);
    let name_bytes = u32::from_ne_bytes(
        bytes[RENAME_LENGTH_AT..RENAME_LENGTH_AT + 4]
            .try_into()
            .ok()?,
    ) as usize;
    // A caller can put any length in the structure. On x86 the addition can wrap, which would let a huge length past
    // the check and panic on the slice below; refuse it instead.
    let end = RENAME_NAME_AT.checked_add(name_bytes)?;

    if end > bytes.len() {
        return None;
    }

    let (pairs, _) = bytes[RENAME_NAME_AT..end].as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_ne_bytes(*pair)).collect();
    Some(RenameInfo {
        replace,
        root,
        name: String::from_utf16_lossy(&units),
    })
}

/// The rename structure that passes on `name`: the flags of `header` (its first four bytes), no root directory, and the
/// new name. The name is absolute, so NT takes it without a root. `None` when the name cannot be counted in a
/// `UNICODE_STRING` byte length without overflowing or truncating it.
pub(super) fn build_rename(header: &[u8], name: &[u16]) -> Option<Vec<u8>> {
    let bytes = name.len().checked_mul(2)?;
    let length = u32::try_from(bytes).ok()?;
    let mut renamed = vec![0u8; RENAME_NAME_AT.checked_add(bytes)?];
    renamed[..4].copy_from_slice(&header[..4]);
    renamed[RENAME_LENGTH_AT..RENAME_LENGTH_AT + 4].copy_from_slice(&length.to_ne_bytes());
    let (pairs, _) = renamed[RENAME_NAME_AT..].as_chunks_mut::<2>();

    for (at, unit) in pairs.iter_mut().zip(name) {
        *at = unit.to_ne_bytes();
    }

    Some(renamed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nt::layout::FILE_DISPOSITION_INFORMATION;

    /// The structure as the program passes it: the flags, the root handle, and the name after them.
    fn structure(flags: u32, root: usize, name: &str) -> Vec<u8> {
        let units: Vec<u8> = name.encode_utf16().flat_map(u16::to_ne_bytes).collect();
        let mut bytes = vec![0u8; RENAME_NAME_AT];
        bytes[..4].copy_from_slice(&flags.to_ne_bytes());
        bytes[RENAME_ROOT_AT..RENAME_ROOT_AT + RENAME_ROOT_SIZE]
            .copy_from_slice(&root.to_ne_bytes());
        bytes[RENAME_LENGTH_AT..RENAME_LENGTH_AT + 4]
            .copy_from_slice(&(units.len() as u32).to_ne_bytes());
        bytes.extend(units);
        bytes
    }

    #[test]
    fn the_extended_form_reads_replace_from_the_flags() {
        let replacing = structure(FILE_RENAME_REPLACE_IF_EXISTS, 0, "x");
        assert!(
            parse_rename(&replacing, FILE_RENAME_INFORMATION_EX)
                .unwrap()
                .replace
        );
        let plain = structure(0, 0, "x");
        assert!(
            !parse_rename(&plain, FILE_RENAME_INFORMATION_EX)
                .unwrap()
                .replace
        );
    }

    #[test]
    fn the_plain_form_reads_replace_from_its_first_byte() {
        assert!(
            parse_rename(&structure(1, 0, "x"), FILE_RENAME_INFORMATION)
                .unwrap()
                .replace
        );
        assert!(
            !parse_rename(&structure(0, 0, "x"), FILE_RENAME_INFORMATION)
                .unwrap()
                .replace
        );
    }

    #[test]
    fn the_root_and_the_name_are_read_as_they_are() {
        let parsed = parse_rename(&structure(0, 0x1234, "dir\\file"), FILE_RENAME_INFORMATION);
        assert_eq!(
            parsed,
            Some(RenameInfo {
                replace: false,
                root: 0x1234,
                name: "dir\\file".to_string(),
            })
        );
    }

    #[test]
    fn a_name_longer_than_the_bytes_or_another_class_is_refused() {
        let mut short = structure(0, 0, "abc");
        short.truncate(short.len() - 2);
        assert_eq!(parse_rename(&short, FILE_RENAME_INFORMATION), None);
        let disposition = structure(0, 0, "a");
        assert_eq!(
            parse_rename(&disposition, FILE_DISPOSITION_INFORMATION),
            None
        );
        assert_eq!(parse_rename(&[0u8; 4], FILE_RENAME_INFORMATION), None);
    }

    #[test]
    fn a_maximum_name_length_in_a_short_buffer_is_refused_without_wrapping() {
        let mut bytes = structure(0, 0, "abc");
        bytes[RENAME_LENGTH_AT..RENAME_LENGTH_AT + 4].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert_eq!(parse_rename(&bytes, FILE_RENAME_INFORMATION), None);
        bytes[RENAME_LENGTH_AT..RENAME_LENGTH_AT + 4]
            .copy_from_slice(&(u32::MAX - 3).to_ne_bytes());
        assert_eq!(parse_rename(&bytes, FILE_RENAME_INFORMATION_EX), None);
    }

    #[test]
    fn a_built_rename_reads_back_with_the_new_name_and_no_root() {
        let header = structure(FILE_RENAME_REPLACE_IF_EXISTS, 0x99, "old");
        let new_name: Vec<u16> = "\\??\\C:\\new".encode_utf16().collect();
        let built = build_rename(&header, &new_name).expect("the name fits in the structure");
        assert_eq!(
            parse_rename(&built, FILE_RENAME_INFORMATION_EX),
            Some(RenameInfo {
                replace: true,
                root: 0,
                name: "\\??\\C:\\new".to_string(),
            })
        );
    }
}
