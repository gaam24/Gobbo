//! The machine code the launcher writes into a suspended process.
//!
//! The stub runs on the program's main thread before `LdrInitializeThunk`. It loads the hook, calls its
//! `GobboOverlayInit` export, and then returns to the instruction the thread was going to execute. The loader then
//! resolves the program's own static imports, which the launcher points at the layer. Imports below the program are
//! resolved by the first `LoadLibraryExW` and are not covered (see `ARCHITECTURE.md`, invariant 11).
//!
//! This module is safe Rust: it only builds bytes. Writing them into the target process is in `inject`.

/// Why a stub could not be built. The launcher reports it as an `io::Error`; it is not a game error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StubError {
    /// A patch did not fit the buffer the builder made.
    Layout,
    /// A relative jump from the stub did not fit its 32-bit displacement.
    Jump,
}

impl std::fmt::Display for StubError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout => formatter.write_str("the injection stub has an invalid layout"),
            Self::Jump => formatter.write_str("a jump in the injection stub is too long"),
        }
    }
}

/// The data block of the x64 stub, with addresses valid in the target process.
#[cfg(target_arch = "x86_64")]
pub(crate) struct Data64 {
    pub(crate) load_library_ex_w: u64,
    pub(crate) get_proc_address: u64,
    pub(crate) exit_process: u64,
    pub(crate) hook_path: Vec<u16>,
    pub(crate) init_name: &'static [u8],
    pub(crate) flags: u32,
}

/// The data block of the x86 stub, with addresses valid in the target process.
#[cfg(target_arch = "x86")]
pub(crate) struct Data32 {
    pub(crate) load_library_ex_w: u32,
    pub(crate) get_proc_address: u32,
    pub(crate) exit_process: u32,
    pub(crate) hook_path: Vec<u16>,
    pub(crate) init_name: &'static [u8],
    pub(crate) flags: u32,
}

/// A generated stub: the code, its data block, and the places the caller patches with the remote address.
pub(crate) struct Image {
    pub(crate) code: Vec<u8>,
    pub(crate) data: Vec<u8>,
    data_pointer_offset: usize,
    path_pointer_offset: usize,
    name_pointer_offset: usize,
    strings_offset: usize,
    name_offset: usize,
    pointer_size: usize,
}

impl Image {
    /// Number of bytes the image needs in the target process.
    pub(crate) fn len(&self) -> usize {
        self.code.len() + self.data.len()
    }

    /// Points the stub's data register at the data block at `data_base` in the target process.
    pub(crate) fn set_data_base(&mut self, data_base: usize) -> Result<(), StubError> {
        let slot = self
            .code
            .get_mut(self.data_pointer_offset..self.data_pointer_offset + self.pointer_size)
            .ok_or(StubError::Layout)?;
        write_pointer(slot, data_base, self.pointer_size)
    }

    /// Points the data block at the hook path and the init name inside the image.
    pub(crate) fn set_string_pointers(&mut self, data_base: usize) -> Result<(), StubError> {
        let path = data_base
            .checked_add(self.strings_offset)
            .ok_or(StubError::Layout)?;
        let name = data_base
            .checked_add(self.name_offset)
            .ok_or(StubError::Layout)?;
        let path_slot = self
            .data
            .get_mut(self.path_pointer_offset..self.path_pointer_offset + self.pointer_size)
            .ok_or(StubError::Layout)?;
        write_pointer(path_slot, path, self.pointer_size)?;
        let name_slot = self
            .data
            .get_mut(self.name_pointer_offset..self.name_pointer_offset + self.pointer_size)
            .ok_or(StubError::Layout)?;
        write_pointer(name_slot, name, self.pointer_size)
    }
}

/// Writes `value` as a little-endian pointer of `size` bytes (4 or 8).
fn write_pointer(slot: &mut [u8], value: usize, size: usize) -> Result<(), StubError> {
    let mut bytes = [0u8; 8];

    if size == 8 {
        bytes[..8].copy_from_slice(
            &u64::try_from(value)
                .map_err(|_| StubError::Layout)?
                .to_le_bytes(),
        );
    } else {
        bytes[..4].copy_from_slice(
            &u32::try_from(value)
                .map_err(|_| StubError::Layout)?
                .to_le_bytes(),
        );
    }

    let target = slot.get_mut(..size).ok_or(StubError::Layout)?;
    target.copy_from_slice(&bytes[..size]);
    Ok(())
}

/// Builds the x64 stub. `original_rip` is the instruction the thread would have run without the injection.
#[cfg(target_arch = "x86_64")]
pub(crate) fn build64(data: &Data64, original_rip: u64) -> Result<Image, StubError> {
    let mut code = Vec::with_capacity(160);
    let mut fixups = Vec::with_capacity(3);

    // Save everything the stub or the called functions may change. The thread starts with an arbitrary register state.
    code.push(0x9C);

    for push in [0x50u8, 0x51, 0x52, 0x53, 0x55, 0x56, 0x57] {
        code.push(push);
    }

    for push in 0x50u8..=0x57 {
        code.extend_from_slice(&[0x41, push]);
    }
    // RBP keeps the stack as it was after the pushes. The calls need a 16-byte aligned RSP and shadow space.
    code.extend_from_slice(&[0x48, 0x89, 0xE5]);
    code.extend_from_slice(&[0x48, 0x83, 0xE4, 0xF0]);
    code.extend_from_slice(&[0x48, 0x83, 0xEC, 0x20]);
    // RBX holds the data block; the immediate is patched after the image is placed in the target.
    code.extend_from_slice(&[0x48, 0xBB]);
    let data_pointer_offset = code.len();
    code.extend_from_slice(&[0; 8]);

    // LoadLibraryExW(path, NULL, flags).
    code.extend_from_slice(&[0x48, 0x8B, 0x4B, 0x18]);
    code.extend_from_slice(&[0x31, 0xD2]);
    code.extend_from_slice(&[0x44, 0x8B, 0x43, 0x28]);
    code.extend_from_slice(&[0x48, 0x8B, 0x43, 0x00]);
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.extend_from_slice(&[0x48, 0x85, 0xC0]);
    emit_jz(&mut code, &mut fixups);
    code.extend_from_slice(&[0x49, 0x89, 0xC4]);

    // GobboOverlayInit = GetProcAddress(module, "GobboOverlayInit").
    code.extend_from_slice(&[0x4C, 0x89, 0xE1]);
    code.extend_from_slice(&[0x48, 0x8B, 0x53, 0x20]);
    code.extend_from_slice(&[0x48, 0x8B, 0x43, 0x08]);
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.extend_from_slice(&[0x48, 0x85, 0xC0]);
    emit_jz(&mut code, &mut fixups);

    // Call it. EAX != 0 means the hooks are installed.
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.extend_from_slice(&[0x85, 0xC0]);
    emit_jz(&mut code, &mut fixups);

    // Success: restore the original stack, registers and flags, then continue at the original RIP.
    code.extend_from_slice(&[0x48, 0x89, 0xEC]);

    for register in [0x5Fu8, 0x5E, 0x5D, 0x5C, 0x5B, 0x5A, 0x59, 0x58] {
        code.extend_from_slice(&[0x41, register]);
    }

    for register in [0x5Fu8, 0x5E, 0x5D, 0x5B, 0x5A, 0x59, 0x58] {
        code.push(register);
    }

    code.push(0x9D);
    // jmp qword ptr [rip + 0]: the original RIP follows the instruction.
    code.extend_from_slice(&[0xFF, 0x25, 0x00, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&original_rip.to_le_bytes());

    // Failure: the process must not continue into the game without the overlay.
    let fail = code.len();
    code.push(0xB9);
    code.extend_from_slice(&1114u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x8B, 0x43, 0x10]);
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.push(0xCC);
    patch_jumps(&mut code, &fixups, fail)?;

    let mut strings = Vec::with_capacity(data.hook_path.len() * 2 + data.init_name.len());

    for unit in &data.hook_path {
        strings.extend_from_slice(&unit.to_le_bytes());
    }

    let strings_offset = 48;
    let name_offset = strings_offset + data.hook_path.len() * 2;
    let mut block = vec![0u8; strings_offset];
    block[0..8].copy_from_slice(&data.load_library_ex_w.to_le_bytes());
    block[8..16].copy_from_slice(&data.get_proc_address.to_le_bytes());
    block[16..24].copy_from_slice(&data.exit_process.to_le_bytes());
    block[40..44].copy_from_slice(&data.flags.to_le_bytes());
    block.extend_from_slice(&strings);
    block.extend_from_slice(data.init_name);

    Ok(Image {
        code,
        data: block,
        data_pointer_offset,
        path_pointer_offset: 24,
        name_pointer_offset: 32,
        strings_offset,
        name_offset,
        pointer_size: 8,
    })
}

/// Builds the x86 stub. `original_eip` is the instruction the thread would have run without the injection.
#[cfg(target_arch = "x86")]
pub(crate) fn build32(data: &Data32, original_eip: u32) -> Result<Image, StubError> {
    let mut code = Vec::with_capacity(120);
    let mut fixups: Vec<(usize, usize)> = Vec::with_capacity(3);

    code.push(0x60);
    code.push(0x9C);
    code.push(0xBB);
    let data_pointer_offset = code.len();
    code.extend_from_slice(&[0; 4]);

    // LoadLibraryExW(path, NULL, flags): stdcall, the callee removes the arguments.
    code.extend_from_slice(&[0xFF, 0x73, 0x14]);
    code.extend_from_slice(&[0x6A, 0x00]);
    code.extend_from_slice(&[0xFF, 0x73, 0x0C]);
    code.extend_from_slice(&[0x8B, 0x43, 0x00]);
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.extend_from_slice(&[0x85, 0xC0]);
    emit_jz_to(&mut code, &mut fixups, 0);
    code.extend_from_slice(&[0x89, 0xC6]);

    // GobboOverlayInit = GetProcAddress(module, "GobboOverlayInit").
    code.extend_from_slice(&[0xFF, 0x73, 0x10]);
    code.push(0x56);
    code.extend_from_slice(&[0x8B, 0x43, 0x04]);
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.extend_from_slice(&[0x85, 0xC0]);
    emit_jz_to(&mut code, &mut fixups, 1);

    // Call it. EAX != 0 means the hooks are installed.
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.extend_from_slice(&[0x85, 0xC0]);
    emit_jz_to(&mut code, &mut fixups, 2);

    // Success: restore the original stack, registers and flags, then continue at the original EIP.
    code.push(0x9D);
    code.push(0x61);
    code.push(0x68);
    code.extend_from_slice(&original_eip.to_le_bytes());
    code.push(0xC3);

    // Failure: the process must not continue into the game without the overlay. Each step has its own exit code.
    let fail_load = code.len();
    emit_failure(&mut code, 1114);
    let fail_proc = code.len();
    emit_failure(&mut code, 1115);
    let fail_init = code.len();
    emit_failure(&mut code, 1116);
    patch_jumps_to(&mut code, &fixups, &[fail_load, fail_proc, fail_init])?;

    let mut strings = Vec::with_capacity(data.hook_path.len() * 2 + data.init_name.len());

    for unit in &data.hook_path {
        strings.extend_from_slice(&unit.to_le_bytes());
    }

    let strings_offset = 24;
    let name_offset = strings_offset + data.hook_path.len() * 2;
    let mut block = vec![0u8; strings_offset];
    block[0..4].copy_from_slice(&data.load_library_ex_w.to_le_bytes());
    block[4..8].copy_from_slice(&data.get_proc_address.to_le_bytes());
    block[8..12].copy_from_slice(&data.exit_process.to_le_bytes());
    block[20..24].copy_from_slice(&data.flags.to_le_bytes());
    block.extend_from_slice(&strings);
    block.extend_from_slice(data.init_name);

    Ok(Image {
        code,
        data: block,
        data_pointer_offset,
        path_pointer_offset: 12,
        name_pointer_offset: 16,
        strings_offset,
        name_offset,
        pointer_size: 4,
    })
}

/// Writes a `jz rel32` whose target is fixed after the failure label is known.
#[cfg(target_arch = "x86_64")]
fn emit_jz(code: &mut Vec<u8>, fixups: &mut Vec<usize>) {
    code.extend_from_slice(&[0x0F, 0x84, 0, 0, 0, 0]);
    fixups.push(code.len() - 4);
}

/// Writes a `jz rel32` and records which of the failure labels it should point at.
#[cfg(target_arch = "x86")]
fn emit_jz_to(code: &mut Vec<u8>, fixups: &mut Vec<(usize, usize)>, target: usize) {
    code.extend_from_slice(&[0x0F, 0x84, 0, 0, 0, 0]);
    fixups.push((code.len() - 4, target));
}

/// Writes the 32-bit displacement of a jump from `position` to `target`.
#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
fn patch_relative(code: &mut [u8], position: usize, target: usize) -> Result<(), StubError> {
    let end = position + 4;
    let relative = target as isize - end as isize;
    let relative = i32::try_from(relative).map_err(|_| StubError::Jump)?;
    let slot = code.get_mut(position..end).ok_or(StubError::Layout)?;
    slot.copy_from_slice(&relative.to_le_bytes());
    Ok(())
}

/// Points the `jz` instructions at `target`.
#[cfg(target_arch = "x86_64")]
fn patch_jumps(code: &mut [u8], fixups: &[usize], target: usize) -> Result<(), StubError> {
    for &position in fixups {
        patch_relative(code, position, target)?;
    }

    Ok(())
}

/// Points each `jz` at the label its fixup names.
#[cfg(target_arch = "x86")]
fn patch_jumps_to(
    code: &mut [u8],
    fixups: &[(usize, usize)],
    labels: &[usize],
) -> Result<(), StubError> {
    for &(position, label) in fixups {
        let target = *labels.get(label).ok_or(StubError::Layout)?;
        patch_relative(code, position, target)?;
    }

    Ok(())
}

/// The failure path of the x86 stub: `push code; call ExitProcess; int3`.
#[cfg(target_arch = "x86")]
fn emit_failure(code: &mut Vec<u8>, code_value: u32) {
    code.push(0x68);
    code.extend_from_slice(&code_value.to_le_bytes());
    code.extend_from_slice(&[0x8B, 0x43, 0x08]);
    code.extend_from_slice(&[0xFF, 0xD0]);
    code.push(0xCC);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "x86_64")]
    fn data64() -> Data64 {
        Data64 {
            load_library_ex_w: 0x1111,
            get_proc_address: 0x2222,
            exit_process: 0x3333,
            hook_path: "C:\\hook.dll\0".encode_utf16().collect(),
            init_name: b"GobboOverlayInit\0",
            flags: 0x900,
        }
    }

    #[cfg(target_arch = "x86")]
    fn data32() -> Data32 {
        Data32 {
            load_library_ex_w: 0x1111,
            get_proc_address: 0x2222,
            exit_process: 0x3333,
            hook_path: "C:\\hook.dll\0".encode_utf16().collect(),
            init_name: b"GobboOverlayInit\0",
            flags: 0x900,
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn the_x64_image_has_room_for_its_strings_and_patches_its_data_pointer() {
        let mut image = build64(&data64(), 0x7fff_0000_1234).unwrap();
        image.set_data_base(0x1_0000_0000).unwrap();
        image
            .set_string_pointers(0x1_0000_0000 + image.code.len())
            .unwrap();
        assert!(image.len() > 48);
        assert!(image.data.len() > 48);
        assert_eq!(&image.data[40..44], &0x900u32.to_le_bytes());
    }

    #[cfg(target_arch = "x86")]
    #[test]
    fn the_x86_image_has_room_for_its_strings_and_patches_its_data_pointer() {
        let mut image = build32(&data32(), 0x1234_5678).unwrap();
        image.set_data_base(0x1000_0000).unwrap();
        image
            .set_string_pointers(0x1000_0000 + image.code.len())
            .unwrap();
        assert!(image.len() > 24);
        assert!(image.data.len() > 24);
        assert_eq!(&image.data[20..24], &0x900u32.to_le_bytes());
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn the_x64_image_contains_the_original_rip() {
        let image = build64(&data64(), 0x7fff_0000_1234).unwrap();
        let needle = 0x7fff_0000_1234u64.to_le_bytes();
        assert!(
            image
                .code
                .windows(needle.len())
                .any(|window| window == needle)
        );
    }

    #[cfg(target_arch = "x86")]
    #[test]
    fn the_x86_image_contains_the_original_eip() {
        let image = build32(&data32(), 0x1234_5678).unwrap();
        let needle = 0x1234_5678u32.to_le_bytes();
        assert!(
            image
                .code
                .windows(needle.len())
                .any(|window| window == needle)
        );
    }
}
