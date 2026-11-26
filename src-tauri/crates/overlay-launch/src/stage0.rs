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
    MEM_COMMIT, MEM_IMAGE, MEM_RELEASE, MEM_RESERVE, MEMORY_BASIC_INFORMATION,
    PAGE_EXECUTE_READWRITE, VirtualAllocEx, VirtualFreeEx, VirtualProtectEx, VirtualQueryEx,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use gobbo_overlay_core::early::Line;

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

/// The patched child: the bytes the hook reads to put the stubs back. The test-only fields let a test restore them
/// through the child instead of through the handover file.
pub(crate) struct Patched {
    config_address: usize,
    stubs: Vec<(usize, Vec<u8>)>,
    #[cfg(all(test, target_arch = "x86_64"))]
    process: HANDLE,
    #[cfg(all(test, target_arch = "x86_64"))]
    trampolines: usize,
}

impl Patched {
    /// The text the hook reads: the configuration's address, then each stub with the bytes it had before the patch.
    pub(crate) fn handover(&self) -> String {
        let mut text = Line::Config(self.config_address).to_text();

        for (address, original) in &self.stubs {
            text.push_str(
                &Line::Stub {
                    address: *address,
                    original: original.clone(),
                }
                .to_text(),
            );
        }

        text
    }

    /// Puts the original bytes back into the child's stubs and turns the redirect off, so no later call is served from
    /// the layer by this DLL. The image stays mapped: a call may still be running in it.
    #[cfg(all(test, target_arch = "x86_64"))]
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

/// The configuration for a session: the game folder reads from the mod layer and writes under the layer go to `upper`.
/// All three are absolute Windows paths; the redirect starts with the NT prefix that the loader's names carry.
pub fn redirect_config(
    game_root: &Path,
    layer_root: &Path,
    upper_root: &Path,
) -> io::Result<logic::Config> {
    let mut config = logic::Config::OFF;
    config.enabled = 1;
    let game = nt_prefix(game_root)?;
    let layer = nt_prefix(layer_root)?;
    let upper = nt_prefix(upper_root)?;
    config.game[..game.len()].copy_from_slice(&game);
    config.game_units = game.len() as u32;
    config.layer[..layer.len()].copy_from_slice(&layer);
    config.layer_units = layer.len() as u32;
    config.upper[..upper.len()].copy_from_slice(&upper);
    config.upper_units = upper.len() as u32;
    Ok(config)
}

/// The two leading separators of a Win32 UNC path.
const WIN32_UNC: [u16; 2] = [b'\\' as u16; 2];
/// The NT spelling of a UNC root, before the server and share.
const NT_UNC: [u16; 8] = [
    b'\\' as u16,
    b'?' as u16,
    b'?' as u16,
    b'\\' as u16,
    b'U' as u16,
    b'N' as u16,
    b'C' as u16,
    b'\\' as u16,
];

/// Whether `tail` names both a server and a share. Without both, the path is not a UNC folder the redirect can use.
fn has_unc_server_share(tail: &[u16]) -> bool {
    let Some(separator) = tail.iter().position(|&unit| unit == u16::from(b'\\')) else {
        return false;
    };
    separator > 0 && tail.len() > separator + 1
}

/// `\??\` and the folder, with `/` as `\` and no trailing separator. A drive path keeps its DOS spelling; a Win32 UNC
/// root becomes the `\??\UNC\server\share` form the loader uses, and an NT UNC root is kept as it is.
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

    let drive = units.len() >= 3 && units[1] == u16::from(b':') && units[2] == u16::from(b'\\');
    let win32_unc = units.len() >= WIN32_UNC.len() && units[..WIN32_UNC.len()] == WIN32_UNC[..];
    let nt_unc =
        units.len() >= NT_UNC.len() && logic::starts_with_ascii_ignore_case(&units, &NT_UNC);

    let prefix = if drive {
        let mut prefix = logic::NT_PREFIX.to_vec();
        prefix.extend(units);
        prefix
    } else if win32_unc {
        let tail = &units[WIN32_UNC.len()..];
        if !has_unc_server_share(tail) {
            return Err(io::Error::other(
                "the redirect needs a UNC folder with a server and a share",
            ));
        }
        let mut prefix = NT_UNC.to_vec();
        prefix.extend(tail);
        prefix
    } else if nt_unc {
        let tail = &units[NT_UNC.len()..];
        if !has_unc_server_share(tail) {
            return Err(io::Error::other(
                "the redirect needs a UNC folder with a server and a share",
            ));
        }
        units
    } else {
        return Err(io::Error::other("the redirect needs an absolute folder"));
    };

    if prefix.len() > logic::PREFIX_UNITS {
        return Err(io::Error::other(
            "the folder path is too long for the redirect",
        ));
    }

    Ok(prefix)
}

/// Unmaps the child's mappings if `install` returns early. A successful install disowns this: the child still needs the
/// mappings after the launcher returns, and the handover file owns their meaning from here.
struct Mappings {
    process: HANDLE,
    image: Option<(usize, usize)>,
    trampolines: Option<(usize, usize)>,
}

impl Mappings {
    fn new(process: HANDLE) -> Self {
        Self {
            process,
            image: None,
            trampolines: None,
        }
    }

    fn track_image(&mut self, address: usize, size: usize) {
        self.image = Some((address, size));
    }

    fn track_trampolines(&mut self, address: usize, size: usize) {
        self.trampolines = Some((address, size));
    }
}

impl Drop for Mappings {
    fn drop(&mut self) {
        for region in [self.image, self.trampolines].into_iter().flatten() {
            let (address, size) = region;
            debug_assert!(size > 0, "a tracked mapping is empty");
            // SAFETY: both regions were allocated in the child by `allocate` and are unused after this point.
            unsafe { VirtualFreeEx(self.process, address as *mut c_void, 0, MEM_RELEASE) };
        }
    }
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
    // A 32-bit child's exports are read once in bulk. The x64 path keeps its process-local export lookup.
    let stub_addresses = match bits {
        Bits::X64 => None,
        Bits::X86 => Some(export_stubs(process, ntdll)?),
    };

    for (index, name) in STUBS.iter().enumerate() {
        let address = match stub_addresses {
            Some(addresses) => addresses[index],
            None => ntdll_export(name)?,
        };
        let original = read_code(process, address, bits.stub_length())?;

        if !is_accepted_stub(bits, &original, index) {
            return Err(io::Error::other(format!(
                "the child's {name} has a stub shape the launcher does not know"
            )));
        }

        originals.push((address, original));
    }

    let mut mappings = Mappings::new(process);
    let image = map_image(process, bits, &mut mappings)?;
    let trampolines_size = STUBS.len() * TRAMPOLINE_SLOT;
    let trampolines = allocate(process, trampolines_size)?;
    mappings.track_trampolines(trampolines, trampolines_size);

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

    // Both mappings outlive this function: the child still runs code in them, and the handover file tells the hook what
    // they mean. Dropping the guard here would unmap them under a live child.
    std::mem::forget(mappings);
    Ok(Patched {
        #[cfg(all(test, target_arch = "x86_64"))]
        process,
        config_address,
        #[cfg(all(test, target_arch = "x86_64"))]
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
/// check of the shared user data between them (Windows 10 and 11) is allowed, not required. A RIP-relative memory
/// operand is refused: the trampoline is a copy at a different address, so its displacement would name a different
/// address than the original did. The decoder identifies that operand reliably, so no byte pattern is guessed.
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
    let no_ip_relative_memory = instructions
        .iter()
        .all(|instruction| !matches!(instruction.memory_base(), Register::RIP | Register::EIP));
    copies_first_argument && loads_number && system_call && no_ip_relative_memory
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

/// Checks that the child's ntdll is the one this process uses: its header carries the same timestamp.
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

/// The PE fields shared by the image scan, the 32-bit export lookup and the DLL mapper. `bits` selects the optional
/// header layout and, when the image is one the launcher maps, the structural checks a malformed header must pass.
#[derive(Debug)]
struct PeHeaders {
    machine: u16,
    magic: u16,
    section_count: usize,
    optional_size: usize,
    preferred_base: u64,
    image_size: usize,
    headers_size: usize,
    directory_count: u32,
    export: (usize, usize),
    section_table: usize,
}

/// Decodes a PE header with the bounds-checked readers. With `Some(bits)` the PE signature and section table must be
/// valid; architecture and magic are checked by the callers, which have different failures for them. With `None` the
/// parse is best effort: it uses the PE32 layout and stops at the export RVA, as the image-region scan always did.
fn pe_headers(bytes: &[u8], bits: Option<Bits>) -> io::Result<PeHeaders> {
    let nt = u32le(bytes, 0x3C)? as usize;

    if bits.is_some() && bytes.get(nt..image_at(nt, 4)?) != Some(b"PE\0\0".as_slice()) {
        return Err(io::Error::other(
            "the stage 0 DLL is not the PE image for this child",
        ));
    }

    let machine = u16le(bytes, image_at(nt, 4)?)?;
    let section_count = usize::from(u16le(bytes, image_at(nt, 6)?)?);
    let optional_size = usize::from(u16le(bytes, image_at(nt, 20)?)?);
    let optional = image_at(nt, 24)?;
    let magic = u16le(bytes, optional)?;

    let layout = bits.unwrap_or(Bits::X86);
    let preferred_base = match layout {
        Bits::X64 => u64le(bytes, image_at(optional, 24)?)?,
        Bits::X86 => u64::from(u32le(bytes, image_at(optional, 28)?)?),
    };
    let image_size = u32le(bytes, image_at(optional, 56)?)? as usize;
    let headers_size = u32le(bytes, image_at(optional, 60)?)? as usize;
    let directory_count = u32le(bytes, image_at(optional, 92)?)?;
    let directories = image_at(optional, layout.directories())?;
    let export_rva = u32le(bytes, directories)? as usize;
    // The best-effort scan never read the directory's size; adding that read could reject a header it used to accept.
    let export_size = match bits {
        Some(_) => u32le(bytes, image_at(directories, 4)?)? as usize,
        None => 0,
    };
    let section_table = image_at(optional, optional_size)?;

    if bits.is_some() {
        let table = 40usize
            .checked_mul(section_count)
            .and_then(|length| section_table.checked_add(length))
            .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))?;
        if table > bytes.len() {
            return Err(io::Error::other("the stage 0 DLL is truncated"));
        }
    }

    Ok(PeHeaders {
        machine,
        magic,
        section_count,
        optional_size,
        preferred_base,
        image_size,
        headers_size,
        directory_count,
        export: (export_rva, export_size),
        section_table,
    })
}

/// Lays an embedded DLL out by RVA: the headers, then each section's bytes at its virtual address. Returns the image
/// and its size. A section that reaches past the image is clipped, exactly as `map_image` did before.
fn layout_image(bytes: &[u8], bits: Bits) -> io::Result<(Vec<u8>, usize)> {
    let headers = pe_headers(bytes, Some(bits))?;
    let mut image = vec![0u8; headers.image_size];
    // The embedded image is trusted to be well formed, but a broken build must not panic in the launcher.
    if headers.headers_size > image.len() || headers.headers_size > bytes.len() {
        return Err(io::Error::other("the stage 0 DLL is truncated"));
    }
    image[..headers.headers_size].copy_from_slice(&bytes[..headers.headers_size]);

    for index in 0..headers.section_count {
        let header = headers.section_table + 40 * index;
        let size = u32le(bytes, header + 16)? as usize;
        let raw = u32le(bytes, header + 20)? as usize;
        let address = u32le(bytes, header + 12)? as usize;
        let length = size.min(image.len().saturating_sub(address));

        if length > 0 {
            let end = raw
                .checked_add(length)
                .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))?;
            let source = bytes
                .get(raw..end)
                .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))?;
            let target = image
                .get_mut(address..address + length)
                .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))?;
            target.copy_from_slice(source);
        }
    }

    Ok((image, headers.image_size))
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

    let directory = pe_headers(&header, None).ok()?.export.0;

    if directory == 0 {
        return None;
    }

    let export = read_code(process, base + directory, 16).ok()?;
    let name_rva = u32le(&export, 12).ok()? as usize;
    cstring(process, base + name_rva)
}

/// The largest export table scanned in a child's image. An ntdll has a few thousand entries; a larger table is refused
/// instead of being copied into the launcher.
const MAX_EXPORT_NAMES: usize = 64 * 1024;
/// The largest window of export name strings read in one call.
const MAX_EXPORT_STRINGS: usize = 1024 * 1024;
/// How many bytes of an export name are read, matching the old per-name lookup.
const NAME_BYTES: usize = 64;

/// Adds an offset inside a child's image, refusing an overflow before it can wrap on a 32-bit launcher.
fn image_at(base: usize, offset: usize) -> io::Result<usize> {
    base.checked_add(offset)
        .ok_or_else(|| io::Error::other("an image offset overflows"))
}

/// The array RVAs and counts of an export directory, every range checked against the image size.
#[derive(Clone, Copy)]
struct ExportArrays {
    count: usize,
    function_count: usize,
    functions: usize,
    names: usize,
    ordinals: usize,
}

/// Reads the counts and array RVAs of an export directory. All values come from the child and are bounded before any
/// remote array is read.
fn export_arrays(directory: &[u8], image_size: usize) -> io::Result<ExportArrays> {
    let count = u32le(directory, 24)? as usize;
    let function_count = u32le(directory, 20)? as usize;
    let functions = u32le(directory, 28)? as usize;
    let names = u32le(directory, 32)? as usize;
    let ordinals = u32le(directory, 36)? as usize;

    if count == 0 || count > MAX_EXPORT_NAMES || function_count > MAX_EXPORT_NAMES {
        return Err(io::Error::other(
            "the child's ntdll has an invalid export table",
        ));
    }

    for (rva, width, entries) in [
        (names, 4usize, count),
        (ordinals, 2usize, count),
        (functions, 4usize, function_count),
    ] {
        let length = width
            .checked_mul(entries)
            .ok_or_else(|| io::Error::other("the child's export table is too large"))?;
        if image_at(rva, length)? > image_size {
            return Err(io::Error::other(
                "an export array is outside the child's image",
            ));
        }
    }

    Ok(ExportArrays {
        count,
        function_count,
        functions,
        names,
        ordinals,
    })
}

/// Checks the bulk-read name pointers and returns the RVA and length of one window holding every name string.
fn export_window(
    arrays: &ExportArrays,
    name_pointers: &[u8],
    image_size: usize,
) -> io::Result<(usize, usize)> {
    let expected = arrays
        .count
        .checked_mul(4)
        .ok_or_else(|| io::Error::other("the child's export table is too large"))?;
    if name_pointers.len() != expected {
        return Err(io::Error::other(
            "the child's export name array is truncated",
        ));
    }

    let mut low = usize::MAX;
    let mut high = 0;

    for pointer in name_pointers.as_chunks::<4>().0 {
        let rva = u32::from_le_bytes(*pointer) as usize;
        if image_at(rva, NAME_BYTES)? > image_size {
            return Err(io::Error::other(
                "an export name is outside the child's image",
            ));
        }
        low = low.min(rva);
        high = high.max(rva);
    }

    let end = image_at(high, NAME_BYTES)?;
    let length = end
        .checked_sub(low)
        .ok_or_else(|| io::Error::other("the child's export names have an invalid range"))?;
    if length > MAX_EXPORT_STRINGS {
        return Err(io::Error::other(
            "the child's export names are too spread out",
        ));
    }
    Ok((low, length))
}

/// The indices of the wanted names in a bulk-read export name table. The first entry with a given name wins, and the
/// scan stops once all four names have been found. Names are compared byte for byte, case sensitively.
fn stub_indices(
    wanted: &[&str; 4],
    name_pointers: &[u8],
    strings: &[u8],
    strings_rva: usize,
) -> [Option<usize>; 4] {
    let mut found = [None; 4];

    for (index, pointer) in name_pointers.as_chunks::<4>().0.iter().enumerate() {
        if found.iter().all(Option::is_some) {
            break;
        }
        let rva = u32::from_le_bytes(*pointer) as usize;
        let Some(offset) = rva.checked_sub(strings_rva) else {
            continue;
        };
        let Some(bytes) = offset
            .checked_add(NAME_BYTES)
            .and_then(|end| strings.get(offset..end))
        else {
            continue;
        };
        let end = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        let name = &bytes[..end];

        for (slot, candidate) in wanted.iter().enumerate() {
            if found[slot].is_none() && name == candidate.as_bytes() {
                found[slot] = Some(index);
            }
        }
    }

    found
}

/// The addresses of the four loader stubs of a 32-bit image in the child. The header and the export arrays are read
/// once in bulk; only the found entries' names, ordinals and functions are read from the child individually.
fn export_stubs(process: HANDLE, base: usize) -> io::Result<[usize; STUBS.len()]> {
    let missing = |name: &str| io::Error::other(format!("the child's ntdll has no {name}"));

    let header = read_code(process, base, 0x1000)?;
    if !header.starts_with(b"MZ") {
        return Err(io::Error::other("the child's ntdll has no DOS header"));
    }
    let nt = u32le(&header, 0x3C)? as usize;
    if !header
        .get(nt..)
        .is_some_and(|tail| tail.starts_with(b"PE\0\0"))
    {
        return Err(io::Error::other("the child's ntdll has no PE header"));
    }
    let headers = pe_headers(&header, None)?;
    let image_size = headers.image_size;
    if headers.magic != Bits::X86.magic() {
        return Err(io::Error::other("the child's ntdll is not a 32-bit image"));
    }
    if headers.directory_count == 0 {
        return Err(io::Error::other(
            "the child's ntdll has no data directories",
        ));
    }
    let directory = headers.export.0;
    if directory == 0 {
        return Err(missing(STUBS[0]));
    }
    if image_at(directory, 40)? > headers.image_size {
        return Err(io::Error::other(
            "the child's export directory is outside the image",
        ));
    }

    let export = read_code(process, image_at(base, directory)?, 40)?;
    let arrays = export_arrays(&export, image_size)?;
    let name_pointers = read_code(process, image_at(base, arrays.names)?, arrays.count * 4)?;
    let (strings_rva, strings_len) = export_window(&arrays, &name_pointers, image_size)?;
    let strings = read_code(process, image_at(base, strings_rva)?, strings_len)?;
    let found = stub_indices(&STUBS, &name_pointers, &strings, strings_rva);

    let mut addresses = [0; STUBS.len()];
    for (slot, name) in STUBS.iter().enumerate() {
        let Some(index) = found[slot] else {
            return Err(missing(name));
        };

        // Confirm and resolve only this entry, with the same four reads the old per-name scan made for a hit.
        let name_rva = u32le(
            &read_code(
                process,
                image_at(base, image_at(arrays.names, index * 4)?)?,
                4,
            )?,
            0,
        )? as usize;
        let bytes = read_code(process, image_at(base, name_rva)?, NAME_BYTES)?;
        let end = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        if &bytes[..end] != name.as_bytes() {
            return Err(missing(name));
        }

        let ordinal = usize::from(u16le(
            &read_code(
                process,
                image_at(base, image_at(arrays.ordinals, index * 2)?)?,
                2,
            )?,
            0,
        )?);
        if ordinal >= arrays.function_count {
            return Err(io::Error::other(
                "an export ordinal is outside the child's image",
            ));
        }
        let rva = u32le(
            &read_code(
                process,
                image_at(base, image_at(arrays.functions, ordinal * 4)?)?,
                4,
            )?,
            0,
        )? as usize;
        if image_at(rva, Bits::X86.stub_length())? > image_size {
            return Err(io::Error::other(
                "an export function is outside the child's image",
            ));
        }
        addresses[slot] = image_at(base, rva)?;
    }

    Ok(addresses)
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
fn map_image(process: HANDLE, bits: Bits, mappings: &mut Mappings) -> io::Result<Image> {
    let bytes = bits.dll();

    if bytes.is_empty() {
        return Err(io::Error::other(
            "this build has no stage 0 DLL for this child",
        ));
    }

    let headers = pe_headers(bytes, Some(bits))?;

    // The architecture checks stay here: the image scan uses a best-effort parse and the child's ntdll has its own
    // error messages for the same fields.
    if headers.machine != bits.machine() {
        return Err(io::Error::other(
            "the stage 0 DLL is not the PE image for this child",
        ));
    }
    if headers.magic != bits.magic() {
        return Err(io::Error::other(
            "the stage 0 DLL has the wrong optional header",
        ));
    }

    // The section table starts after the optional header, so the data directories can be found from its position.
    let optional = headers
        .section_table
        .checked_sub(headers.optional_size)
        .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))?;
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

    let base = allocate(process, headers.image_size)?;
    mappings.track_image(base, headers.image_size);
    let (mut image, _) = layout_image(bytes, bits)?;
    let delta = (base as u64).wrapping_sub(headers.preferred_base);

    let (relocations, relocations_size) = directory(5)?;
    let mut offset = 0;

    while offset < relocations_size {
        let page = u32le(&image, relocations + offset)? as usize;
        let block = u32le(&image, relocations + offset + 4)? as usize;

        // A zero-size block would leave `offset` unchanged and hang the launcher. The DLL is embedded at build time,
        // so a malformed block means a broken build: fail here instead of hanging a child.
        if block < 8 {
            return Err(io::Error::other(
                "the stage 0 DLL has a malformed relocation block",
            ));
        }

        for entry in 0..(block - 8) / 2 {
            let value = u16le(&image, relocations + offset + 8 + 2 * entry)?;
            let at = page + usize::from(value & 0xFFF);

            match (value >> 12, bits) {
                (0, _) => {}
                (10, Bits::X64) => {
                    let old = u64le(&image, at)?;
                    let slot = image.get_mut(at..at + 8).ok_or_else(|| {
                        io::Error::other("the stage 0 DLL has a relocation outside the image")
                    })?;
                    slot.copy_from_slice(&old.wrapping_add(delta).to_le_bytes());
                }
                (3, Bits::X86) => {
                    let old = u32le(&image, at)?;
                    let slot = image.get_mut(at..at + 4).ok_or_else(|| {
                        io::Error::other("the stage 0 DLL has a relocation outside the image")
                    })?;
                    slot.copy_from_slice(&old.wrapping_add(delta as u32).to_le_bytes());
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
/// decoration is removed, so the names are the ones the launcher asks for. The counts and array RVAs come from the
/// same checked decoder the child's ntdll uses; only the name strings are read here.
fn read_exports(image: &[u8], directory: usize) -> io::Result<HashMap<String, usize>> {
    if directory == 0 {
        return Err(io::Error::other("the stage 0 DLL exports nothing"));
    }

    // Slice first so a directory at the end of the image keeps the field reads' truncation error.
    let directory = image
        .get(directory..)
        .ok_or_else(|| io::Error::other("the stage 0 DLL is truncated"))?;
    let arrays = export_arrays(directory, image.len())?;
    let mut exports = HashMap::new();

    for index in 0..arrays.count {
        let name_rva = u32le(image, arrays.names + 4 * index)? as usize;
        let ordinal = usize::from(u16le(image, arrays.ordinals + 2 * index)?);
        let rva = u32le(image, arrays.functions + 4 * ordinal)? as usize;
        let tail = image
            .get(name_rva..)
            .ok_or_else(|| io::Error::other("an export name is outside the image"))?;
        let end = tail
            .iter()
            .position(|&byte| byte == 0)
            .ok_or_else(|| io::Error::other("an export name is not terminated"))?;
        let raw = String::from_utf8_lossy(&tail[..end]).into_owned();
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
    at.checked_add(N)
        .and_then(|end| bytes.get(at..end))
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

    const SYNTHETIC_DIRECTORY: usize = 0x1000;
    const SYNTHETIC_FUNCTIONS: usize = 0x1100;
    const SYNTHETIC_NAMES: usize = 0x1200;
    const SYNTHETIC_ORDINALS: usize = 0x1300;
    const SYNTHETIC_STRINGS: usize = 0x2000;
    const SYNTHETIC_SIZE: usize = 0x5000;

    /// Lays an export directory out by RVA: seven names, seven functions, and ordinals that are not in name order.
    fn synthetic_exports() -> Vec<u8> {
        let names = [
            "OtherExport",
            "NtCreateFile",
            "ntopenfile",
            "NtOpenFile",
            "NtOpenFileEx",
            "NtQueryAttributesFile",
            "NtQueryFullAttributesFile",
        ];
        let ordinals = [0u16, 3, 1, 2, 4, 5, 6];
        let mut image = vec![0u8; SYNTHETIC_SIZE];

        let mut string_rva = SYNTHETIC_STRINGS;
        for (index, name) in names.iter().enumerate() {
            image[SYNTHETIC_NAMES + 4 * index..SYNTHETIC_NAMES + 4 * index + 4]
                .copy_from_slice(&(string_rva as u32).to_le_bytes());
            image[SYNTHETIC_ORDINALS + 2 * index..SYNTHETIC_ORDINALS + 2 * index + 2]
                .copy_from_slice(&ordinals[index].to_le_bytes());
            image[string_rva..string_rva + name.len()].copy_from_slice(name.as_bytes());
            string_rva += name.len() + 1;
        }

        for index in 0..ordinals.len() {
            let rva = 0x4000 + 0x10 * index;
            image[SYNTHETIC_FUNCTIONS + 4 * index..SYNTHETIC_FUNCTIONS + 4 * index + 4]
                .copy_from_slice(&(rva as u32).to_le_bytes());
        }

        let put_u32 = |image: &mut [u8], at: usize, value: u32| {
            image[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put_u32(
            &mut image,
            SYNTHETIC_DIRECTORY + 12,
            SYNTHETIC_STRINGS as u32,
        );
        put_u32(&mut image, SYNTHETIC_DIRECTORY + 16, 1);
        put_u32(&mut image, SYNTHETIC_DIRECTORY + 20, ordinals.len() as u32);
        put_u32(&mut image, SYNTHETIC_DIRECTORY + 24, names.len() as u32);
        put_u32(
            &mut image,
            SYNTHETIC_DIRECTORY + 28,
            SYNTHETIC_FUNCTIONS as u32,
        );
        put_u32(&mut image, SYNTHETIC_DIRECTORY + 32, SYNTHETIC_NAMES as u32);
        put_u32(
            &mut image,
            SYNTHETIC_DIRECTORY + 36,
            SYNTHETIC_ORDINALS as u32,
        );
        image
    }

    /// Carves the three bulk reads the launcher makes out of the synthetic image.
    fn synthetic_slices(image: &[u8]) -> (&[u8], &[u8], usize) {
        let arrays = export_arrays(
            &image[SYNTHETIC_DIRECTORY..SYNTHETIC_DIRECTORY + 40],
            SYNTHETIC_SIZE,
        )
        .unwrap();
        let pointers = &image[arrays.names..arrays.names + arrays.count * 4];
        let (rva, length) = export_window(&arrays, pointers, SYNTHETIC_SIZE).unwrap();
        (pointers, &image[rva..rva + length], rva)
    }

    #[test]
    fn the_bulk_matcher_finds_the_four_stubs_by_their_exact_names() {
        let image = synthetic_exports();
        let (pointers, strings, strings_rva) = synthetic_slices(&image);
        let found = stub_indices(&STUBS, pointers, strings, strings_rva);
        assert_eq!(found, [Some(3), Some(1), Some(5), Some(6)]);

        // The ordinals are deliberately not in name order: each function must come from its own entry.
        for (slot, index) in found.into_iter().enumerate() {
            let index = index.unwrap();
            let ordinal = u16::from_le_bytes(
                image[SYNTHETIC_ORDINALS + 2 * index..SYNTHETIC_ORDINALS + 2 * index + 2]
                    .try_into()
                    .unwrap(),
            );
            let function = u32::from_le_bytes(
                image[SYNTHETIC_FUNCTIONS + 4 * ordinal as usize
                    ..SYNTHETIC_FUNCTIONS + 4 * ordinal as usize + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(function as usize, 0x4000 + 0x10 * [2, 3, 5, 6][slot]);
        }

        let mut wanted = STUBS;
        wanted[0] = "NtOpenFileMissing";
        assert_eq!(
            stub_indices(&wanted, pointers, strings, strings_rva)[0],
            None
        );
    }

    #[test]
    fn the_bulk_matcher_ignores_name_pointers_outside_the_string_window() {
        let image = synthetic_exports();
        let (pointers, strings, _) = synthetic_slices(&image);
        assert_eq!(stub_indices(&STUBS, pointers, &strings[..8], 0), [None; 4]);
    }

    #[test]
    fn export_arrays_refuse_counts_and_ranges_outside_the_image() {
        let image = synthetic_exports();
        let directory = &image[SYNTHETIC_DIRECTORY..SYNTHETIC_DIRECTORY + 40];
        let arrays = export_arrays(directory, SYNTHETIC_SIZE).unwrap();
        assert_eq!(arrays.count, 7);
        assert_eq!(arrays.function_count, 7);
        assert_eq!(arrays.names, SYNTHETIC_NAMES);
        assert!(export_arrays(directory, SYNTHETIC_FUNCTIONS + 27).is_err());
        assert!(export_arrays(directory, 0).is_err());

        let mut too_many = directory.to_vec();
        too_many[24..28].copy_from_slice(&((MAX_EXPORT_NAMES + 1) as u32).to_le_bytes());
        assert!(export_arrays(&too_many, SYNTHETIC_SIZE).is_err());
    }

    #[test]
    fn read_exports_uses_the_shared_export_array_decoder() {
        let image = synthetic_exports();
        let directory = &image[SYNTHETIC_DIRECTORY..SYNTHETIC_DIRECTORY + 40];
        let arrays = export_arrays(directory, image.len()).unwrap();
        let exports = read_exports(&image, SYNTHETIC_DIRECTORY).unwrap();
        assert_eq!(exports.len(), arrays.count);

        // Every entry the shared decoder describes must appear with the same name and function RVA.
        for index in 0..arrays.count {
            let name_rva = u32le(&image, arrays.names + 4 * index).unwrap() as usize;
            let ordinal = usize::from(u16le(&image, arrays.ordinals + 2 * index).unwrap());
            let rva = u32le(&image, arrays.functions + 4 * ordinal).unwrap() as usize;
            let end = image[name_rva..]
                .iter()
                .position(|&byte| byte == 0)
                .unwrap();
            let name = std::str::from_utf8(&image[name_rva..name_rva + end]).unwrap();
            assert_eq!(exports.get(undecorate(name).as_str()), Some(&rva));
        }
        assert_eq!(exports["OtherExport"], 0x4000);
        assert_eq!(exports["ntopenfile"], 0x4010);
        assert_eq!(exports["NtOpenFile"], 0x4020);
        assert_eq!(exports["NtCreateFile"], 0x4030);
        assert_eq!(
            read_exports(&image, 0).unwrap_err().to_string(),
            "the stage 0 DLL exports nothing"
        );
    }

    #[test]
    fn the_export_string_window_is_bounded_and_inside_the_image() {
        let image = synthetic_exports();
        let directory = &image[SYNTHETIC_DIRECTORY..SYNTHETIC_DIRECTORY + 40];
        let arrays = export_arrays(directory, SYNTHETIC_SIZE).unwrap();
        let pointers = &image[arrays.names..arrays.names + arrays.count * 4];
        assert_eq!(
            export_window(&arrays, pointers, SYNTHETIC_SIZE).unwrap().0,
            SYNTHETIC_STRINGS
        );
        assert!(export_window(&arrays, pointers, SYNTHETIC_STRINGS + NAME_BYTES - 1).is_err());

        let mut outside = pointers.to_vec();
        outside[0..4].copy_from_slice(&(SYNTHETIC_SIZE as u32).to_le_bytes());
        assert!(export_window(&arrays, &outside, SYNTHETIC_SIZE).is_err());

        let mut spread = pointers.to_vec();
        spread[0..4].copy_from_slice(&0u32.to_le_bytes());
        spread[4..8].copy_from_slice(&((MAX_EXPORT_STRINGS + 1) as u32).to_le_bytes());
        assert!(export_window(&arrays, &spread, MAX_EXPORT_STRINGS + NAME_BYTES + 2).is_err());
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
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
    fn a_rip_relative_operand_in_the_stub_prefix_is_refused() {
        // `test byte ptr [rip+0x7FFE0308], 1` (mod=00, rm=101) would point into the trampoline instead of at the
        // shared user data the original stub reads. The absolute SIB form in the fixture above is still accepted.
        let stub = [
            0x4C, 0x8B, 0xD1, 0xB8, 0x33, 0x00, 0x00, 0x00, 0xF6, 0x05, 0x08, 0x03, 0xFE, 0x7F,
            0x01, 0x75, 0x03, 0x0F, 0x05, 0xC3, 0x90, 0x90,
        ];
        assert!(!is_accepted_stub(Bits::X64, &stub, 0));
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

    /// A minimal PE image with one section, in either optional-header layout.
    fn synthetic_pe(bits: Bits) -> Vec<u8> {
        const NT: usize = 0x40;
        let optional_size = match bits {
            Bits::X64 => 240,
            Bits::X86 => 224,
        };
        let optional = NT + 24;
        let section_table = optional + optional_size;
        let mut bytes = vec![0u8; section_table + 40];

        let put_u16 = |bytes: &mut [u8], at: usize, value: u16| {
            bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        let put_u32 = |bytes: &mut [u8], at: usize, value: u32| {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        let put_u64 = |bytes: &mut [u8], at: usize, value: u64| {
            bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
        };

        bytes[..2].copy_from_slice(b"MZ");
        put_u32(&mut bytes, 0x3C, NT as u32);
        bytes[NT..NT + 4].copy_from_slice(b"PE\0\0");
        put_u16(&mut bytes, NT + 4, bits.machine());
        put_u16(&mut bytes, NT + 6, 1);
        put_u16(&mut bytes, NT + 20, optional_size as u16);
        put_u16(&mut bytes, optional, bits.magic());

        let preferred = match bits {
            Bits::X64 => 0x1800_0000u64,
            Bits::X86 => 0x0100_0000u64,
        };
        match bits {
            Bits::X64 => put_u64(&mut bytes, optional + 24, preferred),
            Bits::X86 => put_u32(&mut bytes, optional + 28, preferred as u32),
        }
        put_u32(&mut bytes, optional + 56, 0x5000);
        put_u32(&mut bytes, optional + 60, 0x400);
        put_u32(&mut bytes, optional + 92, 16);
        let directories = optional + bits.directories();
        put_u32(&mut bytes, directories, 0x2000);
        put_u32(&mut bytes, directories + 4, 0x80);
        put_u32(&mut bytes, section_table + 12, 0x1000);
        put_u32(&mut bytes, section_table + 16, 0x200);
        put_u32(&mut bytes, section_table + 20, 0x400);
        bytes
    }

    #[test]
    fn pe_headers_read_both_optional_header_layouts() {
        for bits in [Bits::X64, Bits::X86] {
            let bytes = synthetic_pe(bits);
            let headers = pe_headers(&bytes, Some(bits)).unwrap();
            assert_eq!(headers.machine, bits.machine());
            assert_eq!(headers.magic, bits.magic());
            assert_eq!(headers.section_count, 1);
            assert_eq!(
                headers.optional_size,
                if bits == Bits::X64 { 240 } else { 224 }
            );
            assert_eq!(
                headers.preferred_base,
                match bits {
                    Bits::X64 => 0x1800_0000,
                    Bits::X86 => 0x0100_0000,
                }
            );
            assert_eq!(headers.image_size, 0x5000);
            assert_eq!(headers.headers_size, 0x400);
            assert_eq!(headers.directory_count, 16);
            assert_eq!(headers.export, (0x2000, 0x80));
            assert_eq!(headers.section_table, 0x40 + 24 + headers.optional_size);
        }
    }

    #[test]
    fn pe_headers_refuse_a_truncated_optional_header() {
        let bytes = synthetic_pe(Bits::X64);
        let error = pe_headers(&bytes[..0x40 + 24], Some(Bits::X64)).unwrap_err();
        assert_eq!(error.to_string(), "the stage 0 DLL is truncated");
    }

    #[test]
    fn pe_headers_check_the_signature_only_for_an_image_it_maps() {
        let mut bytes = synthetic_pe(Bits::X86);
        bytes[0x40..0x44].copy_from_slice(b"PX\0\0");
        let error = pe_headers(&bytes, Some(Bits::X86)).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the stage 0 DLL is not the PE image for this child"
        );
        // A foreign image is scanned by RVA, without the mapper's signature check.
        assert_eq!(pe_headers(&bytes, None).unwrap().export.0, 0x2000);
    }

    #[test]
    fn pe_headers_refuse_a_section_table_past_the_buffer() {
        let bytes = synthetic_pe(Bits::X86);
        let truncated = &bytes[..0x40 + 24 + 224 + 39];
        let error = pe_headers(truncated, Some(Bits::X86)).unwrap_err();
        assert_eq!(error.to_string(), "the stage 0 DLL is truncated");
        assert_eq!(pe_headers(truncated, None).unwrap().section_count, 1);
    }

    #[test]
    fn layout_image_refuses_a_header_copy_outside_the_image_or_the_file() {
        let mut bytes = synthetic_pe(Bits::X86);
        let optional = 0x40 + 24;
        let section = optional + 224;
        // Keep the section copy in range so only the header range is under test.
        bytes[section + 16..section + 20].copy_from_slice(&0x40u32.to_le_bytes());
        bytes[section + 20..section + 24].copy_from_slice(&0x100u32.to_le_bytes());

        bytes[optional + 60..optional + 64].copy_from_slice(&0x100u32.to_le_bytes());
        assert!(layout_image(&bytes, Bits::X86).is_ok());

        // The header fits the image but reaches past the embedded file.
        let past_file = bytes.len() as u32 + 1;
        bytes[optional + 60..optional + 64].copy_from_slice(&past_file.to_le_bytes());
        assert_eq!(
            layout_image(&bytes, Bits::X86).unwrap_err().to_string(),
            "the stage 0 DLL is truncated"
        );

        // The file is long enough now, but the header reaches past the image.
        bytes.resize(0x5002, 0);
        bytes[optional + 60..optional + 64].copy_from_slice(&0x5001u32.to_le_bytes());
        assert_eq!(
            layout_image(&bytes, Bits::X86).unwrap_err().to_string(),
            "the stage 0 DLL is truncated"
        );
    }

    #[test]
    fn layout_image_refuses_a_section_whose_raw_range_is_past_the_buffer() {
        let mut bytes = synthetic_pe(Bits::X86);
        let optional = 0x40 + 24;
        let section = optional + 224;
        bytes[optional + 60..optional + 64].copy_from_slice(&0x100u32.to_le_bytes());

        // A section with a valid virtual address but raw data starting at the end of the file.
        let past_file = bytes.len() as u32;
        bytes[section + 20..section + 24].copy_from_slice(&past_file.to_le_bytes());
        assert_eq!(
            layout_image(&bytes, Bits::X86).unwrap_err().to_string(),
            "the stage 0 DLL is truncated"
        );

        // A raw pointer near the top of the address space cannot be added to the length.
        bytes[section + 20..section + 24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            layout_image(&bytes, Bits::X86).unwrap_err().to_string(),
            "the stage 0 DLL is truncated"
        );

        // A section reaching past the image is still clipped, as it was before.
        bytes[section + 12..section + 16].copy_from_slice(&(0x5000u32 - 0x20).to_le_bytes());
        bytes[section + 20..section + 24].copy_from_slice(&0x100u32.to_le_bytes());
        let (image, size) = layout_image(&bytes, Bits::X86).unwrap();
        assert_eq!(image.len(), size);
        assert_eq!(&image[0x5000 - 0x20..], &bytes[0x100..0x120]);
    }

    /// Lays the embedded DLL out by RVA, the way `map_image` does in the child, so its exports and code can be read.
    fn mapped_dll(bits: Bits) -> Vec<u8> {
        let bytes = bits.dll();
        assert!(!bytes.is_empty(), "this build has no stage 0 DLL");
        let (image, image_size) = layout_image(bytes, bits).unwrap();
        assert_eq!(image.len(), image_size);
        image
    }

    #[test]
    fn the_embedded_dlls_pass_the_shared_export_decoder() {
        for bits in [Bits::X64, Bits::X86] {
            let bytes = bits.dll();
            if bytes.is_empty() {
                continue;
            }
            let image = mapped_dll(bits);
            let rva = pe_headers(bytes, Some(bits)).unwrap().export.0;
            let arrays = export_arrays(&image[rva..rva + 40], image.len()).unwrap();
            assert!(arrays.count > 0);
            assert!(read_exports(&image, rva).unwrap().len() >= STUBS.len());
        }
    }

    #[test]
    fn the_x86_shim_tails_remove_the_argument_bytes_the_launcher_checks() {
        let bytes = DLL_X86;
        let nt = u32le(bytes, 0x3C).unwrap() as usize;
        let directories = nt + 24 + Bits::X86.directories();
        let exports_rva = u32le(bytes, directories).unwrap() as usize;
        let image = mapped_dll(Bits::X86);
        let exports = read_exports(&image, exports_rva).unwrap();

        for (index, name) in SHIMS.iter().enumerate() {
            let rva = *exports
                .get(*name)
                .unwrap_or_else(|| panic!("{name} is not exported by the x86 stage 0 DLL"));
            let ret = Decoder::with_ip(32, &image[rva..], 0, DecoderOptions::NONE)
                .iter()
                .take_while(|instruction| !instruction.is_invalid())
                .find(|instruction| {
                    instruction.mnemonic() == Mnemonic::Ret
                        && instruction.op0_kind() == OpKind::Immediate16
                })
                .unwrap_or_else(|| panic!("{name} has no ret imm16"));
            assert_eq!(
                ret.immediate16(),
                ARGUMENT_BYTES_32[index],
                "{name} removes a different number of argument bytes"
            );
        }
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
    fn a_unc_folder_becomes_an_nt_unc_prefix() {
        let win32 = nt_prefix(Path::new(r"\\server\share\Games")).unwrap();
        let nt = nt_prefix(Path::new(r"\??\UNC\server\share\Games")).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&win32),
            r"\??\UNC\server\share\Games"
        );
        assert_eq!(nt, win32);
        assert!(nt_prefix(Path::new(r"\\server")).is_err());
        assert!(nt_prefix(Path::new(r"\\server\")).is_err());
        assert!(nt_prefix(Path::new(r"\??\UNC\share")).is_err());
    }

    #[test]
    fn the_configuration_starts_with_the_enabled_flag_it_is_read_with() {
        let config = redirect_config(
            Path::new(r"C:\Game"),
            Path::new(r"C:\Mods"),
            Path::new(r"C:\Writes"),
        )
        .unwrap();
        assert_eq!(config.enabled, 1);
        assert_eq!(
            config_bytes(&config).len(),
            std::mem::size_of::<logic::Config>()
        );
        assert_eq!(&config_bytes(&config)[..4], &1u32.to_le_bytes());
    }
}
