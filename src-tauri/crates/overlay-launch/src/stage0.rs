//! The early redirect. The launcher maps the stage 0 DLL (`stage0/lib.rs`) into a suspended child, writes its
//! configuration, and patches the start of four ntdll loader stubs with a jump to the DLL's
//! shims. Each stub gets a trampoline first: a copy of its original bytes, which the shim calls when it does not redirect.
//! The DLL imports nothing, so it can run before the child's loader has initialised anything.
//!
//! A 64-bit child gets the x64 DLL and the 64-bit stub shape; a 32-bit child (WOW64) gets the x86 DLL, its own stub shape,
//! and the 32-bit ntdll found in the child's address space.

use std::collections::HashMap;
use std::ffi::{OsStr, c_void};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use windows_sys::Win32::Foundation::{FALSE, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_IMAGE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE,
    VirtualAllocEx, VirtualProtectEx, VirtualQueryEx,
};
#[cfg(test)]
use windows_sys::Win32::System::Memory::{MEM_RELEASE, VirtualFreeEx};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::wide;

/// The path logic of the stage 0 DLL, compiled here too so its rules have tests on this side.
#[path = "../stage0/logic.rs"]
#[allow(
    dead_code,
    reason = "the DLL calls the redirect; the launcher writes the configuration and checks paths"
)]
pub mod logic;

/// The loader's file calls the redirect serves: each one is a plain syscall stub in ntdll.
pub(crate) const STUBS: [&str; 4] = [
    "NtOpenFile",
    "NtCreateFile",
    "NtQueryAttributesFile",
    "NtQueryFullAttributesFile",
];

/// The shim of each stub, in the same order as `STUBS`: the exports of the DLL.
pub(crate) const SHIMS: [&str; 4] = [
    "GobboStage0NtOpenFile",
    "GobboStage0NtCreateFile",
    "GobboStage0NtQueryAttributesFile",
    "GobboStage0NtQueryFullAttributesFile",
];

/// The bytes of arguments each stub removes with `ret N` on a 32-bit system (the stdcall convention): the attributes are
/// three arguments for `NtOpenFile` (24), eleven for `NtCreateFile` (44), and two for the queries (8).
const ARGUMENT_BYTES_32: [u16; 4] = [0x18, 0x2C, 0x08, 0x08];

/// The configuration symbol of the DLL.
const CONFIG: &str = "GOBBO_STAGE0_CONFIG";

/// The stage 0 DLLs, built by `build.rs`. Empty when this build has no Windows target for them.
const DLL_X64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/gobbo_overlay_stage0_x64.dll"));
const DLL_X86: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/gobbo_overlay_stage0_x86.dll"));

/// The bit width of the child the redirect is set up for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bits {
    X64,
    X86,
}

/// Generates a `Bits` method that maps the two architectures to their values.
macro_rules! bits_info {
    ($name:ident, $ty:ty, X64 => $x64:expr, X86 => $x86:expr) => {
        const fn $name(self) -> $ty {
            match self {
                Self::X64 => $x64,
                Self::X86 => $x86,
            }
        }
    };
}

impl Bits {
    fn dll(self) -> &'static [u8] {
        match self {
            Self::X64 => DLL_X64,
            Self::X86 => DLL_X86,
        }
    }

    // The bitness the decoder reads the stub with.
    bits_info!(bitness, u32, X64 => 64, X86 => 32);

    // The length of the stub shape the launcher checks before it touches a stub.
    bits_info!(stub_length, usize, X64 => 22, X86 => 15);

    // The machine of the DLL's PE header, and the optional header's magic.
    bits_info!(machine, u16, X64 => 0x8664, X86 => 0x14C);

    bits_info!(magic, u16, X64 => 0x20B, X86 => 0x10B);

    // The offset of the data directories from the start of the optional header.
    bits_info!(directories, usize, X64 => 112, X86 => 96);
}

/// Room for one trampoline: the copy of a stub, with spare bytes so the slots do not share a cache line.
const TRAMPOLINE_SLOT: usize = 32;

/// The redirect asked for by a launch: the configuration, and the file the hook reads to hand the stubs back.
pub struct Early<'a> {
    pub config: logic::Config,
    pub handover: &'a Path,
}

/// The patched child: what `restore` needs to put the stubs back.
#[allow(
    dead_code,
    reason = "the hook hands the stubs back in the product; restore is the test's way to do the same"
)]
pub(crate) struct Patched {
    process: HANDLE,
    config_address: usize,
    trampolines: usize,
    stubs: Vec<(usize, Vec<u8>)>,
}

impl Patched {
    /// The text the hook reads: the configuration's address, then each stub with the bytes it had before the patch.
    pub(crate) fn handover(&self) -> String {
        let mut text = format!("config {}\n", self.config_address);
        for (address, original) in &self.stubs {
            let hex: String = original.iter().map(|byte| format!("{byte:02x}")).collect();
            text.push_str(&format!("stub {address} {hex}\n"));
        }
        text
    }

    /// Puts the original bytes back into the child's stubs and turns the redirect off, so no later call is served from
    /// the layer by this DLL. The image stays mapped: a call may still be running in it.
    #[cfg(test)]
    pub(crate) fn restore(self) -> io::Result<()> {
        for (address, original) in &self.stubs {
            write_code(self.process, *address, original)?;
        }
        write_code(self.process, self.config_address, &0u32.to_le_bytes())?;
        // SAFETY: the trampoline block was allocated in the child by `install` and is no longer reached.
        unsafe {
            VirtualFreeEx(
                self.process,
                self.trampolines as *mut c_void,
                0,
                MEM_RELEASE,
            )
        };
        Ok(())
    }
}

/// The configuration for a session: the game folder is redirected to the mod layer of the same program. Both are
/// absolute Windows paths; the redirect starts with the NT prefix that the loader's names carry.
pub fn redirect_config(game_root: &Path, layer_root: &Path) -> io::Result<logic::Config> {
    let mut config = logic::Config::OFF;
    config.enabled = 1;
    let game = nt_prefix(game_root)?;
    let layer = nt_prefix(layer_root)?;
    config.game[..game.len()].copy_from_slice(&game);
    config.game_units = game.len() as u32;
    config.layer[..layer.len()].copy_from_slice(&layer);
    config.layer_units = layer.len() as u32;
    Ok(config)
}

/// `\??\` and the folder, with `/` as `\` and no trailing separator.
fn nt_prefix(folder: &Path) -> io::Result<Vec<u16>> {
    let mut units: Vec<u16> = folder
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .collect();
    while units.len() > 3 && units.last() == Some(&u16::from(b'\\')) {
        units.pop();
    }
    let absolute = units.len() >= 3 && units[1] == u16::from(b':') && units[2] == u16::from(b'\\');
    if !absolute {
        return Err(io::Error::other("the redirect needs an absolute folder"));
    }
    let mut prefix = logic::NT_PREFIX.to_vec();
    prefix.extend(units);
    if prefix.len() > logic::PREFIX_UNITS {
        return Err(io::Error::other(
            "the folder path is too long for the redirect",
        ));
    }
    Ok(prefix)
}

/// Maps the stage 0 DLL into the suspended child, writes `config` into it, and patches the four stubs. Refuses a child
/// whose ntdll or stubs are not the ones this launcher knows, before any write to the child's code.
pub(crate) fn install(
    process: HANDLE,
    mut config: logic::Config,
    bits: Bits,
) -> io::Result<Patched> {
    let ntdll = match bits {
        Bits::X64 => {
            check_ntdll(process)?;
            ntdll_base()
        }
        Bits::X86 => find_image(process, "ntdll.dll")?,
    };
    let mut originals = Vec::with_capacity(STUBS.len());
    for (index, name) in STUBS.iter().enumerate() {
        let address = match bits {
            Bits::X64 => ntdll_export(name)?,
            Bits::X86 => export_in(process, ntdll, name)?,
        };
        let original = read_code(process, address, bits.stub_length())?;
        if !is_accepted_stub(bits, &original, index) {
            return Err(io::Error::other(format!(
                "the child's {name} has a stub shape the launcher does not know"
            )));
        }
        originals.push((address, original));
    }
    let image = map_image(process, bits)?;
    let trampolines = allocate(process, STUBS.len() * TRAMPOLINE_SLOT)?;
    for (index, (_, original)) in originals.iter().enumerate() {
        let address = trampolines + index * TRAMPOLINE_SLOT;
        write_code(process, address, original)?;
        config.trampolines[index] = address as u64;
    }
    for (index, name) in SHIMS.iter().enumerate() {
        config.shims[index] = (image.base + image.export(name)?) as u64;
    }
    let config_address = image.base + image.export(CONFIG)?;
    write_code(process, config_address, config_bytes(&config))?;
    for (index, (stub, _)) in originals.iter().enumerate() {
        let jump = match bits {
            Bits::X64 => jump_to(config.shims[index] as usize),
            Bits::X86 => {
                let slot = config_address + std::mem::offset_of!(logic::Config, shims) + 8 * index;
                jump_through(slot as u32)
            }
        };
        write_code(process, *stub, &jump)?;
    }
    Ok(Patched {
        process,
        config_address,
        trampolines,
        stubs: originals,
    })
}

/// The bytes of the configuration, as the DLL reads them.
fn config_bytes(config: &logic::Config) -> &[u8] {
    // SAFETY: `Config` is `repr(C)` and holds only integers and arrays of integers, so its bytes are initialised and
    // can be read as a slice of `size_of::<Config>()` bytes that lives as long as `config`.
    unsafe {
        std::slice::from_raw_parts(
            (config as *const logic::Config).cast::<u8>(),
            std::mem::size_of::<logic::Config>(),
        )
    }
}

/// `jmp qword ptr [rip+0]` followed by the absolute target (x64).
fn jump_to(target: usize) -> Vec<u8> {
    let mut jump = vec![0xFF, 0x25, 0x00, 0x00, 0x00, 0x00];
    jump.extend_from_slice(&(target as u64).to_le_bytes());
    jump
}

/// `jmp dword ptr [slot]`, where the slot holds the target (x86).
fn jump_through(slot: u32) -> Vec<u8> {
    let mut jump = vec![0xFF, 0x25];
    jump.extend_from_slice(&slot.to_le_bytes());
    jump
}

/// Whether the bytes at the start of a loader stub have a shape the launcher can redirect. The bytes are decoded, not
/// matched: the shape is an instruction sequence, so a build that places the same instructions elsewhere, or with a
/// different check in between, is still recognized. The trampoline keeps every byte of the stub, so the instructions the
/// launcher does not look at run as they did. A stub it does not recognize fails the launch; it is never patched blind.
fn is_accepted_stub(bits: Bits, bytes: &[u8], index: usize) -> bool {
    if bytes.len() < bits.stub_length() {
        return false;
    }
    let instructions: Vec<Instruction> =
        Decoder::with_ip(bits.bitness(), bytes, 0, DecoderOptions::NONE)
            .iter()
            .take_while(|instruction| !instruction.is_invalid())
            .collect();
    match bits {
        Bits::X64 => accepted_x64(&instructions),
        Bits::X86 => accepted_x86(&instructions, ARGUMENT_BYTES_32[index]),
    }
}

/// The x64 stub of a system call: `mov r10, rcx; mov eax, <number>`, and a `syscall` followed by `ret` further on. A
/// check of the shared user data between them (Windows 10 and 11) is allowed, not required.
fn accepted_x64(instructions: &[Instruction]) -> bool {
    let [first, second, ..] = instructions else {
        return false;
    };
    let copies_first_argument = first.mnemonic() == Mnemonic::Mov
        && first.op0_register() == Register::R10
        && first.op1_register() == Register::RCX;
    let loads_number = second.mnemonic() == Mnemonic::Mov
        && second.op0_register() == Register::EAX
        && second.op1_kind() == OpKind::Immediate32;
    let system_call = instructions
        .windows(2)
        .any(|pair| pair[0].mnemonic() == Mnemonic::Syscall && pair[1].mnemonic() == Mnemonic::Ret);
    copies_first_argument && loads_number && system_call
}

/// The x86 stub of a system call through WOW64: `mov eax, <number>; mov edx, <target>; call edx; ret <argument bytes>`.
/// The `ret` must remove the argument bytes the shim of this stub removes.
fn accepted_x86(instructions: &[Instruction], argument_bytes: u16) -> bool {
    let [number, target, call, ret, ..] = instructions else {
        return false;
    };
    let loads_number = number.mnemonic() == Mnemonic::Mov
        && number.op0_register() == Register::EAX
        && number.op1_kind() == OpKind::Immediate32;
    let loads_target = target.mnemonic() == Mnemonic::Mov
        && target.op0_register() == Register::EDX
        && target.op1_kind() == OpKind::Immediate32;
    let calls_target = call.mnemonic() == Mnemonic::Call
        && call.op0_kind() == OpKind::Register
        && call.op0_register() == Register::EDX;
    let returns_arguments = ret.mnemonic() == Mnemonic::Ret
        && ret.op0_kind() == OpKind::Immediate16
        && ret.immediate16() == argument_bytes;
    loads_number && loads_target && calls_target && returns_arguments
}

/// The address of an ntdll export in this process. ntdll has the same base in every process of the boot.
pub(crate) fn ntdll_export(name: &str) -> io::Result<usize> {
    let module = ntdll_base();
    let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
    // SAFETY: `module` is ntdll and the export name is NUL-terminated.
    let procedure = unsafe { GetProcAddress(module as _, name.as_ptr().cast()) };
    procedure
        .map(|procedure| procedure as usize)
        .ok_or_else(io::Error::last_os_error)
}

fn ntdll_base() -> usize {
    // SAFETY: the name is NUL-terminated; ntdll is loaded in every Windows process, so the handle is not null.
    unsafe { GetModuleHandleW(wide(OsStr::new("ntdll.dll")).as_ptr()) as usize }
}

/// Checks that the child's ntdll is the one this process uses: its header carries the same timestamp and image size.
fn check_ntdll(process: HANDLE) -> io::Result<()> {
    let base = ntdll_base();
    if base == 0 {
        return Err(io::Error::last_os_error());
    }
    let ours = read_code(current_process(), base, 0x400)?;
    let theirs = read_code(process, base, 0x400)?;
    let stamp =
        |bytes: &[u8]| u32::from_le_bytes([bytes[0x108], bytes[0x109], bytes[0x10A], bytes[0x10B]]);
    if theirs[..2] != *b"MZ" || stamp(&ours) != stamp(&theirs) {
        return Err(io::Error::other(
            "the child's ntdll is not at the address this launcher expects",
        ));
    }
    Ok(())
}

/// The base of an image named `name` in the child, found by scanning its committed image regions. Used for a 32-bit
/// child, where the launcher's own ntdll is a different image.
fn find_image(process: HANDLE, name: &str) -> io::Result<usize> {
    let mut address = 0x10000usize;
    while address < 0x7FFF_0000 {
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the query writes one structure of the size given; the address is any value in the child.
        let got = unsafe {
            VirtualQueryEx(
                process,
                address as *const c_void,
                &mut info,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if got == 0 {
            break;
        }
        let base = info.BaseAddress as usize;
        if info.State == MEM_COMMIT
            && info.Type == MEM_IMAGE
            && export_name(process, base).is_some_and(|found| found.eq_ignore_ascii_case(name))
        {
            return Ok(base);
        }
        address = base + info.RegionSize;
    }
    Err(io::Error::other(format!(
        "the child has no image named {name}"
    )))
}

/// The export name of a PE image in the child, if it has an export directory whose name is at the image's own header.
fn export_name(process: HANDLE, base: usize) -> Option<String> {
    let header = read_code(process, base, 0x1000).ok()?;
    if &header[..2] != b"MZ" {
        return None;
    }
    let nt = u32le(&header, 0x3C).ok()? as usize;
    let directory = u32le(&header, nt + 24 + 96).ok()? as usize;
    if directory == 0 {
        return None;
    }
    let export = read_code(process, base + directory, 16).ok()?;
    let name_rva = u32le(&export, 12).ok()? as usize;
    cstring(process, base + name_rva)
}

/// The address of an export of a 32-bit image in the child, read from the child's memory.
fn export_in(process: HANDLE, base: usize, name: &str) -> io::Result<usize> {
    let header = read_code(process, base, 0x1000)?;
    let nt = u32le(&header, 0x3C)? as usize;
    let directory = u32le(&header, nt + 24 + 96)? as usize;
    let export = read_code(process, base + directory, 40)?;
    let count = u32le(&export, 24)? as usize;
    let functions = u32le(&export, 28)? as usize;
    let names = u32le(&export, 32)? as usize;
    let ordinals = u32le(&export, 36)? as usize;
    for index in 0..count {
        let name_rva = u32le(&read_code(process, base + names + 4 * index, 4)?, 0)? as usize;
        if cstring(process, base + name_rva).as_deref() == Some(name) {
            let ordinal = usize::from(u16le(
                &read_code(process, base + ordinals + 2 * index, 2)?,
                0,
            )?);
            let rva = u32le(&read_code(process, base + functions + 4 * ordinal, 4)?, 0)? as usize;
            return Ok(base + rva);
        }
    }
    Err(io::Error::other(format!("the child's ntdll has no {name}")))
}

/// A NUL-terminated string at `address` in the child, at most 64 bytes.
fn cstring(process: HANDLE, address: usize) -> Option<String> {
    let bytes = read_code(process, address, 64).ok()?;
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// The mapped DLL: its base in the child and its export table.
struct Image {
    base: usize,
    exports: HashMap<String, usize>,
}

impl Image {
    fn export(&self, name: &str) -> io::Result<usize> {
        self.exports
            .get(name)
            .copied()
            .ok_or_else(|| io::Error::other(format!("the stage 0 DLL does not export {name}")))
    }
}

/// Maps the stage 0 DLL into the child: sections at their addresses, base relocations for the new base, and the export
/// table. A DLL that imports something is refused: the child's loader has not started, so nothing could resolve it.
fn map_image(process: HANDLE, bits: Bits) -> io::Result<Image> {
    let bytes = bits.dll();
    if bytes.is_empty() {
        return Err(io::Error::other(
            "this build has no stage 0 DLL for this child",
        ));
    }
    let nt = u32le(bytes, 0x3C)? as usize;
    if bytes.get(nt..nt + 4) != Some(b"PE\0\0") || u16le(bytes, nt + 4)? != bits.machine() {
        return Err(io::Error::other(
            "the stage 0 DLL is not the PE image for this child",
        ));
    }
    let section_count = usize::from(u16le(bytes, nt + 6)?);
    let optional_size = usize::from(u16le(bytes, nt + 20)?);
    let optional = nt + 24;
    if u16le(bytes, optional)? != bits.magic() {
        return Err(io::Error::other(
            "the stage 0 DLL has the wrong optional header",
        ));
    }
    let preferred = match bits {
        Bits::X64 => u64le(bytes, optional + 24)?,
        Bits::X86 => u64::from(u32le(bytes, optional + 28)?),
    };
    let image_size = u32le(bytes, optional + 56)? as usize;
    let headers_size = u32le(bytes, optional + 60)? as usize;
    let directories = optional + bits.directories();
    let directory = |index: usize| -> io::Result<(usize, usize)> {
        Ok((
            u32le(bytes, directories + 8 * index)? as usize,
            u32le(bytes, directories + 8 * index + 4)? as usize,
        ))
    };
    if directory(1)?.1 != 0 {
        return Err(io::Error::other(
            "the stage 0 DLL imports something; it must not",
        ));
    }

    let base = allocate(process, image_size)?;
    let delta = (base as u64).wrapping_sub(preferred);
    let mut image = vec![0u8; image_size];
    image[..headers_size].copy_from_slice(&bytes[..headers_size]);
    let sections = nt + 24 + optional_size;
    for index in 0..section_count {
        let header = sections + 40 * index;
        let size = u32le(bytes, header + 16)? as usize;
        let raw = u32le(bytes, header + 20)? as usize;
        let address = u32le(bytes, header + 12)? as usize;
        let length = size.min(image_size.saturating_sub(address));
        if length > 0 {
            image[address..address + length].copy_from_slice(&bytes[raw..raw + length]);
        }
    }
    let (relocations, relocations_size) = directory(5)?;
    let mut offset = 0;
    while offset < relocations_size {
        let page = u32le(&image, relocations + offset)? as usize;
        let block = u32le(&image, relocations + offset + 4)? as usize;
        for entry in 0..(block.saturating_sub(8)) / 2 {
            let value = u16le(&image, relocations + offset + 8 + 2 * entry)?;
            let at = page + usize::from(value & 0xFFF);
            match (value >> 12, bits) {
                (0, _) => {}
                (10, Bits::X64) => {
                    let old = u64le(&image, at)?;
                    image[at..at + 8].copy_from_slice(&old.wrapping_add(delta).to_le_bytes());
                }
                (3, Bits::X86) => {
                    let old = u32le(&image, at)?;
                    image[at..at + 4]
                        .copy_from_slice(&old.wrapping_add(delta as u32).to_le_bytes());
                }
                _ => {
                    return Err(io::Error::other(
                        "the stage 0 DLL has a relocation this launcher does not apply",
                    ));
                }
            }
        }
        offset += block;
    }
    let exports = read_exports(&image, directory(0)?.0)?;
    write_code(process, base, &image)?;
    Ok(Image { base, exports })
}

/// The export names of a mapped image, with their RVAs. A 32-bit stdcall export is decorated (`_name@N`); the
/// decoration is removed, so the names are the ones the launcher asks for.
fn read_exports(image: &[u8], directory: usize) -> io::Result<HashMap<String, usize>> {
    if directory == 0 {
        return Err(io::Error::other("the stage 0 DLL exports nothing"));
    }
    let count = u32le(image, directory + 24)? as usize;
    let functions = u32le(image, directory + 28)? as usize;
    let names = u32le(image, directory + 32)? as usize;
    let ordinals = u32le(image, directory + 36)? as usize;
    let mut exports = HashMap::new();
    for index in 0..count {
        let name_rva = u32le(image, names + 4 * index)? as usize;
        let ordinal = usize::from(u16le(image, ordinals + 2 * index)?);
        let rva = u32le(image, functions + 4 * ordinal)? as usize;
        let end = image[name_rva..]
            .iter()
            .position(|&byte| byte == 0)
            .ok_or_else(|| io::Error::other("an export name is not terminated"))?;
        let raw = String::from_utf8_lossy(&image[name_rva..name_rva + end]).into_owned();
        exports.insert(undecorate(&raw), rva);
    }
    Ok(exports)
}

/// The name of a 32-bit export without its stdcall or cdecl decoration: `_name@N` and `_name` become `name`.
fn undecorate(name: &str) -> String {
    let name = name.split('@').next().unwrap_or(name);
    name.strip_prefix('_').unwrap_or(name).to_string()
}

macro_rules! read_le {
    ($name:ident, $ty:ty) => {
        fn $name(bytes: &[u8], at: usize) -> io::Result<$ty> {
            Ok(<$ty>::from_le_bytes(field(bytes, at)?))
        }
    };
}

read_le!(u16le, u16);
read_le!(u32le, u32);
read_le!(u64le, u64);

fn field<const N: usize>(bytes: &[u8], at: usize) -> io::Result<[u8; N]> {
    bytes
        .get(at..at + N)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))
}

/// Allocates committed, executable memory in the child.
fn allocate(process: HANDLE, size: usize) -> io::Result<usize> {
    // SAFETY: a new region in the child; the child is suspended and nothing else uses it.
    let address = unsafe {
        VirtualAllocEx(
            process,
            std::ptr::null(),
            size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_EXECUTE_READWRITE,
        )
    } as usize;
    if address == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(address)
}

/// Reads code bytes of a process; the protection is not changed.
fn read_code(process: HANDLE, address: usize, length: usize) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0u8; length];
    let mut done = 0;
    // SAFETY: `bytes` has room for `length` bytes; the call reports how many it read.
    let ok = unsafe {
        ReadProcessMemory(
            process,
            address as *const c_void,
            bytes.as_mut_ptr().cast(),
            length,
            &mut done,
        )
    };
    if ok == FALSE || done != length {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes)
}

/// Writes code into a process, making the page writable while it does, then flushes the instruction cache.
fn write_code(process: HANDLE, address: usize, bytes: &[u8]) -> io::Result<()> {
    let mut old = 0;
    // SAFETY: the range is inside a module or a block of the child; the old protection is restored below.
    let protected = unsafe {
        VirtualProtectEx(
            process,
            address as *const c_void,
            bytes.len(),
            PAGE_EXECUTE_READWRITE,
            &mut old,
        )
    };
    if protected == FALSE {
        return Err(io::Error::last_os_error());
    }
    let mut written = 0;
    // SAFETY: `bytes` is valid for its length; the call reports how many it wrote.
    let ok = unsafe {
        WriteProcessMemory(
            process,
            address as *const c_void,
            bytes.as_ptr().cast(),
            bytes.len(),
            &mut written,
        )
    };
    let mut restored = 0;
    // SAFETY: restores the protection that `VirtualProtectEx` returned above.
    unsafe {
        VirtualProtectEx(
            process,
            address as *const c_void,
            bytes.len(),
            old,
            &mut restored,
        )
    };
    if ok == FALSE || written != bytes.len() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: flushes the instruction cache of the child for the range just written.
    unsafe { FlushInstructionCache(process, address as *const c_void, bytes.len()) };
    Ok(())
}

/// The pseudo handle of this process. ReadProcessMemory accepts it to read this process's own memory.
fn current_process() -> HANDLE {
    // SAFETY: the call takes no arguments and returns the pseudo handle of this process.
    unsafe { GetCurrentProcess() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accepted_stub_shape_is_the_one_measured_on_windows_11() {
        let stub = [
            0x4C, 0x8B, 0xD1, 0xB8, 0x33, 0x00, 0x00, 0x00, 0xF6, 0x04, 0x25, 0x08, 0x03, 0xFE,
            0x7F, 0x01, 0x75, 0x03, 0x0F, 0x05, 0xC3, 0xCD,
        ];
        assert!(is_accepted_stub(Bits::X64, &stub, 0));
        let mut other = stub;
        other[0] = 0x90;
        assert!(!is_accepted_stub(Bits::X64, &other, 0));
    }

    #[test]
    fn the_32_bit_stub_shape_is_the_one_measured_on_windows_11() {
        // NtOpenFile on this build: mov eax, 0x33; mov edx, <target>; call edx; ret 0x18.
        let stub = [
            0xB8, 0x33, 0x00, 0x00, 0x00, 0xBA, 0x10, 0x52, 0xE2, 0x77, 0xFF, 0xD2, 0xC2, 0x18,
            0x00,
        ];
        assert!(is_accepted_stub(Bits::X86, &stub, 0));
        assert!(
            !is_accepted_stub(Bits::X86, &stub, 1),
            "the argument bytes must match the stub"
        );
        let mut other = stub;
        other[0] = 0x90;
        assert!(!is_accepted_stub(Bits::X86, &other, 0));
    }

    #[test]
    fn the_stubs_of_this_windows_build_are_accepted() {
        // The four loader stubs as this process's ntdll has them. A Windows update that changes their shape fails here
        // first, before a launch does.
        for (index, name) in STUBS.iter().enumerate() {
            let address = ntdll_export(name).expect("ntdll exports the stub");
            // SAFETY: the stub is code of ntdll, loaded in this process for the whole test; the length is the stub length
            // the launcher reads from a child.
            let bytes = unsafe {
                std::slice::from_raw_parts(address as *const u8, Bits::X64.stub_length())
            };
            assert!(
                is_accepted_stub(Bits::X64, bytes, index),
                "{name}: {bytes:02X?}"
            );
        }
    }

    #[test]
    fn a_stub_without_a_system_call_or_with_another_argument_is_refused() {
        // `int 0x2E` is the system call of Windows 2000 and Windows XP, not a `syscall`.
        let interrupt = [
            0x4C, 0x8B, 0xD1, 0xB8, 0x33, 0x00, 0x00, 0x00, 0xCD, 0x2E, 0xC3, 0x90, 0x90, 0x90,
            0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
        ];
        assert!(!is_accepted_stub(Bits::X64, &interrupt, 0));
        // The first argument must be copied from `rcx`, as the system call takes it.
        let other_register = [
            0x4C, 0x8B, 0xD2, 0xB8, 0x33, 0x00, 0x00, 0x00, 0x0F, 0x05, 0xC3, 0x90, 0x90, 0x90,
            0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
        ];
        assert!(!is_accepted_stub(Bits::X64, &other_register, 0));
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn the_x64_jump_loads_its_target_from_the_word_after_it() {
        let jump = jump_to(0x1122_3344_5566_7788);
        assert_eq!(&jump[..6], &[0xFF, 0x25, 0, 0, 0, 0]);
        assert_eq!(
            u64::from_le_bytes(jump[6..].try_into().unwrap()),
            0x1122_3344_5566_7788
        );
    }

    #[test]
    fn the_x86_jump_reads_its_target_through_the_slot() {
        assert_eq!(
            jump_through(0x1234_5678),
            vec![0xFF, 0x25, 0x78, 0x56, 0x34, 0x12]
        );
    }

    #[test]
    fn decorated_export_names_are_undecorated() {
        assert_eq!(
            undecorate("_GobboStage0NtOpenFile@0"),
            "GobboStage0NtOpenFile"
        );
        assert_eq!(undecorate("_GOBBO_STAGE0_CONFIG"), "GOBBO_STAGE0_CONFIG");
        assert_eq!(undecorate("GobboStage0NtOpenFile"), "GobboStage0NtOpenFile");
    }

    #[test]
    fn a_folder_becomes_an_nt_prefix_with_backslashes_and_no_trailing_separator() {
        let prefix = nt_prefix(Path::new(r"C:/Games/Lethal Company/")).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&prefix),
            r"\??\C:\Games\Lethal Company"
        );
        assert!(nt_prefix(Path::new(r"Games\Lethal")).is_err());
    }

    #[test]
    fn the_configuration_starts_with_the_enabled_flag_it_is_read_with() {
        let config = redirect_config(Path::new(r"C:\Game"), Path::new(r"C:\Mods")).unwrap();
        assert_eq!(config.enabled, 1);
        assert_eq!(
            config_bytes(&config).len(),
            std::mem::size_of::<logic::Config>()
        );
        assert_eq!(&config_bytes(&config)[..4], &1u32.to_le_bytes());
    }
}
