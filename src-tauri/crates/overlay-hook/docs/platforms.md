# Platforms and architectures

- Primary: Windows 10 1809+ / Windows 11, x86_64.
- Required: Windows x86 (32-bit games and 32-bit children). A 32-bit process must load the 32-bit DLL; the launcher
  chooses the DLL by the PE architecture of the program, and the child hook chooses by the child's architecture.
- Experimental: Windows ARM64 via `detour` 0.10. Compile and unit-test it; do not claim it works until it runs on real
  hardware. Keep it opt-in. The launcher has no ARM64 early-injection stub yet: `spawn_hooked` fails with a clear error
  instead of running the game unhooked. Do not run a game with the overlay on ARM64 until the stub, the import patch and
  the x64/x86 tests have an ARM64 equivalent.
- Linux: this crate is empty; the Linux overlay is `src-tauri/src/deploy/overlay/linux.rs`.

32-bit specifics:

- `FILE_RENAME_INFORMATION`/`_EX` offsets differ; `nt/layout.rs` selects them with `cfg(target_pointer_width)`.
- Fake search handles must fit `usize`; `SEARCH_BASE` uses the high bit and an odd low bit, not `1 << 40`.
- `HANDLE`, `usize` and pointer-sized fields are 4 bytes; keep every hand-written NT structure `#[repr(C)]` and test it
  on x86.
- The 64-bit launcher rewrites the static imports of a 32-bit program (PE32; the PEB comes from
  `NtQueryInformationProcess(ProcessWow64Information)`), while `gobbo_overlay_inject32.exe` sets the x86 thread context.
  The 32-bit hook patches its own 32-bit children with `ProcessBasicInformation`.
- A 64-bit process cannot call its `LoadLibraryW` address in a 32-bit process. The launcher and a 64-bit hook start
  `gobbo_overlay_inject32.exe` (built for x86) for a WOW64 process; the helper injects the 32-bit DLL and waits for the
  readiness event. A 32-bit hook can inject a 32-bit child directly.
- The hook starts the helper inside `CreateProcessInternalW`; the re-entrancy guard makes that helper's own start pass
  through unchanged (no suspended start, no injection).
- The out-of-process `minidumper` protocol carries pointer-sized crash fields, so a 32-bit game uses the 32-bit
  `gobbo_overlay_dump32.exe` helper and its own `GOBBO_OVERLAY_DUMPER32` socket.
