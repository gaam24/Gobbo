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
/// The sections kept in the local array: the Windows loader accepts at most 96. An image with more takes the heap
/// fallback, so an unusual section table still resolves the same RVAs.
const MAX_INLINE_SECTIONS: usize = 96;
/// The optional header magic of a PE32 image (x86) and of a PE32+ image (x64).
const MAGIC_PE32: u16 = 0x10B;
const MAGIC_PE32_PLUS: u16 = 0x20B;

/// Why a parse could not finish. A malformed field is decisive; running out of bytes is not, because a longer prefix
/// may still hold the rest of the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    /// The bytes are present but wrong.
    Malformed,
    /// The parser needed a range of bytes past the end of the buffer.
    Truncated,
}

/// The result of parsing a prefix that may be shorter than the whole image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixParse {
    /// The bytes decided the answer: the same one the whole image would give. `Err` means they are genuinely
    /// malformed, not merely cut off.
    Complete(Result<(bool, Vec<Import>), Malformed>),
    /// The prefix ended where the parser needed more bytes. Read a longer prefix (or the whole file) and ask again.
    NeedMore,
}

/// The bitness and the imports, from one pass over the headers. One pass answers both questions, so the launcher
/// reads the file once.
///
/// # Errors
/// [`Malformed`] when a header or the import table is missing or truncated, a field points outside the image, or an
/// import name is not NUL-terminated.
pub fn parse(bytes: &[u8]) -> Result<(bool, Vec<Import>), Malformed> {
    parse_bytes(bytes).map_err(|_| Malformed)
}

/// Parses a prefix that may end before the whole image.
///
/// [`PrefixParse::NeedMore`] only means the given bytes were not enough to decide; it says nothing about whether the
/// image is valid. A caller that gets it must read more bytes and finish with [`parse`], which still fails closed on a
/// malformed image. [`PrefixParse::Complete`] is the same answer the whole image would give.
pub fn parse_prefix(bytes: &[u8]) -> PrefixParse {
    match parse_bytes(bytes) {
        Ok(parsed) => PrefixParse::Complete(Ok(parsed)),
        Err(Problem::Malformed) => PrefixParse::Complete(Err(Malformed)),
        Err(Problem::Truncated) => PrefixParse::NeedMore,
    }
}

fn parse_bytes(bytes: &[u8]) -> Result<(bool, Vec<Import>), Problem> {
    let optional = optional_header(bytes)?;
    let magic = u16_at(bytes, optional)?;
    let (import_rva, import_size) = import_directory_at(bytes, optional, magic)?;
    // The section table is read once here and handed to every descriptor and name lookup.
    let sections = Sections::read(bytes)?;
    Ok((
        magic == MAGIC_PE32,
        imports_from(bytes, &sections, import_rva, import_size)?,
    ))
}

/// The fields of one section header needed to translate an RVA into a file offset.
#[derive(Clone, Copy)]
struct Section {
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    raw: u32,
}

/// The section table of one image. The inline array covers the sections the loader accepts; a longer table uses the
/// heap so the reads stay the same for every image. A header cut off by the end of the buffer is reported as truncated
/// rather than silently skipped, so a prefix parser never misses a section that a longer read would find.
struct Sections {
    inline: [Section; MAX_INLINE_SECTIONS],
    inline_len: usize,
    extra: Vec<Section>,
}

impl Sections {
    fn read(bytes: &[u8]) -> Result<Self, Problem> {
        let empty = Section {
            virtual_size: 0,
            virtual_address: 0,
            raw_size: 0,
            raw: 0,
        };
        let mut sections = Self {
            inline: [empty; MAX_INLINE_SECTIONS],
            inline_len: 0,
            extra: Vec::new(),
        };
        let file_header = lfanew(bytes)?.checked_add(4).ok_or(Problem::Malformed)?;
        let count = u16_at(bytes, file_header.checked_add(2).ok_or(Problem::Malformed)?)?;
        let optional_size = u16_at(
            bytes,
            file_header.checked_add(16).ok_or(Problem::Malformed)?,
        )?;
        let table = file_header
            .checked_add(20)
            .and_then(|at| at.checked_add(usize::from(optional_size)))
            .ok_or(Problem::Malformed)?;

        for index in 0..usize::from(count) {
            let at = index
                .checked_mul(SECTION_SIZE)
                .and_then(|step| table.checked_add(step))
                .ok_or(Problem::Malformed)?;
            let section = section_at(bytes, at)?;

            if index < MAX_INLINE_SECTIONS {
                sections.inline[index] = section;
                sections.inline_len += 1;
            } else {
                sections.extra.push(section);
            }
        }

        Ok(sections)
    }

    fn all(&self) -> impl Iterator<Item = &Section> {
        self.inline[..self.inline_len]
            .iter()
            .chain(self.extra.iter())
    }
}

/// The four fields of one section header at `at`.
fn section_at(bytes: &[u8], at: usize) -> Result<Section, Problem> {
    Ok(Section {
        virtual_size: u32_at(bytes, at.checked_add(8).ok_or(Problem::Malformed)?)?,
        virtual_address: u32_at(bytes, at.checked_add(12).ok_or(Problem::Malformed)?)?,
        raw_size: u32_at(bytes, at.checked_add(16).ok_or(Problem::Malformed)?)?,
        raw: u32_at(bytes, at.checked_add(20).ok_or(Problem::Malformed)?)?,
    })
}

/// The imports from an already located import directory.
fn imports_from(
    bytes: &[u8],
    sections: &Sections,
    import_rva: u32,
    import_size: u32,
) -> Result<Vec<Import>, Problem> {
    if import_rva == 0 || import_size == 0 {
        return Ok(Vec::new());
    }

    // A descriptor whose 20 bytes cross the declared end belongs to another part of the image. The section's raw data
    // may hold more descriptors after that point; they must not be patched as imports.
    let directory_end = import_rva
        .checked_add(import_size)
        .ok_or(Problem::Malformed)?;
    let mut result = Vec::new();

    for index in 0..=MAX_IMPORTS {
        let descriptor_rva = import_rva
            .checked_add(
                index
                    .checked_mul(DESCRIPTOR_SIZE as u32)
                    .ok_or(Problem::Malformed)?,
            )
            .ok_or(Problem::Malformed)?;

        if descriptor_rva
            .checked_add(DESCRIPTOR_SIZE as u32)
            .is_some_and(|end| end > directory_end)
        {
            break;
        }

        let Some(descriptor) = slice(bytes, sections, descriptor_rva, DESCRIPTOR_SIZE)? else {
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

        let name = read_name(bytes, sections, name_rva)?;
        result.push(Import {
            name,
            descriptor_rva,
        });
    }

    if result.len() > MAX_IMPORTS as usize {
        return Err(Problem::Malformed);
    }

    Ok(result)
}

/// The offset of the optional header: `e_lfanew` plus the PE signature and the file header.
fn optional_header(bytes: &[u8]) -> Result<usize, Problem> {
    if bytes.len() < 2 {
        return Err(Problem::Truncated);
    }
    if &bytes[..2] != b"MZ" {
        return Err(Problem::Malformed);
    }
    let signature = lfanew(bytes)?;
    let pe: [u8; 4] = slice_at(bytes, signature, 4)?
        .try_into()
        .map_err(|_| Problem::Malformed)?;
    if pe != *b"PE\0\0" {
        return Err(Problem::Malformed);
    }
    signature
        .checked_add(OPTIONAL_HEADER_AT)
        .ok_or(Problem::Malformed)
}

/// The offset of the PE signature, from the DOS header.
fn lfanew(bytes: &[u8]) -> Result<usize, Problem> {
    usize::try_from(u32_at(bytes, LFANEW_AT)?).map_err(|_| Problem::Malformed)
}

/// The import directory RVA and size, from the data directory of the optional header.
fn import_directory_at(bytes: &[u8], optional: usize, magic: u16) -> Result<(u32, u32), Problem> {
    // The import directory is the second data directory. Its offset depends on the bitness: the PE32+ optional header
    // is 16 bytes longer.
    let data_directory = match magic {
        MAGIC_PE32_PLUS => optional.checked_add(112),
        MAGIC_PE32 => optional.checked_add(96),
        _ => None,
    }
    .ok_or(Problem::Malformed)?;
    let rva = u32_at(
        bytes,
        data_directory.checked_add(8).ok_or(Problem::Malformed)?,
    )?;
    let size = u32_at(
        bytes,
        data_directory.checked_add(12).ok_or(Problem::Malformed)?,
    )?;
    Ok((rva, size))
}

/// A byte range of an image on disk, found through the section table from the RVA. The range must be backed by the
/// section's raw data; the virtual-only tail of a section has no bytes in the file. `None` means no section backs the
/// RVA or the range crosses the section's raw data, both of which end the import table decisively.
fn slice<'a>(
    bytes: &'a [u8],
    sections: &Sections,
    rva: u32,
    length: usize,
) -> Result<Option<&'a [u8]>, Problem> {
    let Some((offset, remaining)) = raw_at(sections, rva) else {
        return Ok(None);
    };
    if usize::try_from(remaining).map_err(|_| Problem::Malformed)? < length {
        return Ok(None);
    }
    let end = offset.checked_add(length).ok_or(Problem::Malformed)?;
    if end > bytes.len() {
        return Err(Problem::Truncated);
    }
    Ok(Some(&bytes[offset..end]))
}

/// The file offset of an RVA and the number of raw bytes left in its section. Only raw bytes are present on disk, so
/// an RVA in the zero-filled tail beyond `SizeOfRawData` is not readable from the file.
fn raw_at(sections: &Sections, rva: u32) -> Option<(usize, u32)> {
    for section in sections.all() {
        let size = section.virtual_size.max(section.raw_size);

        // A damaged section whose virtual range wraps cannot hold an RVA; keep looking in the other sections.
        let Some(end) = section.virtual_address.checked_add(size) else {
            continue;
        };

        if rva >= section.virtual_address && rva < end {
            let delta = rva - section.virtual_address;
            // The bytes in the virtual-only tail are zero-filled at load time, not present in the file. A raw read
            // requires `delta < raw_size` even though the RVA is inside the section's virtual range. Keep looking: a
            // later section may still back the same RVA with raw data.
            if delta >= section.raw_size {
                continue;
            }
            let offset = usize::try_from(section.raw)
                .ok()?
                .checked_add(usize::try_from(delta).ok()?)?;
            return Some((offset, section.raw_size - delta));
        }
    }

    None
}

/// Reads a NUL-terminated import name at `rva`, stopping at the end of the section's raw data. A name terminated
/// inside a short buffer is still decisive; one that may continue past it needs more bytes.
fn read_name(bytes: &[u8], sections: &Sections, rva: u32) -> Result<String, Problem> {
    let (offset, remaining) = raw_at(sections, rva).ok_or(Problem::Malformed)?;
    let section_end = offset
        .checked_add(usize::try_from(remaining).map_err(|_| Problem::Malformed)?)
        .ok_or(Problem::Malformed)?;
    if section_end > bytes.len() {
        let available = bytes.get(offset..).ok_or(Problem::Truncated)?;
        let end = available
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(Problem::Truncated)?;
        return Ok(String::from_utf8_lossy(&available[..end]).into_owned());
    }
    let rest = bytes.get(offset..section_end).ok_or(Problem::Malformed)?;
    let end = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(Problem::Malformed)?;
    Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
}

macro_rules! read_le_at {
    ($name:ident, $ty:ty) => {
        /// A little-endian integer at `offset`.
        fn $name(bytes: &[u8], offset: usize) -> Result<$ty, Problem> {
            let field: [u8; std::mem::size_of::<$ty>()] =
                slice_at(bytes, offset, std::mem::size_of::<$ty>())?
                    .try_into()
                    .map_err(|_| Problem::Malformed)?;
            Ok(<$ty>::from_le_bytes(field))
        }
    };
}

read_le_at!(u16_at, u16);
read_le_at!(u32_at, u32);

/// The bytes at `offset`, `length` of them. Past the buffer is truncated, not malformed: a longer read may decide.
fn slice_at(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], Problem> {
    let end = offset.checked_add(length).ok_or(Problem::Malformed)?;
    bytes.get(offset..end).ok_or(Problem::Truncated)
}

/// Minimal PE images for the tests of this crate and of its callers.
///
/// The `test-support` feature compiles the module into a dependency; it is never part of a shipped build. One builder
/// keeps the header layout in step with the parser for every consumer.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{DESCRIPTOR_SIZE, LFANEW_AT, MAGIC_PE32, MAGIC_PE32_PLUS, OPTIONAL_HEADER_AT};

    /// The RVA of the import table in a fixture image.
    pub const SECTION_RVA: u32 = 0x1000;
    /// The file offset where the section data starts in a fixture image.
    pub const SECTION_RAW: usize = 0x200;

    /// A minimal image with one section holding the import table and the names.
    ///
    /// `pe32` chooses the bitness; `names` are the imported DLLs, in order. The table ends with the all-zero descriptor
    /// of a real image, and every descriptor's bound-import fields are zero.
    pub fn image(pe32: bool, names: &[&str]) -> Vec<u8> {
        image_with_bound_imports(pe32, names, &[])
    }

    /// The same image as [`image`], with bound-import fields set on selected descriptors.
    ///
    /// `TimeDateStamp` (+4) and `ForwarderChain` (+8) are set from `bound`; a descriptor without an entry keeps both
    /// fields at zero, and entries past `names` are ignored. This is the shape an image has before the launcher clears
    /// its bound imports.
    pub fn image_with_bound_imports(pe32: bool, names: &[&str], bound: &[(u32, u32)]) -> Vec<u8> {
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
            if let Some(&(timestamp, forwarder)) = bound.get(index) {
                table[at + 4..at + 8].copy_from_slice(&timestamp.to_le_bytes());
                table[at + 8..at + 12].copy_from_slice(&forwarder.to_le_bytes());
            }
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
}

#[cfg(test)]
mod tests {
    use super::test_support::{SECTION_RAW, SECTION_RVA, image};
    use super::*;

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

    /// The section table as one `parse` would see it, for tests of the raw-coordinate helper.
    fn sections(bytes: &[u8]) -> Sections {
        Sections::read(bytes).unwrap()
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
            raw_at(
                &sections(&bytes),
                virtual_address + virtual_size.max(raw_size)
            ),
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

    /// The bound-import builder is the one the launcher uses for its duplicate-descriptor fixture. It must still make
    /// an image the parser can read, and only the descriptor named in `bound` carries the stale fields.
    #[test]
    fn a_bound_import_fixture_is_parseable_and_sets_only_the_named_descriptor() {
        let bytes = test_support::image_with_bound_imports(
            false,
            &["kernel32.dll", "version.dll"],
            &[(0x1234_5678, 0x9ABC_DEF0)],
        );
        let first = SECTION_RAW;
        let second = SECTION_RAW + DESCRIPTOR_SIZE;
        assert_eq!(&bytes[first + 4..first + 8], &0x1234_5678u32.to_le_bytes());
        assert_eq!(&bytes[first + 8..first + 12], &0x9ABC_DEF0u32.to_le_bytes());
        assert_eq!(&bytes[second + 4..second + 12], &[0u8; 8]);

        let (pe32, imports) = parse(&bytes).unwrap();
        assert!(!pe32);
        assert_eq!(imports.len(), 2);
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

    #[test]
    fn an_image_without_the_dos_and_pe_signatures_is_malformed() {
        let bytes = image(false, &["kernel32.dll"]);
        let mut no_dos = bytes.clone();
        no_dos[..2].fill(0);
        assert_eq!(parse(&no_dos), Err(Malformed));

        let mut no_pe = bytes;
        let signature = lfanew(&no_pe).unwrap();
        no_pe[signature..signature + 4].fill(0);
        assert_eq!(parse(&no_pe), Err(Malformed));
    }

    #[test]
    fn a_name_in_the_virtual_only_tail_of_a_section_is_not_read_from_later_bytes() {
        let mut bytes = image(false, &["kernel32.dll"]);
        let section = section_header(&bytes);
        let raw_size = u32_at(&bytes, section + 16).unwrap();
        // Extend the section virtually without backing the extra RVAs with raw data, and place a plausible name in
        // the bytes after the section so a read that ignores the raw extent would find it.
        bytes[section + 8..section + 12].copy_from_slice(&(raw_size + 64).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(b"fake.dll\0");
        let name_rva = SECTION_RVA + raw_size + 4;
        bytes[SECTION_RAW + 12..SECTION_RAW + 16].copy_from_slice(&name_rva.to_le_bytes());

        assert_eq!(parse(&bytes), Err(Malformed));
    }

    #[test]
    fn a_descriptor_that_runs_past_the_sections_raw_data_ends_the_table() {
        let mut bytes = image(false, &["kernel32.dll"]);
        let section = section_header(&bytes);
        let raw_size = u32_at(&bytes, section + 16).unwrap();
        let name_rva = u32_at(&bytes, SECTION_RAW + 12).unwrap();
        let directory = import_directory(&bytes).0;
        let descriptor_rva = SECTION_RVA + raw_size - 4;
        bytes[directory..directory + 4].copy_from_slice(&descriptor_rva.to_le_bytes());
        // Only four bytes of the descriptor are inside the section. Its name field would be in the appended bytes.
        let mut fake = [0u8; DESCRIPTOR_SIZE];
        fake[8..12].copy_from_slice(&name_rva.to_le_bytes());
        bytes.extend_from_slice(&fake);

        assert!(parse(&bytes).unwrap().1.is_empty());
    }

    #[test]
    fn descriptors_past_the_declared_import_directory_are_not_read() {
        let mut bytes = image(false, &["kernel32.dll", "version.dll"]);
        let (_, size_at) = import_directory(&bytes);
        // Declare room for one descriptor, though the section still holds the second one and both names.
        bytes[size_at..size_at + 4].copy_from_slice(&(DESCRIPTOR_SIZE as u32).to_le_bytes());

        let found = parse(&bytes).unwrap().1;
        assert_eq!(
            found,
            [Import {
                name: "kernel32.dll".to_string(),
                descriptor_rva: SECTION_RVA,
            }]
        );
    }

    #[test]
    fn a_virtual_only_tail_never_resolves_to_file_bytes() {
        let mut bytes = image(false, &["kernel32.dll"]);
        let section = section_header(&bytes);
        let raw_size = u32_at(&bytes, section + 16).unwrap();
        // Claim a virtual tail and put a plausible name in the file bytes after the section's raw data.
        bytes[section + 8..section + 12].copy_from_slice(&(raw_size + 128).to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(b"fake.dll\0");

        let table = sections(&bytes);
        assert_eq!(raw_at(&table, SECTION_RVA + raw_size), None);
        assert_eq!(raw_at(&table, SECTION_RVA + raw_size + 8), None);
        assert!(raw_at(&table, SECTION_RVA + raw_size - 4).is_some());
    }

    #[test]
    fn the_section_table_is_read_once_for_every_lookup() {
        let bytes = image(false, &["kernel32.dll", "version.dll"]);
        let table = sections(&bytes);
        // The descriptors and both names are all in the one section the launcher reads once.
        assert_eq!(
            raw_at(&table, SECTION_RVA).map(|(offset, _)| offset),
            Some(SECTION_RAW)
        );
        let name_rva = u32_at(&bytes, SECTION_RAW + DESCRIPTOR_SIZE + 12).unwrap();
        assert!(raw_at(&table, name_rva).is_some());
        assert_eq!(parse(&bytes).unwrap().1.len(), 2);
    }

    #[test]
    fn an_overflowing_section_does_not_hide_a_later_valid_section() {
        let mut bytes = image(false, &["kernel32.dll"]);
        let section = section_header(&bytes);
        let mut original: Vec<u8> = bytes[section..section + SECTION_SIZE].to_vec();
        // The first section's virtual range wraps around the u32 address space, so its end cannot be computed.
        bytes[section + 8..section + 12].copy_from_slice(&0x2000u32.to_le_bytes());
        bytes[section + 12..section + 16].copy_from_slice(&0xFFFF_F000u32.to_le_bytes());
        bytes[section + 16..section + 20].fill(0);
        bytes[section + 20..section + 24].fill(0);
        // The second section maps the same RVAs to the image's real raw data.
        let high_rva = 0xFFFF_F000u32;
        original[12..16].copy_from_slice(&high_rva.to_le_bytes());
        bytes[section + SECTION_SIZE..section + 2 * SECTION_SIZE].copy_from_slice(&original);
        let file_header = lfanew(&bytes).unwrap() + 4;
        bytes[file_header + 2..file_header + 4].copy_from_slice(&2u16.to_le_bytes());
        let directory = import_directory(&bytes).0;
        bytes[directory..directory + 4].copy_from_slice(&high_rva.to_le_bytes());
        let name_rva = high_rva + 2 * DESCRIPTOR_SIZE as u32;
        bytes[SECTION_RAW + 12..SECTION_RAW + 16].copy_from_slice(&name_rva.to_le_bytes());

        let found = parse(&bytes).unwrap().1;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "kernel32.dll");
    }

    #[test]
    fn a_complete_image_is_a_complete_prefix_and_matches_parse() {
        let bytes = image(false, &["kernel32.dll", "version.dll"]);
        assert_eq!(parse_prefix(&bytes), PrefixParse::Complete(parse(&bytes)));
    }

    #[test]
    fn a_prefix_cut_in_a_header_needs_more_bytes() {
        let bytes = image(false, &["kernel32.dll"]);
        // The optional header starts at 0x98, so 0x90 stops inside it.
        assert_eq!(parse_prefix(&bytes[..0x90]), PrefixParse::NeedMore);
        // The section table starts just after the optional header at 0x188.
        assert_eq!(parse_prefix(&bytes[..0x184]), PrefixParse::NeedMore);
    }

    #[test]
    fn a_prefix_cut_in_an_import_descriptor_needs_more_bytes() {
        let bytes = image(false, &["kernel32.dll"]);
        // The first descriptor starts at 0x200 and needs all 20 bytes.
        assert_eq!(parse_prefix(&bytes[..0x205]), PrefixParse::NeedMore);
    }

    #[test]
    fn a_prefix_cut_in_an_import_name_needs_more_bytes() {
        let bytes = image(false, &["kernel32.dll", "version.dll"]);
        // Both descriptors fit, but the first name ends at 0x249 and the second at 0x255.
        assert_eq!(parse_prefix(&bytes[..0x240]), PrefixParse::NeedMore);
        assert_eq!(parse_prefix(&bytes[..0x250]), PrefixParse::NeedMore);
    }

    #[test]
    fn a_name_terminated_inside_a_short_buffer_is_still_decisive() {
        let mut bytes = image(false, &["kernel32.dll"]);
        let section = section_header(&bytes);
        let raw_size = u32_at(&bytes, section + 16).unwrap();
        // Claim more raw data than the buffer holds. The name itself is terminated inside the buffer, so no longer
        // read can change it even though the section's raw extent is cut off.
        bytes[section + 16..section + 20].copy_from_slice(&(raw_size + 100).to_le_bytes());
        let expected = parse(&bytes);
        assert_eq!(parse_prefix(&bytes), PrefixParse::Complete(expected));
    }

    #[test]
    fn a_bad_signature_in_the_prefix_is_malformed_not_needs_more() {
        let mut bytes = image(false, &["kernel32.dll"]);
        bytes[..2].fill(0);
        assert_eq!(parse_prefix(&bytes), PrefixParse::Complete(Err(Malformed)));
    }

    #[test]
    fn bytes_after_the_image_do_not_change_the_prefix_parse() {
        let bytes = image(false, &["kernel32.dll", "version.dll"]);
        let image_len = bytes.len();
        let mut with_tail = bytes;
        with_tail.extend(std::iter::repeat_n(0xAB, 4096));
        let expected = parse(&with_tail);
        assert_eq!(
            parse_prefix(&with_tail[..image_len]),
            PrefixParse::Complete(expected)
        );
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
