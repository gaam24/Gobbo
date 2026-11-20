//! Stage 0: the redirect DLL the launcher maps into a suspended child before its loader runs. The contract is in
//! `src-tauri/crates/overlay-hook/ARCHITECTURE.md` (section 3).
//!
//! The launcher patches the start of four ntdll loader stubs with a jump to one of the shims below. A shim saves the call's
//! arguments, lets `logic::redirect` decide, and calls the stub's trampoline (a copy of the original stub) with the
//! attributes it chose. Nothing here imports anything: the trampolines and the configuration are written by the launcher.
//! Built twice by `build.rs` of the launcher, for x64 and for x86, with `panic=abort` and without the C runtime.

#![no_std]

mod logic;

use core::arch::naked_asm;
use core::ffi::c_void;

use logic::{Config, Kind, QueryAttributes, Work};

/// The configuration the launcher writes before the loader runs. The launcher finds it by name in the export table.
#[unsafe(no_mangle)]
pub static mut GOBBO_STAGE0_CONFIG: Config = Config::OFF;

/// The frame of a shim, and the offsets in it: the saved arguments, the stack arguments of a 64-bit call, and the work
/// area. The outgoing arguments of a 64-bit call sit at the bottom of the frame, below `SAVED`.
const FRAME: usize = 0x600;
const SAVED: usize = 0x60;
#[cfg(target_arch = "x86_64")]
const STACK: usize = 0x80;
const WORK: usize = 0x100;

/// The offset of the first trampoline in `Config`. The trampolines are consecutive `u64` fields.
const TRAMPOLINES: usize = 16;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// The worker the shims call. It returns the attributes to pass on: the redirected ones, or the caller's.
///
/// # Safety
/// Called only by the shims, with the caller's attributes and a work area in the shim's frame.
#[unsafe(no_mangle)]
unsafe extern "C" fn gobbo_stage0_worker(attributes: usize, work: *mut Work) -> usize {
    // SAFETY: the launcher writes the configuration before the loader runs and does not write it afterwards, so this
    // reads a stable value. The query trampoline is the loader's own stub, so the existence check is not redirected.
    let config = unsafe { &*(&raw const GOBBO_STAGE0_CONFIG) };
    // SAFETY: the trampoline of the query stub has the query's signature (the stub is the loader's own code).
    let query =
        unsafe { core::mem::transmute::<usize, QueryAttributes>(config.trampolines[2] as usize) };
    // SAFETY: the shim passes its own work area and the caller's attributes, which the loader guarantees valid.
    match unsafe { logic::redirect(config, attributes, &mut *work, query) } {
        Some(redirected) => redirected,
        None => attributes,
    }
}

#[cfg(target_arch = "x86_64")]
mod x64 {
    use super::*;

    /// `NtOpenFile` and `NtCreateFile`: the attributes are the third argument. The stack arguments are copied, so the
    /// trampoline receives them as the caller passed them.
    macro_rules! open_shim {
        ($name:ident, $index:literal, $kind:literal) => {
            #[unsafe(no_mangle)]
            #[unsafe(naked)]
            pub unsafe extern "system" fn $name() {
                naked_asm!(
                    "push rbx",
                    "mov rbx, rsp",
                    "sub rsp, {frame}",
                    "mov [rsp + {saved}], rcx",
                    "mov [rsp + {saved} + 8], rdx",
                    "mov [rsp + {saved} + 16], r8",
                    "mov [rsp + {saved} + 24], r9",
                    "mov rax, [rbx + 0x30]",
                    "mov [rsp + {stack}], rax",
                    "mov rax, [rbx + 0x38]",
                    "mov [rsp + {stack} + 8], rax",
                    "mov rax, [rbx + 0x40]",
                    "mov [rsp + {stack} + 16], rax",
                    "mov rax, [rbx + 0x48]",
                    "mov [rsp + {stack} + 24], rax",
                    "mov rax, [rbx + 0x50]",
                    "mov [rsp + {stack} + 32], rax",
                    "mov rax, [rbx + 0x58]",
                    "mov [rsp + {stack} + 40], rax",
                    "mov rax, [rbx + 0x60]",
                    "mov [rsp + {stack} + 48], rax",
                    "mov rax, [rbx + 0x68]",
                    "mov [rsp + {stack} + 56], rax",
                    "mov dword ptr [rsp + {work}], {kind}",
                    "mov eax, dword ptr [rsp + {saved} + 8]",
                    "mov dword ptr [rsp + {work} + 4], eax",
                    "mov eax, dword ptr [rbx + 0x48]",
                    "mov dword ptr [rsp + {work} + 8], eax",
                    "mov rcx, [rsp + {saved} + 16]",
                    "lea rdx, [rsp + {work}]",
                    "call {worker}",
                    "mov r8, rax",
                    "mov rcx, [rsp + {saved}]",
                    "mov rdx, [rsp + {saved} + 8]",
                    "mov r9, [rsp + {saved} + 24]",
                    "mov rax, [rsp + {stack}]",
                    "mov [rsp + 0x20], rax",
                    "mov rax, [rsp + {stack} + 8]",
                    "mov [rsp + 0x28], rax",
                    "mov rax, [rsp + {stack} + 16]",
                    "mov [rsp + 0x30], rax",
                    "mov rax, [rsp + {stack} + 24]",
                    "mov [rsp + 0x38], rax",
                    "mov rax, [rsp + {stack} + 32]",
                    "mov [rsp + 0x40], rax",
                    "mov rax, [rsp + {stack} + 40]",
                    "mov [rsp + 0x48], rax",
                    "mov rax, [rsp + {stack} + 48]",
                    "mov [rsp + 0x50], rax",
                    "mov rax, [rsp + {stack} + 56]",
                    "mov [rsp + 0x58], rax",
                    "call qword ptr [rip + {config} + {tramp}]",
                    "mov rsp, rbx",
                    "pop rbx",
                    "ret",
                    frame = const FRAME,
                    saved = const SAVED,
                    stack = const STACK,
                    work = const WORK,
                    kind = const $kind,
                    worker = sym gobbo_stage0_worker,
                    config = sym GOBBO_STAGE0_CONFIG,
                    tramp = const TRAMPOLINES + 8 * $index,
                );
            }
        };
    }

    /// `NtQueryAttributesFile` and `NtQueryFullAttributesFile`: the attributes are the first argument, there are no stack
    /// arguments, and the call only reads.
    macro_rules! query_shim {
        ($name:ident, $index:literal) => {
            #[unsafe(no_mangle)]
            #[unsafe(naked)]
            pub unsafe extern "system" fn $name() {
                naked_asm!(
                    "push rbx",
                    "mov rbx, rsp",
                    "sub rsp, {frame}",
                    "mov [rsp + {saved}], rcx",
                    "mov [rsp + {saved} + 8], rdx",
                    "mov dword ptr [rsp + {work}], {query}",
                    "mov dword ptr [rsp + {work} + 4], {access}",
                    "mov dword ptr [rsp + {work} + 8], 0",
                    "mov rcx, [rsp + {saved}]",
                    "lea rdx, [rsp + {work}]",
                    "call {worker}",
                    "mov rcx, rax",
                    "mov rdx, [rsp + {saved} + 8]",
                    "call qword ptr [rip + {config} + {tramp}]",
                    "mov rsp, rbx",
                    "pop rbx",
                    "ret",
                    frame = const FRAME,
                    saved = const SAVED,
                    work = const WORK,
                    query = const Kind::Query.code(),
                    access = const logic::QUERY_ACCESS,
                    worker = sym gobbo_stage0_worker,
                    config = sym GOBBO_STAGE0_CONFIG,
                    tramp = const TRAMPOLINES + 8 * $index,
                );
            }
        };
    }

    open_shim!(GobboStage0NtOpenFile, 0, 0);
    open_shim!(GobboStage0NtCreateFile, 1, 1);
    query_shim!(GobboStage0NtQueryAttributesFile, 2);
    query_shim!(GobboStage0NtQueryFullAttributesFile, 3);
}

#[cfg(target_arch = "x86")]
mod x86 {
    use super::*;

    /// The 32-bit calling convention is stdcall: the stub's arguments are on the stack, and the stub pops them with `ret N`.
    /// A shim keeps the caller's arguments (at most eleven dwords) in its frame, has the worker pick the attributes, then
    /// calls the trampoline with the arguments in place at the bottom of the frame. The trampoline cleans them up and
    /// returns the status; the shim returns with the same `ret N` as the stub.
    macro_rules! open_shim32 {
        ($name:ident, $index:literal, $kind:literal, $args:literal) => {
            #[unsafe(no_mangle)]
            #[unsafe(naked)]
            pub unsafe extern "system" fn $name() {
                naked_asm!(
                    "push ebp",
                    "mov ebp, esp",
                    "sub esp, {frame}",
                    "mov eax, [ebp + 8]",
                    "mov [esp + {saved}], eax",
                    "mov eax, [ebp + 12]",
                    "mov [esp + {saved} + 4], eax",
                    "mov eax, [ebp + 16]",
                    "mov [esp + {saved} + 8], eax",
                    "mov eax, [ebp + 20]",
                    "mov [esp + {saved} + 12], eax",
                    "mov eax, [ebp + 24]",
                    "mov [esp + {saved} + 16], eax",
                    "mov eax, [ebp + 28]",
                    "mov [esp + {saved} + 20], eax",
                    "mov eax, [ebp + 32]",
                    "mov [esp + {saved} + 24], eax",
                    "mov eax, [ebp + 36]",
                    "mov [esp + {saved} + 28], eax",
                    "mov eax, [ebp + 40]",
                    "mov [esp + {saved} + 32], eax",
                    "mov eax, [ebp + 44]",
                    "mov [esp + {saved} + 36], eax",
                    "mov eax, [ebp + 48]",
                    "mov [esp + {saved} + 40], eax",
                    "mov dword ptr [esp + {work}], {kind}",
                    "mov eax, [esp + {saved} + 4]",
                    "mov [esp + {work} + 4], eax",
                    "mov eax, [esp + {saved} + 28]",
                    "mov [esp + {work} + 8], eax",
                    "mov eax, [esp + {saved} + 8]",
                    "mov [esp], eax",
                    "lea eax, [esp + {work}]",
                    "mov [esp + 4], eax",
                    "call {worker}",
                    "mov edx, [esp + {saved}]",
                    "mov [esp], edx",
                    "mov edx, [esp + {saved} + 4]",
                    "mov [esp + 4], edx",
                    "mov edx, [esp + {saved} + 8]",
                    "mov [esp + 8], edx",
                    "mov edx, [esp + {saved} + 12]",
                    "mov [esp + 12], edx",
                    "mov edx, [esp + {saved} + 16]",
                    "mov [esp + 16], edx",
                    "mov edx, [esp + {saved} + 20]",
                    "mov [esp + 20], edx",
                    "mov edx, [esp + {saved} + 24]",
                    "mov [esp + 24], edx",
                    "mov edx, [esp + {saved} + 28]",
                    "mov [esp + 28], edx",
                    "mov edx, [esp + {saved} + 32]",
                    "mov [esp + 32], edx",
                    "mov edx, [esp + {saved} + 36]",
                    "mov [esp + 36], edx",
                    "mov edx, [esp + {saved} + 40]",
                    "mov [esp + 40], edx",
                    "mov [esp + 8], eax",
                    "call dword ptr [{config} + {tramp}]",
                    "mov esp, ebp",
                    "pop ebp",
                    "ret {args}",
                    frame = const FRAME,
                    saved = const SAVED,
                    work = const WORK,
                    kind = const $kind,
                    worker = sym gobbo_stage0_worker,
                    config = sym GOBBO_STAGE0_CONFIG,
                    tramp = const TRAMPOLINES + 8 * $index,
                    args = const $args,
                );
            }
        };
    }

    macro_rules! query_shim32 {
        ($name:ident, $index:literal) => {
            #[unsafe(no_mangle)]
            #[unsafe(naked)]
            pub unsafe extern "system" fn $name() {
                naked_asm!(
                    "push ebp",
                    "mov ebp, esp",
                    "sub esp, {frame}",
                    "mov eax, [ebp + 8]",
                    "mov [esp + {saved}], eax",
                    "mov eax, [ebp + 12]",
                    "mov [esp + {saved} + 4], eax",
                    "mov dword ptr [esp + {work}], {query}",
                    "mov dword ptr [esp + {work} + 4], {access}",
                    "mov dword ptr [esp + {work} + 8], 0",
                    "mov eax, [esp + {saved}]",
                    "mov [esp], eax",
                    "lea eax, [esp + {work}]",
                    "mov [esp + 4], eax",
                    "call {worker}",
                    "mov edx, [esp + {saved}]",
                    "mov [esp], edx",
                    "mov edx, [esp + {saved} + 4]",
                    "mov [esp + 4], edx",
                    "mov [esp], eax",
                    "call dword ptr [{config} + {tramp}]",
                    "mov esp, ebp",
                    "pop ebp",
                    "ret 8",
                    frame = const FRAME,
                    saved = const SAVED,
                    work = const WORK,
                    query = const Kind::Query.code(),
                    access = const logic::QUERY_ACCESS,
                    worker = sym gobbo_stage0_worker,
                    config = sym GOBBO_STAGE0_CONFIG,
                    tramp = const TRAMPOLINES + 8 * $index,
                );
            }
        };
    }

    open_shim32!(GobboStage0NtOpenFile, 0, 0, 24);
    open_shim32!(GobboStage0NtCreateFile, 1, 1, 44);
    query_shim32!(GobboStage0NtQueryAttributesFile, 2);
    query_shim32!(GobboStage0NtQueryFullAttributesFile, 3);
}

// The C runtime is not linked, so the memory functions the compiler may call are here. The byte loops use volatile
// accesses, so the compiler cannot turn them back into a call of `memcpy`.

/// # Safety
/// The ranges must be valid and must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(
    dst: *mut c_void,
    src: *const c_void,
    length: usize,
) -> *mut c_void {
    let (dst, src) = (dst.cast::<u8>(), src.cast::<u8>());
    let mut index = 0;

    while index < length {
        // SAFETY: the caller guarantees both ranges hold `length` bytes.
        unsafe {
            core::ptr::write_volatile(dst.add(index), core::ptr::read_volatile(src.add(index)))
        };
        index += 1;
    }

    dst.cast()
}

/// # Safety
/// The range must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut c_void, value: i32, length: usize) -> *mut c_void {
    let dst = dst.cast::<u8>();
    let mut index = 0;

    while index < length {
        // SAFETY: the caller guarantees the range holds `length` bytes.
        unsafe { core::ptr::write_volatile(dst.add(index), value as u8) };
        index += 1;
    }

    dst.cast()
}

/// # Safety
/// The ranges must be valid; they may overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(
    dst: *mut c_void,
    src: *const c_void,
    length: usize,
) -> *mut c_void {
    let (dst, src) = (dst.cast::<u8>(), src.cast::<u8>());

    if (dst as usize) <= (src as usize) || (dst as usize) >= (src as usize) + length {
        // SAFETY: a forward copy is safe when the destination starts before the source or past its end.
        return unsafe { memcpy(dst.cast(), src.cast(), length) }.cast();
    }

    let mut index = length;

    while index > 0 {
        index -= 1;
        // SAFETY: the caller guarantees both ranges hold `length` bytes.
        unsafe {
            core::ptr::write_volatile(dst.add(index), core::ptr::read_volatile(src.add(index)))
        };
    }

    dst.cast()
}

/// # Safety
/// The ranges must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(left: *const c_void, right: *const c_void, length: usize) -> i32 {
    let (left, right) = (left.cast::<u8>(), right.cast::<u8>());
    let mut index = 0;

    while index < length {
        // SAFETY: the caller guarantees both ranges hold `length` bytes.
        let (a, b) = unsafe {
            (
                core::ptr::read_volatile(left.add(index)),
                core::ptr::read_volatile(right.add(index)),
            )
        };

        if a != b {
            return i32::from(a) - i32::from(b);
        }

        index += 1;
    }

    0
}

// Two symbols the precompiled `core` and `compiler_builtins` refer to. Neither is reached: the DLL never unwinds (a panic
// ends the process), and it does no floating-point work. The values are the ones the C runtime uses for the same markers.

/// Marks a module that uses floating point; the loader does not read it.
#[unsafe(no_mangle)]
pub static _fltused: i32 = 0x9875;

/// The C++ exception handler of the unwind tables of `core`; never called, because a panic aborts.
#[unsafe(no_mangle)]
pub extern "C" fn __CxxFrameHandler3() -> i32 {
    0
}

// The 32-bit C runtime's 64-bit division helper, which the precompiled `core` refers to (bounds-check panics format
// through it). It divides with 32-bit halves only, so it does not call itself: a 64-bit division here would. The
// arguments are on the stack and the callee removes them, as the helper of the C runtime does.

/// # Safety
/// Only the compiler and the precompiled `core` call this, with a non-zero divisor.
#[cfg(target_arch = "x86")]
#[unsafe(no_mangle)]
extern "C" fn gobbo_aulldiv(dividend: u64, divisor: u64) -> u64 {
    let (mut n_hi, mut n_lo) = ((dividend >> 32) as u32, dividend as u32);
    let (d_hi, d_lo) = ((divisor >> 32) as u32, divisor as u32);
    let (mut q_hi, mut q_lo, mut r_hi, mut r_lo) = (0u32, 0u32, 0u32, 0u32);

    for _ in 0..64 {
        let top = n_hi >> 31;
        n_hi = (n_hi << 1) | (n_lo >> 31);
        n_lo <<= 1;
        r_hi = (r_hi << 1) | (r_lo >> 31);
        r_lo = (r_lo << 1) | top;
        q_hi = (q_hi << 1) | (q_lo >> 31);
        q_lo <<= 1;

        if (r_hi, r_lo) >= (d_hi, d_lo) {
            let (lo, borrow) = r_lo.overflowing_sub(d_lo);
            r_hi = r_hi.wrapping_sub(d_hi).wrapping_sub(u32::from(borrow));
            r_lo = lo;
            q_lo |= 1;
        }
    }

    (u64::from(q_hi) << 32) | u64::from(q_lo)
}

/// The name the compiler calls: undecorated, with the arguments removed by the callee (stdcall), as the C runtime's
/// helper is. It forwards the four stack words to `gobbo_aulldiv`, which takes them with the C convention.
///
/// # Safety
/// Called only by the compiler, with two 64-bit arguments on the stack.
#[cfg(target_arch = "x86")]
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn __aulldiv() {
    naked_asm!(
        "push dword ptr [esp + 16]",
        "push dword ptr [esp + 16]",
        "push dword ptr [esp + 16]",
        "push dword ptr [esp + 16]",
        "call {divide}",
        "add esp, 16",
        "ret 16",
        divide = sym gobbo_aulldiv,
    );
}
