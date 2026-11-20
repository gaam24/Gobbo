//! The header reads the overlay needs from a Windows executable, from its bytes: the static import table and the bitness.
//! The launcher reads the file and asks this crate, so the reads are plain logic with no Windows calls and no `unsafe`,
//! and they are tested on every platform. Every offset is checked: a malformed or truncated image is an error, never a
//! panic, and an arithmetic overflow on a 32-bit build is an error too.

#![forbid(unsafe_code)]

/// One import descriptor of an executable: the DLL name and the RVA of the descriptor in the image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The name in the import table, usually a base name such as `version.dll`.
    pub name: String,
    /// The offset of the `IMAGE_IMPORT_DESCRIPTOR` from the image base.
    pub descriptor_rva: u32,
}

/// The bytes are not a PE image this crate can read: a header is missing, truncated or points outside the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

impl std::fmt::Display for Malformed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the program has a malformed PE header")
    }
}

impl std::error::Error for Malformed {}

/// The offset of `e_lfanew` in the DOS header.
const LFANEW_AT: usize = 0x3C;
/// The size of the PE signature and the file header that follows it, before the optional header.
const OPTIONAL_HEADER_AT: usize = 24;
/// The size of one import descriptor.
const DESCRIPTOR_SIZE: usize = 20;
/// The size of one section header.
const SECTION_SIZE: usize = 40;
/// The most import descriptors an image may have; a larger table is treated as malformed.
const MAX_IMPORTS: u32 = 4096;
/// The optional header magic of a PE32 image (x86) and of a PE32+ image (x64).
const MAGIC_PE32: u16 = 0x10B;
const MAGIC_PE32_PLUS: u16 = 0x20B;

/// The bitness and the imports, from one pass over the headers. One pass answers both questions, so the launcher
/// reads the file once.
///
/// # Errors
/// [`Malformed`] when a header or the import table is missing or truncated, a field points outside the image, or an
/// import name is not NUL-terminated.
pub fn parse(bytes: &[u8]) -> Result<(bool, Vec<Import>), Malformed> {
    let optional = optional_header(bytes)?;
    let magic = u16_at(bytes, optional)?;
    let (import_rva, import_size) = import_directory_at(bytes, optional, magic)?;
    Ok((
        magic == MAGIC_PE32,
        imports_from(bytes, import_rva, import_size)?,
    ))
}

/// The imports from an already located import directory.
fn imports_from(bytes: &[u8], import_rva: u32, import_size: u32) -> Result<Vec<Import>, Malformed> {
    if import_rva == 0 || import_size == 0 {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();

    for index in 0..=MAX_IMPORTS {
        let descriptor_rva = import_rva
            .checked_add(index.checked_mul(DESCRIPTOR_SIZE as u32).ok_or(Malformed)?)
            .ok_or(Malformed)?;
        let Some(descriptor) = slice(bytes, descriptor_rva, DESCRIPTOR_SIZE) else {
            break;
        };
        let name_rva = u32::from_le_bytes([
            descriptor[12],
            descriptor[13],
            descriptor[14],
            descriptor[15],
        ]);

        if name_rva == 0 {
            break;
        }

        let name = read_name(bytes, name_rva).ok_or(Malformed)?;
        result.push(Import {
            name,
            descriptor_rva,
        });
    }

    if result.len() > MAX_IMPORTS as usize {
        return Err(Malformed);
    }

    Ok(result)
}

/// The offset of the optional header: `e_lfanew` plus the PE signature and the file header.
fn optional_header(bytes: &[u8]) -> Result<usize, Malformed> {
    lfanew(bytes)?
        .checked_add(OPTIONAL_HEADER_AT)
        .ok_or(Malformed)
}

/// The offset of the PE signature, from the DOS header.
fn lfanew(bytes: &[u8]) -> Result<usize, Malformed> {
    usize::try_from(u32_at(bytes, LFANEW_AT)?).map_err(|_| Malformed)
}

/// The import directory RVA and size, from the data directory of the optional header.
fn import_directory_at(bytes: &[u8], optional: usize, magic: u16) -> Result<(u32, u32), Malformed> {
    // The import directory is the second data directory. Its offset depends on the bitness: the PE32+ optional header
    // is 16 bytes longer.
    let data_directory = match magic {
        MAGIC_PE32_PLUS => optional.checked_add(112),
        MAGIC_PE32 => optional.checked_add(96),
        _ => None,
    }
    .ok_or(Malformed)?;
    let rva = u32_at(bytes, data_directory.checked_add(8).ok_or(Malformed)?)?;
    let size = u32_at(bytes, data_directory.checked_add(12).ok_or(Malformed)?)?;
    Ok((rva, size))
}

/// A byte range of an image on disk, found through the section table from the RVA.
fn slice(bytes: &[u8], rva: u32, length: usize) -> Option<&[u8]> {
    let offset = file_offset(bytes, rva)?;
    bytes.get(offset..offset.checked_add(length)?)
}

/// The file offset of an RVA in a PE image, from the section that holds it.
fn file_offset(bytes: &[u8], rva: u32) -> Option<usize> {
    let file_header = lfanew(bytes).ok()?.checked_add(4)?;
    let sections = usize::from(u16_at(bytes, file_header.checked_add(2)?).ok()?);
    let optional_size = usize::from(u16_at(bytes, file_header.checked_add(16)?).ok()?);
    let table = file_header.checked_add(20)?.checked_add(optional_size)?;

    for index in 0..sections {
        let section = table.checked_add(index.checked_mul(SECTION_SIZE)?)?;
        let virtual_size = u32_at(bytes, section.checked_add(8)?).ok()?;
        let virtual_address = u32_at(bytes, section.checked_add(12)?).ok()?;
        let raw_size = u32_at(bytes, section.checked_add(16)?).ok()?;
        let raw = u32_at(bytes, section.checked_add(20)?).ok()?;
        let size = virtual_size.max(raw_size);

        if rva >= virtual_address && rva < virtual_address.checked_add(size)? {
            return usize::try_from(raw)
                .ok()?
                .checked_add(usize::try_from(rva - virtual_address).ok()?);
        }
    }

    None
}

/// Reads a NUL-terminated import name at `rva`.
fn read_name(bytes: &[u8], rva: u32) -> Option<String> {
    let offset = file_offset(bytes, rva)?;
    let rest = bytes.get(offset..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

macro_rules! read_le_at {
    ($name:ident, $ty:ty) => {
        /// A little-endian integer at `offset`.
        fn $name(bytes: &[u8], offset: usize) -> Result<$ty, Malformed> {
            let field: [u8; std::mem::size_of::<$ty>()] =
                slice_at(bytes, offset, std::mem::size_of::<$ty>())?
                    .try_into()
                    .map_err(|_| Malformed)?;
            Ok(<$ty>::from_le_bytes(field))
        }
    };
}

read_le_at!(u16_at, u16);
read_le_at!(u32_at, u32);

/// The bytes at `offset`, `length` of them.
fn slice_at(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], Malformed> {
    let end = offset.checked_add(length).ok_or(Malformed)?;
    bytes.get(offset..end).ok_or(Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal image: a DOS header, the PE headers, one section with the import table and the names. `pe32` chooses the
    /// bitness; `names` are the imported DLLs, in order.
    fn image(pe32: bool, names: &[&str]) -> Vec<u8> {
        const SECTION_RAW: usize = 0x200;
        const SECTION_RVA: u32 = 0x1000;
        let optional_size = if pe32 { 224 } else { 240 };
        let mut bytes = vec![0u8; SECTION_RAW];
        bytes[0] = b'M';
        bytes[1] = b'Z';
        let header = 0x80usize;
        bytes[LFANEW_AT..LFANEW_AT + 4].copy_from_slice(&(header as u32).to_le_bytes());
        bytes[header..header + 4].copy_from_slice(b"PE\0\0");
        let file_header = header + 4;
        bytes[file_header + 2..file_header + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[file_header + 16..file_header + 18]
            .copy_from_slice(&(optional_size as u16).to_le_bytes());
        let optional = header + OPTIONAL_HEADER_AT;
        let magic = if pe32 { MAGIC_PE32 } else { MAGIC_PE32_PLUS };
        bytes[optional..optional + 2].copy_from_slice(&magic.to_le_bytes());
        let directories = if pe32 { 96 } else { 112 };
        let import_directory_at = optional + directories + 8;
        let descriptors = DESCRIPTOR_SIZE * (names.len() + 1);
        bytes[import_directory_at..import_directory_at + 4]
            .copy_from_slice(&SECTION_RVA.to_le_bytes());
        bytes[import_directory_at + 4..import_directory_at + 8]
            .copy_from_slice(&(descriptors as u32).to_le_bytes());
        // The section header follows the optional header; its raw data starts at `SECTION_RAW`.
        let section = optional + optional_size;
        let mut table = vec![0u8; descriptors];
        let mut tail = Vec::new();

        for (index, name) in names.iter().enumerate() {
            let name_rva = SECTION_RVA + (descriptors + tail.len()) as u32;
            tail.extend_from_slice(name.as_bytes());
            tail.push(0);
            let at = index * DESCRIPTOR_SIZE;
            table[at + 12..at + 16].copy_from_slice(&name_rva.to_le_bytes());
        }

        let mut raw = table;
        raw.extend_from_slice(&tail);
        bytes[section..section + 8].copy_from_slice(b".idata\0\0");
        bytes[section + 8..section + 12].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        bytes[section + 12..section + 16].copy_from_slice(&SECTION_RVA.to_le_bytes());
        bytes[section + 16..section + 20].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        bytes[section + 20..section + 24].copy_from_slice(&(SECTION_RAW as u32).to_le_bytes());
        bytes.extend_from_slice(&raw);
        bytes
    }

    /// The RVA of the import table in the test image.
    const SECTION_RVA: u32 = 0x1000;
    /// The file offset where the section data starts in the test image.
    const SECTION_RAW: usize = 0x200;

    /// The two `u32` fields of the import data directory in the test image.
    fn import_directory(bytes: &[u8]) -> (usize, usize) {
        let optional = optional_header(bytes).unwrap();
        let magic = u16_at(bytes, optional).unwrap();
        let at = match magic {
            MAGIC_PE32 => optional + 96 + 8,
            _ => optional + 112 + 8,
        };
        (at, at + 4)
    }

    /// The start of the first section header in the test image.
    fn section_header(bytes: &[u8]) -> usize {
        let file_header = lfanew(bytes).unwrap() + 4;
        let optional_size = usize::from(u16_at(bytes, file_header + 16).unwrap());
        file_header + 20 + optional_size
    }

    #[test]
    fn malformed_describes_the_error() {
        assert_eq!(
            Malformed.to_string(),
            "the program has a malformed PE header"
        );
    }

    #[test]
    fn an_import_directory_with_one_zero_field_has_no_imports() {
        let bytes = image(false, &["kernel32.dll"]);
        let (rva_at, size_at) = import_directory(&bytes);
        let mut zero_rva = bytes.clone();
        zero_rva[rva_at..rva_at + 4].fill(0);
        assert_eq!(parse(&zero_rva).unwrap().1, []);
        let mut zero_size = bytes;
        zero_size[size_at..size_at + 4].fill(0);
        assert_eq!(parse(&zero_size).unwrap().1, []);
    }

    #[test]
    fn a_null_descriptor_ends_the_table() {
        let mut bytes = image(false, &["kernel32.dll", "version.dll"]);
        bytes[SECTION_RAW..SECTION_RAW + DESCRIPTOR_SIZE].fill(0);
        assert_eq!(parse(&bytes).unwrap().1, []);
    }

    #[test]
    fn a_descriptor_without_a_name_ends_the_table() {
        let mut bytes = image(false, &["kernel32.dll", "version.dll"]);
        // Keep another field non-zero so only the name RVA marks the terminator.
        bytes[SECTION_RAW] = 1;
        bytes[SECTION_RAW + 12..SECTION_RAW + 16].fill(0);
        assert_eq!(parse(&bytes).unwrap().1, []);
    }

    #[test]
    fn a_broken_name_rva_is_malformed_not_an_empty_import() {
        let mut outside = image(false, &["kernel32.dll"]);
        let name_at = SECTION_RAW + 12;
        outside[name_at..name_at + 4].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
        assert_eq!(parse(&outside), Err(Malformed));

        let mut unterminated = image(false, &["kernel32.dll"]);
        unterminated.truncate(SECTION_RAW + DESCRIPTOR_SIZE * 2 + "kernel32.dll".len());
        assert_eq!(parse(&unterminated), Err(Malformed));
    }

    #[test]
    fn an_rva_at_the_end_of_a_section_is_outside() {
        let bytes = image(false, &["kernel32.dll"]);
        let section = section_header(&bytes);
        let virtual_address = u32_at(&bytes, section + 12).unwrap();
        let virtual_size = u32_at(&bytes, section + 8).unwrap();
        let raw_size = u32_at(&bytes, section + 16).unwrap();
        assert_eq!(
            file_offset(&bytes, virtual_address + virtual_size.max(raw_size)),
            None
        );
    }

    #[test]
    fn exactly_the_limit_of_imports_is_allowed() {
        let names = vec!["a"; MAX_IMPORTS as usize];
        let found = parse(&image(false, &names)).unwrap().1;
        assert_eq!(found.len(), MAX_IMPORTS as usize);
    }

    #[test]
    fn more_imports_than_the_limit_are_malformed() {
        let names = vec!["a"; MAX_IMPORTS as usize + 1];
        assert_eq!(
            parse(&image(false, &names)).map(|(_, imports)| imports),
            Err(Malformed)
        );
    }

    #[test]
    fn the_imports_come_in_table_order_with_their_descriptors() {
        let bytes = image(false, &["kernel32.dll", "version.dll"]);
        let found = parse(&bytes).unwrap().1;
        assert_eq!(
            found,
            [
                Import {
                    name: "kernel32.dll".to_string(),
                    descriptor_rva: SECTION_RVA,
                },
                Import {
                    name: "version.dll".to_string(),
                    descriptor_rva: SECTION_RVA + 20,
                },
            ]
        );
    }

    #[test]
    fn parse_reads_the_bitness_and_the_imports_together() {
        let pe32 = parse(&image(true, &["kernel32.dll"])).unwrap();
        assert!(pe32.0);
        assert_eq!(pe32.1.len(), 1);
        assert_eq!(pe32.1[0].name, "kernel32.dll");
        let pe32_plus = parse(&image(false, &["version.dll"])).unwrap();
        assert!(!pe32_plus.0);
        assert_eq!(pe32_plus.1[0].name, "version.dll");
    }

    #[test]
    fn an_image_without_imports_has_none() {
        assert_eq!(parse(&image(false, &[])).unwrap().1, []);
    }

    #[test]
    fn a_truncated_or_unknown_image_is_malformed_not_a_panic() {
        let bytes = image(false, &["kernel32.dll"]);
        assert_eq!(
            parse(&bytes[..0x90]).map(|(_, imports)| imports),
            Err(Malformed)
        );
        let mut unknown = bytes;
        unknown[0x98..0x9A].copy_from_slice(&0x1234u16.to_le_bytes());
        assert_eq!(parse(&unknown).map(|(_, imports)| imports), Err(Malformed));
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parsing_arbitrary_or_damaged_bytes_never_panics(
                bytes in prop_oneof![
                    proptest::collection::vec(any::<u8>(), 0..4096),
                    (any::<bool>(), proptest::collection::vec((any::<usize>(), any::<u8>()), 1..16)).prop_map(
                        |(pe32, damage)| {
                            let mut bytes = image(pe32, &["kernel32.dll", "ntdll.dll"]);
                            let length = bytes.len();

                            for (at, value) in damage {
                                bytes[at % length] = value;
                            }

                            bytes
                        },
                    ),
                ],
            ) {
                let _ = parse(&bytes);
            }
        }
    }
}
