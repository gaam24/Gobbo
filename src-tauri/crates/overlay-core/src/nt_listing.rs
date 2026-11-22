//! The bytes of an NT directory listing: the `FILE_*_INFORMATION` entries a program reads through `NtQueryDirectoryFile`.
//! This module takes the fields of an entry as plain values and writes the structures the program expects, so the
//! offsets are tested on every platform. The Windows hook reads the listing it holds, passes each entry in, and copies
//! the buffer the program gave it. Nothing here allocates or calls Windows.

/// `FILE_DIRECTORY_INFORMATION`.
pub const DIRECTORY: u32 = 1;
/// `FILE_FULL_DIRECTORY_INFORMATION`.
pub const FULL_DIRECTORY: u32 = 2;
/// `FILE_BOTH_DIRECTORY_INFORMATION`.
pub const BOTH_DIRECTORY: u32 = 3;
/// `FILE_NAMES_INFORMATION`.
pub const NAMES: u32 = 12;
/// `FILE_ID_BOTH_DIRECTORY_INFORMATION`.
pub const ID_BOTH_DIRECTORY: u32 = 37;
/// `FILE_ID_FULL_DIRECTORY_INFORMATION`.
pub const ID_FULL_DIRECTORY: u32 = 38;

/// The longest 8.3 name the formatter writes, in UTF-16 units.
const SHORT_NAME_UNITS: usize = 12;

/// One entry of a listing. The names are UTF-16 without a terminator; `short_name` is empty when the file has none.
/// The times are `FILETIME` values (100-nanosecond units since 1601), and `size` is the file size in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry<'a> {
    /// The name as the listing spells it.
    pub name: &'a [u16],
    /// The 8.3 name, or empty.
    pub short_name: &'a [u16],
    /// The file attributes.
    pub attributes: u32,
    /// The file size in bytes.
    pub size: u64,
    /// The creation time.
    pub created: u64,
    /// The last access time.
    pub accessed: u64,
    /// The last write time; it is also the change time, as a listing of a folder reports it.
    pub written: u64,
}

/// What `fill` did with a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// The class is not one this module writes.
    UnknownClass,
    /// No entry is left from the start index.
    End,
    /// The first entry does not fit the buffer, so nothing is written and nothing is consumed.
    Overflow,
    /// `consumed` entries were written; the answer is the first `bytes` bytes of the buffer.
    Written { bytes: usize, consumed: usize },
}

/// Whether this module writes `class`.
pub fn knows(class: u32) -> bool {
    layout(class).is_some()
}

/// Writes entries into `buffer` from index `start`, in the structure of `class`. `entry(index)` gives the fields of entry
/// `index` for `index < count`. With `single`, one entry is written and no next-entry offset is set, as
/// `SL_RETURN_SINGLE_ENTRY` asks. Every entry is aligned to eight bytes, and the last one has no successor.
pub fn fill<'a>(
    class: u32,
    count: usize,
    entry: impl Fn(usize) -> Entry<'a>,
    start: usize,
    buffer: &mut [u8],
    single: bool,
) -> Fill {
    if start >= count {
        return Fill::End;
    }

    let Some(layout) = layout(class) else {
        return Fill::UnknownClass;
    };
    let mut offset = 0;
    let mut index = start;
    let mut previous: Option<(usize, usize)> = None;

    while index < count {
        let data = entry(index);
        let size = layout.name + 2 * data.name.len();

        if offset + size > buffer.len() {
            break;
        }

        if let Some((at, _)) = previous {
            write_u32(buffer, at, (offset - at) as u32);
        }

        write_entry(&layout, &data, &mut buffer[offset..offset + size]);
        previous = Some((offset, size));
        offset += align8(size);
        index += 1;

        if single {
            break;
        }
    }

    if index == start {
        return Fill::Overflow;
    }
    // The last entry has no successor; the padding after it is not part of the answer.
    let bytes = previous.map_or(0, |(at, size)| at + size);
    Fill::Written {
        bytes,
        consumed: index - start,
    }
}

/// The offsets of the fields of one structure. `None` means the structure has no such field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    /// Where the name starts.
    name: usize,
    /// Where `FileNameLength` is (in bytes).
    name_length: usize,
    /// Whether the entry carries the metadata fields. The names-only class has none: its `FileNameLength` is the
    /// last field before the name.
    metadata: bool,
    ea: Option<usize>,
    short_name: Option<usize>,
    short_name_length: Option<usize>,
    file_id: Option<usize>,
}

/// The layout the metadata-carrying structures share.
const COMMON_LAYOUT: Layout = Layout {
    name: 64,
    name_length: 60,
    metadata: true,
    ea: None,
    short_name: None,
    short_name_length: None,
    file_id: None,
};

/// The layout of every class this module writes. The offsets are the ones of the Windows structures, which are the same
/// on x86 and x64.
const LAYOUTS: [(u32, Layout); 6] = [
    (DIRECTORY, COMMON_LAYOUT),
    (
        FULL_DIRECTORY,
        Layout {
            name: 68,
            ea: Some(64),
            ..COMMON_LAYOUT
        },
    ),
    (
        BOTH_DIRECTORY,
        Layout {
            name: 94,
            ea: Some(64),
            short_name: Some(70),
            short_name_length: Some(68),
            ..COMMON_LAYOUT
        },
    ),
    (
        NAMES,
        Layout {
            name: 12,
            name_length: 8,
            metadata: false,
            ea: None,
            short_name: None,
            short_name_length: None,
            file_id: None,
        },
    ),
    (
        ID_FULL_DIRECTORY,
        Layout {
            name: 80,
            ea: Some(64),
            file_id: Some(72),
            ..COMMON_LAYOUT
        },
    ),
    (
        ID_BOTH_DIRECTORY,
        Layout {
            name: 104,
            ea: Some(64),
            short_name: Some(70),
            short_name_length: Some(68),
            file_id: Some(96),
            ..COMMON_LAYOUT
        },
    ),
];

/// The layout of `class`, or `None` when this module does not write it.
fn layout(class: u32) -> Option<Layout> {
    LAYOUTS
        .iter()
        .find(|(known, _)| *known == class)
        .map(|(_, layout)| *layout)
}

/// Writes one entry. `out` is exactly the entry's size; its next-entry offset is zero until the caller writes another
/// entry after it.
fn write_entry(layout: &Layout, data: &Entry<'_>, out: &mut [u8]) {
    out.fill(0);
    write_u32(out, layout.name_length, (2 * data.name.len()) as u32);
    if layout.metadata {
        write_u64(out, 8, data.created);
        write_u64(out, 16, data.accessed);
        write_u64(out, 24, data.written);
        write_u64(out, 32, data.written);
        write_u64(out, 40, data.size);
        write_u32(out, 56, data.attributes);
    }

    if let Some(at) = layout.ea {
        write_u32(out, at, 0);
    }

    if let (Some(at), Some(length_at)) = (layout.short_name, layout.short_name_length) {
        let length = data.short_name.len().min(SHORT_NAME_UNITS);
        // `ShortNameLength` is in bytes, like every length in the NT structures.
        out[length_at] = (length * 2) as u8;

        for (slot, unit) in data.short_name.iter().take(length).enumerate() {
            write_u16(out, at + slot * 2, *unit);
        }
    }

    if let Some(at) = layout.file_id {
        write_u64(out, at, 0);
    }

    for (slot, unit) in data.name.iter().enumerate() {
        write_u16(out, layout.name + slot * 2, *unit);
    }
}

/// The eight-byte boundary after `value`: every entry of a listing starts on one.
const fn align8(value: usize) -> usize {
    (value + 7) & !7
}

macro_rules! write_le {
    ($name:ident, $ty:ty) => {
        fn $name(out: &mut [u8], at: usize, value: $ty) {
            out[at..at + std::mem::size_of::<$ty>()].copy_from_slice(&value.to_le_bytes());
        }
    };
}

write_le!(write_u16, u16);
write_le!(write_u32, u32);
write_le!(write_u64, u64);

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn entry_of(name: &[u16]) -> Entry<'_> {
        Entry {
            name,
            short_name: &[],
            attributes: 0x20,
            size: 7,
            created: 1,
            accessed: 2,
            written: 3,
        }
    }

    fn fill_directory(names: &[Vec<u16>], start: usize, buffer: &mut [u8], single: bool) -> Fill {
        fill(
            DIRECTORY,
            names.len(),
            |index| entry_of(&names[index]),
            start,
            buffer,
            single,
        )
    }

    #[test]
    fn only_the_listed_classes_are_known() {
        for class in [
            DIRECTORY,
            FULL_DIRECTORY,
            BOTH_DIRECTORY,
            NAMES,
            ID_BOTH_DIRECTORY,
            ID_FULL_DIRECTORY,
        ] {
            assert!(knows(class));
        }

        assert!(!knows(0));
        assert!(!knows(4));
        assert!(!knows(39));
    }

    #[test]
    fn a_directory_entry_carries_its_name_size_and_times() {
        let name = units("a.txt");
        let mut out = vec![0u8; 64 + 10];
        let layout = layout(DIRECTORY).expect("a known class");
        write_entry(&layout, &entry_of(&name), &mut out);
        assert_eq!(u32::from_le_bytes(out[60..64].try_into().unwrap()), 10);
        assert_eq!(u64::from_le_bytes(out[40..48].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(out[56..60].try_into().unwrap()), 0x20);
        assert_eq!(u64::from_le_bytes(out[24..32].try_into().unwrap()), 3);
        assert_eq!(
            &out[64..74],
            "a.txt"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_id_both_entry_puts_the_name_where_windows_does() {
        let name = units("b.bin");
        let layout = layout(ID_BOTH_DIRECTORY).expect("a known class");
        let mut out = vec![0u8; 104 + 10];
        write_entry(&layout, &entry_of(&name), &mut out);
        assert_eq!(
            &out[104..114],
            "b.bin"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_both_entry_carries_the_short_name_in_byte_length() {
        let name = units("LongName.txt");
        let short = units("LONGNA~1.TXT");
        let layout = layout(BOTH_DIRECTORY).expect("a known class");
        let data = Entry {
            short_name: &short,
            ..entry_of(&name)
        };
        let mut out = vec![0u8; 94 + 2 * name.len()];
        write_entry(&layout, &data, &mut out);
        assert_eq!(out[68], (short.len() * 2) as u8);
        let expected: Vec<u8> = short.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        assert_eq!(&out[70..70 + expected.len()], expected.as_slice());
    }

    #[test]
    fn a_names_only_entry_has_no_metadata() {
        let name = units("a.txt");
        let layout = layout(NAMES).expect("a known class");
        let mut out = vec![0u8; 12 + 10];
        write_entry(&layout, &entry_of(&name), &mut out);
        assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 10);
        assert_eq!(
            &out[12..22],
            "a.txt"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_single_entry_is_returned_without_a_next_offset() {
        let names = [units("a"), units("bb")];
        let mut buffer = [0u8; 512];
        let fill = fill_directory(&names, 0, &mut buffer, true);
        assert_eq!(
            fill,
            Fill::Written {
                bytes: 66,
                consumed: 1
            }
        );
        assert_eq!(u32::from_le_bytes(buffer[0..4].try_into().unwrap()), 0);
    }

    #[test]
    fn entries_are_chained_and_the_last_one_ends_the_list() {
        let names = [units("a"), units("bb")];
        let mut buffer = [0u8; 512];
        let fill = fill_directory(&names, 0, &mut buffer, false);
        assert_eq!(
            fill,
            Fill::Written {
                bytes: 72 + 68,
                consumed: 2
            }
        );
        let next = u32::from_le_bytes(buffer[0..4].try_into().unwrap()) as usize;
        assert_eq!(next, 72);
        assert_eq!(
            u32::from_le_bytes(buffer[next..next + 4].try_into().unwrap()),
            0
        );
    }

    #[test]
    fn a_buffer_too_small_for_one_entry_overflows_and_consumes_nothing() {
        let names = [units("a")];
        let mut buffer = [0u8; 32];
        let fill = fill_directory(&names, 0, &mut buffer, false);
        assert_eq!(fill, Fill::Overflow);
    }

    #[test]
    fn the_end_and_an_unknown_class_are_reported_before_any_write() {
        let names = [units("a")];
        let mut buffer = [0u8; 512];
        assert_eq!(fill_directory(&names, 1, &mut buffer, false), Fill::End);
        assert_eq!(
            fill(
                99,
                names.len(),
                |index| entry_of(&names[index]),
                0,
                &mut buffer,
                false
            ),
            Fill::UnknownClass
        );
    }

    #[test]
    fn every_layout_matches_the_windows_structure() {
        let expected = [
            (DIRECTORY, (64, 60, true, None, None, None, None)),
            (FULL_DIRECTORY, (68, 60, true, Some(64), None, None, None)),
            (
                BOTH_DIRECTORY,
                (94, 60, true, Some(64), Some(70), Some(68), None),
            ),
            (NAMES, (12, 8, false, None, None, None, None)),
            (
                ID_FULL_DIRECTORY,
                (80, 60, true, Some(64), None, None, Some(72)),
            ),
            (
                ID_BOTH_DIRECTORY,
                (104, 60, true, Some(64), Some(70), Some(68), Some(96)),
            ),
        ];

        for (class, fields) in expected {
            let layout = layout(class).expect("a known class");
            assert_eq!(
                (
                    layout.name,
                    layout.name_length,
                    layout.metadata,
                    layout.ea,
                    layout.short_name,
                    layout.short_name_length,
                    layout.file_id,
                ),
                fields,
                "class {class}"
            );
        }
        assert_eq!(layout(0), None);
    }

    #[test]
    fn an_entry_that_fits_exactly_is_written() {
        let name = units("a");
        let mut buffer = vec![0u8; 66];
        let fill = fill_directory(std::slice::from_ref(&name), 0, &mut buffer, false);
        assert_eq!(
            fill,
            Fill::Written {
                bytes: 66,
                consumed: 1
            }
        );
    }

    #[test]
    fn every_next_offset_is_measured_from_the_entry_before_it() {
        let names = [units("a"), units("b"), units("c")];
        let mut buffer = [0u8; 512];
        let fill = fill_directory(&names, 0, &mut buffer, false);
        assert_eq!(
            fill,
            Fill::Written {
                bytes: 72 + 72 + 66,
                consumed: 3
            }
        );
        let first = u32::from_le_bytes(buffer[0..4].try_into().unwrap()) as usize;
        let second = u32::from_le_bytes(buffer[first..first + 4].try_into().unwrap()) as usize;
        assert_eq!(first, 72);
        assert_eq!(second, 72);
    }

    #[test]
    fn a_start_index_is_reported_as_the_number_of_written_entries() {
        let names = [units("a"), units("b")];
        let mut buffer = [0u8; 512];
        let fill = fill_directory(&names, 1, &mut buffer, false);
        assert_eq!(
            fill,
            Fill::Written {
                bytes: 66,
                consumed: 1
            }
        );
    }
}
