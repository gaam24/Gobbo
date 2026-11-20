//! Stage 0 logic, shared by the DLLs that run in the child (`lib.rs` in this folder, built for x64 and x86) and by the
//! launcher's tests. It uses `core` only: no allocation, no locks, no thread-local storage, no imports.
//!
//! A call of one of the four loader functions is redirected when its `OBJECT_ATTRIBUTES` names a file under the game
//! folder, the layer holds the same relative path as a file (not a folder), and the call only reads. The redirected
//! attributes point into the work area that the shim's frame holds, so they live as long as the call.
//!
//! Pointers are `usize` so one layout serves both architectures: `repr(C)` gives the 64-bit and the 32-bit layouts.

use core::mem::size_of;

/// The longest game or layer prefix in UTF-16 units, with its `\??\` prefix.
pub const PREFIX_UNITS: usize = 512;

/// The longest redirected path in UTF-16 units. A longer path passes through unchanged.
pub const WORK_UNITS: usize = 520;

/// The `\??\` prefix of an NT path, in UTF-16 units.
pub const NT_PREFIX: [u16; 4] = [b'\\' as u16, b'?' as u16, b'?' as u16, b'\\' as u16];

/// The `OBJ_CASE_INSENSITIVE` attribute.
const OBJ_CASE_INSENSITIVE: u32 = 0x40;

/// `FILE_ATTRIBUTE_DIRECTORY`: a folder is never redirected, so the game keeps seeing its own listing.
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;

/// The access bits that write, delete or change a file. A call with any of them passes through.
const WRITE_ACCESS: u32 = 0x0000_0002 // FILE_WRITE_DATA
    | 0x0000_0004 // FILE_APPEND_DATA
    | 0x0001_0000 // DELETE
    | 0x0004_0000 // WRITE_DAC
    | 0x0008_0000 // WRITE_OWNER
    | 0x1000_0000 // GENERIC_ALL
    | 0x2000_0000 // MAXIMUM_ALLOWED
    | 0x4000_0000; // GENERIC_WRITE

/// The `FILE_OPEN` disposition: open an existing file only. The other dispositions create or replace.
const FILE_OPEN: u32 = 1;

/// The access a query asks for: `FILE_READ_ATTRIBUTES`.
pub const QUERY_ACCESS: u32 = 0x80;

/// The kind of loader function a call comes from, as the shims name it in `Work::kind`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `NtOpenFile`: the attributes are the third argument, the access the second, and the call only opens.
    Open,
    /// `NtCreateFile`: as `Open`, plus the disposition, the eighth argument.
    Create,
    /// `NtQueryAttributesFile` and `NtQueryFullAttributesFile`: the attributes are the first argument, the call only reads.
    Query,
}

impl Kind {
    /// The code a shim writes into `Work::kind`.
    pub const fn code(self) -> u32 {
        match self {
            Self::Open => 0,
            Self::Create => 1,
            Self::Query => 2,
        }
    }

    fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Open),
            1 => Some(Self::Create),
            2 => Some(Self::Query),
            _ => None,
        }
    }
}

/// The configuration the launcher writes into the child before the loader runs. Its layout is shared with the DLLs.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Config {
    /// 1 when the redirect is on. The launcher turns it off before the hook takes over.
    pub enabled: u32,
    /// The length of `game`, in UTF-16 units, without the terminating zero.
    pub game_units: u32,
    /// The length of `layer`, in UTF-16 units, without the terminating zero.
    pub layer_units: u32,
    pub reserved: u32,
    /// The trampolines of the four stubs, in `STUBS` order of the launcher: open, create, query, query full.
    pub trampolines: [u64; 4],
    /// `\??\` and the game folder, for example `\??\C:\Games\Lethal Company`. No trailing separator.
    pub game: [u16; PREFIX_UNITS],
    /// `\??\` and the mod layer folder of the same program. No trailing separator.
    pub layer: [u16; PREFIX_UNITS],
    /// The shims of the four stubs, in the same order. A 32-bit stub jumps through its slot here.
    pub shims: [u64; 4],
}

impl Config {
    /// A configuration with the redirect off and no paths.
    pub const OFF: Self = Self {
        enabled: 0,
        game_units: 0,
        layer_units: 0,
        reserved: 0,
        trampolines: [0; 4],
        game: [0; PREFIX_UNITS],
        layer: [0; PREFIX_UNITS],
        shims: [0; 4],
    };
}

/// `OBJECT_ATTRIBUTES`, with the pointers as addresses in the process it is read in.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ObjectAttributes {
    pub length: u32,
    pub root: usize,
    pub name: usize,
    pub attributes: u32,
    pub security_descriptor: usize,
    pub security_quality_of_service: usize,
}

/// `UNICODE_STRING`, with the buffer as an address. The lengths are in bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnicodeString {
    pub length: u16,
    pub maximum_length: u16,
    pub buffer: usize,
}

/// The work area of one call, in the shim's frame: the call's parameters, the redirected attributes, the candidate path
/// and the output of the existence query.
#[repr(C)]
pub struct Work {
    /// `Kind::code` of the call.
    pub kind: u32,
    /// The access the caller asked for (the second argument of an open or a create; a query asks for attributes).
    pub access: u32,
    /// The disposition of a create (the eighth argument); an open opens an existing file.
    pub disposition: u32,
    pub reserved: u32,
    /// The attributes that a redirected call uses; a copy of the caller's, with the layer name.
    pub redirected: ObjectAttributes,
    /// The candidate path as a name, with its buffer in `units`.
    pub name: UnicodeString,
    /// The attributes of the existence query.
    pub probe: ObjectAttributes,
    /// `FILE_BASIC_INFORMATION` written by the existence query.
    pub information: [u64; 8],
    pub units: [u16; WORK_UNITS],
}

/// The loader's query that the worker calls to see whether the layer holds the file: its attributes and its output.
pub type QueryAttributes = unsafe extern "system" fn(usize, usize) -> i32;

/// Whether `text` starts with `prefix`, comparing ASCII letters without case.
pub fn starts_with_ascii_ignore_case(text: &[u16], prefix: &[u16]) -> bool {
    text.len() >= prefix.len()
        && text
            .iter()
            .zip(prefix)
            .all(|(&unit, &want)| fold(unit) == fold(want))
}

/// Upper-cases an ASCII letter; any other unit is compared as it is.
fn fold(unit: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&unit) {
        unit - 32
    } else {
        unit
    }
}

/// The relative part of a game path: what follows the game folder, which is empty or starts with a separator.
/// `None` when the path is not under the game folder.
pub fn relative_under<'a>(path: &'a [u16], folder: &[u16]) -> Option<&'a [u16]> {
    if !starts_with_ascii_ignore_case(path, folder) {
        return None;
    }

    let rest = &path[folder.len()..];

    match rest.first() {
        None => Some(rest),
        Some(&unit) if unit == b'\\' as u16 => Some(rest),
        _ => None,
    }
}

/// Writes `layer` + `rel` into `out` and returns the number of units. `None` when it does not fit.
pub fn candidate(layer: &[u16], rel: &[u16], out: &mut [u16]) -> Option<usize> {
    let total = layer.len().checked_add(rel.len())?;

    if total > out.len() {
        return None;
    }

    out[..layer.len()].copy_from_slice(layer);
    out[layer.len()..total].copy_from_slice(rel);
    Some(total)
}

/// Whether the call may be served from the layer: a read of an existing file, which is not a write or a create.
pub fn reads_only(kind: Kind, access: u32, disposition: u32) -> bool {
    match kind {
        Kind::Query => true,
        Kind::Open => access & WRITE_ACCESS == 0,
        Kind::Create => access & WRITE_ACCESS == 0 && disposition == FILE_OPEN,
    }
}

/// Decides one call. Returns the attributes to pass on: the redirected ones, which live in `work`, or `None` when the
/// call passes through unchanged.
///
/// `query` is the trampoline of the query stub, the loader's own code, so the existence check is not redirected itself.
///
/// # Safety
/// `attributes` must be the address of a valid `OBJECT_ATTRIBUTES` (or 0), and every address it holds must be valid.
pub unsafe fn redirect(
    config: &Config,
    attributes: usize,
    work: &mut Work,
    query: QueryAttributes,
) -> Option<usize> {
    let kind = Kind::from_code(work.kind)?;

    if config.enabled == 0 || attributes == 0 {
        return None;
    }
    // SAFETY: the caller guarantees the attributes are valid; they are read, not written.
    let original = unsafe { *(attributes as *const ObjectAttributes) };

    if original.root != 0 || original.name == 0 {
        return None;
    }

    if !reads_only(kind, work.access, work.disposition) {
        return None;
    }
    // SAFETY: `name` is a valid `UNICODE_STRING` of the caller, and its buffer holds `length` bytes.
    let name = unsafe { *(original.name as *const UnicodeString) };
    let units = usize::from(name.length) / 2;

    if name.buffer == 0 || units == 0 {
        return None;
    }
    // SAFETY: the buffer holds `units` UTF-16 units, as the length says.
    let path = unsafe { core::slice::from_raw_parts(name.buffer as *const u16, units) };
    let game = &config.game[..config.game_units as usize];
    let layer = &config.layer[..config.layer_units as usize];

    if !starts_with_ascii_ignore_case(path, &NT_PREFIX) {
        return None;
    }

    let under = &path[NT_PREFIX.len()..];
    let rel = relative_under(under, &game[NT_PREFIX.len()..])?;
    // The candidate: the layer's prefix (`\??\` and the layer folder) and the relative path.
    let length = candidate(layer, rel, &mut work.units)?;
    let name_address = &raw const work.name as usize;
    work.name = UnicodeString {
        length: (length * 2) as u16,
        maximum_length: (length * 2) as u16,
        buffer: work.units.as_mut_ptr() as usize,
    };
    work.probe = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root: 0,
        name: name_address,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: 0,
        security_quality_of_service: 0,
    };
    let probe_address = &raw const work.probe as usize;
    let information_address = work.information.as_mut_ptr() as usize;
    // SAFETY: the probe and the information buffer are valid for the call; `query` is the loader's own stub.
    let status = unsafe { query(probe_address, information_address) };

    if status != 0 {
        return None;
    }
    // A folder is not redirected: the game keeps its own listing.
    let file_attributes = (work.information[4] & 0xFFFF_FFFF) as u32;

    if file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return None;
    }

    work.redirected = ObjectAttributes {
        length: original.length,
        root: 0,
        name: name_address,
        attributes: original.attributes,
        security_descriptor: original.security_descriptor,
        security_quality_of_service: original.security_quality_of_service,
    };
    Some(&raw const work.redirected as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn prefixes_compare_ascii_letters_without_case() {
        assert!(starts_with_ascii_ignore_case(
            &wide(r"\??\C:\Games\Lethal"),
            &wide(r"\??\c:\games")
        ));
        assert!(!starts_with_ascii_ignore_case(
            &wide(r"\??\C:\Games"),
            &wide(r"\??\C:\Games\Lethal")
        ));
    }

    #[test]
    fn a_path_is_under_a_folder_only_at_a_separator() {
        let folder = wide(r"C:\Games\Lethal");
        assert_eq!(
            relative_under(&wide(r"C:\Games\Lethal\a.dll"), &folder),
            Some(&wide(r"\a.dll")[..])
        );
        assert_eq!(
            relative_under(&wide(r"C:\Games\Lethal"), &folder),
            Some(&[][..])
        );
        assert_eq!(
            relative_under(&wide(r"C:\Games\Lethal2\a.dll"), &folder),
            None
        );
    }

    #[test]
    fn the_candidate_is_the_layer_and_the_relative_path() {
        let mut out = [0u16; 32];
        let length = candidate(&wide(r"\??\D:\mods"), &wide(r"\bin\a.dll"), &mut out).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&out[..length]),
            r"\??\D:\mods\bin\a.dll"
        );
        let mut small = [0u16; 4];
        assert_eq!(
            candidate(&wide(r"\??\D:\mods"), &wide(r"\a"), &mut small),
            None
        );
    }

    #[test]
    fn only_reads_of_existing_files_are_redirected() {
        assert!(reads_only(Kind::Open, 0x8000_0000, 0));
        assert!(!reads_only(Kind::Open, 0x4000_0000, 0));
        assert!(reads_only(Kind::Create, 0x8000_0000, FILE_OPEN));
        assert!(!reads_only(Kind::Create, 0x8000_0000, 2));
        assert!(reads_only(Kind::Query, 0, 0));
    }

    #[test]
    fn the_kind_codes_round_trip() {
        for kind in [Kind::Open, Kind::Create, Kind::Query] {
            assert_eq!(Kind::from_code(kind.code()), Some(kind));
        }

        assert_eq!(Kind::from_code(9), None);
    }

    #[test]
    fn the_config_offsets_are_the_ones_the_launcher_writes() {
        assert_eq!(core::mem::offset_of!(Config, trampolines), 16);
        assert_eq!(core::mem::offset_of!(Config, game), 48);
        assert_eq!(core::mem::offset_of!(Config, shims), 48 + 4 * PREFIX_UNITS);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn the_64_bit_structures_have_the_64_bit_sizes() {
        assert_eq!(size_of::<ObjectAttributes>(), 48);
        assert_eq!(size_of::<UnicodeString>(), 16);
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn the_32_bit_structures_have_the_32_bit_sizes() {
        assert_eq!(size_of::<ObjectAttributes>(), 24);
        assert_eq!(size_of::<UnicodeString>(), 8);
    }
}
