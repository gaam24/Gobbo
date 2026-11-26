# overlay-hook architecture

Read this before changing any file in this crate. The hook is a DLL loaded into a game process (and into the game's
children). A mistake here can crash the game, hang it under the loader lock, silently write to the game folder, or make
a modded game behave differently from the unmodded one.

This file is the durable contract. A local, git-ignored plan may sit in `plans/` as a roadmap; it is a roadmap, not a
rulebook. When this file and the code disagree, fix one of them in the same change.

This is the top of the hook's own pyramid: read sections 1–4 and the change checklist. The full path model, hook table,
diagnostics contract and platform notes live in `docs/` and are linked from sections 5–8 — open only the one you touch.

## 1. What this crate is

`gobbo_overlay_hook.dll` is loaded into the game before its first instruction. It hooks the file API of
`kernel32`/`KernelBase` and the NT layer of `ntdll`, and redirects the game's file calls through one or more mounts:

- the **game mount**: `root` is the game folder, `staging` the mod layer, `upper` the game's writes;
- **user-data mounts**: `guest` is a folder the game keeps outside its own folder (`Documents/The Witcher 3`),
  `root` is its read-only base (the real folder, or an empty folder), and `upper` is the active profile's layer.

Each mount has the same three roles: a read sees the top-most visible layer, a write goes to `upper` (copy-up first when
the old contents must be kept), a deletion or a rename away from a lower layer leaves a whiteout, and a directory listing
shows the merged layers. The game mount has a `staging` layer; a user-data mount has none. Outside every guest path a
call passes through unchanged.

The launcher also rewrites the executable's **static import descriptors** before it resumes the program
(`overlay-launch/src/static_imports.rs`). The Windows loader resolves those imports before the hook's detours are
installed, so a mod DLL next to the program (for example `version.dll`) cannot be reached by the NT hook alone. The
descriptor's `Name` field is pointed at the full path in the mod layer; the loader reads the DLL from there, and the hook
maps the loaded module back to the game path. The game folder is never written.

The crate is Windows-only: `lib.rs` starts with `#![cfg(windows)]`. The Linux overlay is a different implementation in
`src-tauri/src/deploy/overlay/linux.rs`.

## 2. Hard invariants

These are not suggestions. A change that breaks one of them is a bug, even if the tests pass.

1. **The game folder and every user-data base folder are never written by a hooked call.** If a path cannot be
   classified with certainty, writes, deletes and renames fail closed. Reads may pass through, but they must not be used
   to justify a write. A user-data redirect writes only its profile's `upper` layer.
2. **A hooked function preserves the caller's observable state.** For Win32 calls this means `GetLastError`; for NT
   calls the returned `NTSTATUS`. Logging, counting, whiteout writes and handle bookkeeping must not leak their own
   errors to the game.
3. **No panic or unwind across the FFI boundary.** Hooks are `extern "system"`. Use `Option`/`Result` and explicit
   fallbacks; no `unwrap`, `expect`, `panic!`, indexing that can panic, or arithmetic that can overflow in production
   code. Tests may panic.
4. **The pass-through path is free of allocation, locking, file I/O and formatting.** A path outside every guest path
   must be classified and returned to the original function without touching the heap, a mutex, the trace writer or the
   count report. The session is capped at eight mounts so the prefix loop stays cheap.
5. **The re-entrancy guard is held while the hook performs its own file I/O.** A file call made by the hook itself must
   pass through untouched. The guard is per-thread; never share it or replace it with a global lock.
6. **No heavy work in `DllMain`.** `DllMain` runs under the loader lock. It stores the module handle, disables thread
   notifications and returns. Settings, layers, whiteouts, diagnostics, crash reporting, module ranges and hooks are
   initialized by the exported `GobboOverlayInit`, which the launcher's stub calls after `LoadLibraryExW` returns and
   before the loader resolves the program's static imports. It runs outside the loader lock; it must not create threads
   or call loader APIs that need the process to be initialized. The one exception is the thread of the crash filter and
   the trace flusher: the flusher starts only when a log is named (the dev default, or an explicit `GOBBO_OVERLAY_LOG`
   in a release build) and no thread starts when the log is unset or `off`. Both start after the detours are enabled
   (`diag::start_background_threads`), so the enable sees one thread and needs no suspension (section 3).
7. **Hook installation is observable and all-or-nothing for the essential set.** Each hook has a status
   (`created`/`enabled`/`disabled`/`failed(reason)`). If an essential hook cannot be installed, the launcher must not
   silently start an unhooked game.
8. **Every mutation that reaches the layers and every merged view emits a decision.** A write, delete, rename, copy-up,
   whiteout, refusal after classification and a merged NT directory listing produce one compact record when diagnostics
   are on. An early refusal (an invalid name or a device namespace) sets the Windows error and is traced, but does not
   build a ring path. Pure reads and attribute queries are counted and traced only with `GOBBO_OVERLAY_CALLS`; they must
   not pay for a ring write on the pass-through path.
9. **Process-wide state is limited to the session and the tables the APIs require.** Layers, the whiteout snapshot,
   cached paths, the hook table and statuses, the search-handle and NT listing cursor tables (bounded and keyed by
   handle), per-target copy-up locks, child hook paths, and the diagnostic ring/counts/trace/module cache live in
   `OnceLock`, atomics or short-lived locks. Do not add a static for game state; pass it through the session. A new
   bounded table must say why it is needed and how it is evicted.
10. **No undocumented Windows behavior without a fallback.** `CreateProcessInternalW` is not a documented export. If it
    is missing, hook the documented `CreateProcessW`/`CreateProcessAsUserW` or report the hook as unavailable.
11. **Static imports come from the layer without a file in the game folder.** The launcher rewrites the import
    descriptor of an executable (and a child the hook starts) to the full layer path before the thread resumes. A
    failure to rewrite an import that has a layer copy stops the launch; it never falls back to placing the DLL in the
    game folder. The patch covers only the program's own import table.
    - **Stage 0.** The loader resolves the whole import tree at its first loader call, which is the stub's
      `LoadLibraryExW`, before the hook DLL is mapped. So the hook cannot be the early step. Instead the launcher maps
      the stage 0 DLL (`crates/overlay-launch/stage0/`, no imports, no TLS) into the suspended child and patches the
      start of four ntdll loader stubs (`NtOpenFile`, `NtCreateFile`, `NtQueryAttributesFile`,
      `NtQueryFullAttributesFile`) with a jump into it. A shim serves a read of a file under the game folder from the
      mod layer when the layer holds it, and otherwise calls a trampoline (a copy of the original stub). So the import
      tree binds the layer copy, also for a system name such as `version.dll` (test
      `m3_the_layer_copy_of_a_system_name_is_the_one_bound`). The launcher writes a handover file (`early-redirect.txt`,
      named by `GOBBO_OVERLAY_EARLY`) with the original bytes; this hook restores them before it creates its detours, and
      then switches the redirect off. Folders, writes and creates pass through.
    - **32-bit children** take the x86 build: the stubs of the WOW64 ntdll are `mov eax, N; mov edx, target; call edx; ret M`,
      patched with `jmp dword ptr [slot]`, where the slot is the shim address in the configuration. The 32-bit ntdll is
      found by scanning the child's image regions. The redirect is tested with a 32-bit `cmd.exe` and a 32-bit game DLL
      (`m2_x86_*`, `x5_*`); a real 32-bit game is not tested yet.
    - Historical note: the hook cannot be mapped by hand, because a hook with TLS, `std` thread-local storage and
      imports of `bcryptprimitives`, `ws2_32` and the VC runtime would need a loader for each thread. That is why the
      stage 0 is a separate, minimal DLL.

## 3. Lifecycle

```text
CreateProcessW (launcher, CREATE_SUSPENDED)
  -> map the stage 0 DLL, write its configuration, patch the four loader stubs to its shims (64-bit programs)
  -> write the handover file (`early-redirect.txt`)
  -> inject stub (SetThreadContext on the main thread)
  -> rewrite the static import descriptors whose DLL is in a layer
  -> ResumeThread
  -> stub (before LdrInitializeThunk)
       - LoadLibraryExW(hook, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32)
       - GetProcAddress(hook, "GobboOverlayInit")
       - GobboOverlayInit (outside the loader lock)
            - remember this DLL's path for the children
            - read switches and layers
            - install crash reporting
            - cache module ranges
            - load whiteouts and store the session layers (path caches build on first use)
            - hand the loader stubs back from stage 0 (`early.rs`) before the detours are created
            - create all detours
            - enable the detours in one transaction; with one thread in the process nothing is suspended (`detours::enable`)
            - start the crash filter thread and, when a log is named, the trace flusher thread (`diag::start_background_threads`)
            - record whether the internal process hook is active
            - write session.json
            - open the decision ring (so the report's own writes are not game decisions)
            - signal Local\GobboOverlayReady_<pid>
       - restore registers and jump to the original RIP
  -> the loader resolves static imports through the patched descriptors, then the game code runs
  -> launcher waits for the event or the process exit
  -> hooked call
       - decision/trace (only if enabled)
       - guard
       - classify path
       - resolve (one probe)
       - copy-up / whiteout / pass-through
       - call original
       - preserve LastError/status
       - final-path and module-path mapping comes from the real layer path; there is no handle table
  -> process exit
       - no flush: every ring record is a complete append
       - no final status: session.json was written once at install
       - nothing on DLL_PROCESS_DETACH: no locks, no I/O; dynamic unload of the hook is not a supported operation
```

`CreateProcessInternalW` repeats the load for each child: start suspended, rewrite the child's static imports whose DLL
is in a layer, set the matching hook stub (the native x64/x86 path, or the 32-bit helper for a WOW64 child), resume
unless the caller asked for a suspended child, and wait for readiness. If the matching DLL cannot be loaded, the child is
terminated and the start fails with `ERROR_DLL_INIT_FAILED`; `GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN` is the explicit
opt-out. When `CreateProcessInternalW` is missing or disabled, the documented `CreateProcessW` and `CreateProcessAsUserW`
hooks do the same work.

## 4. Module map and where code goes

| Path             | Responsibility                                                                                                                                                                                                           | May use                                                                                | Must not                                                                                                                                                                            |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lib.rs`         | `DllMain`, exported `GobboOverlayInit`, readiness                                                                                                                                                                        | everything                                                                             | path decisions, business logic in `DllMain`                                                                                                                                         |
| `state.rs`       | Session layers, whiteout snapshot, hook table access                                                                                                                                                                     | `overlay-core`, `detours`, `diag`                                                      | path decisions                                                                                                                                                                      |
| `guard.rs`       | Per-thread re-entrancy flag                                                                                                                                                                                              | `windows-sys`                                                                          | path decisions                                                                                                                                                                      |
| `detours/`       | `detours!` table, export lookup, install/enable transaction                                                                                                                                                              | `detour`, `overlay-core`, `windows-sys`, `diag`, `text`, `state`                        | path decisions                                                                                                                                                                      |
| `win32/`         | One file per Win32 API family: open, attributes, delete, move, replace, links, module path; argument validation and error mapping                                                                                        | `overlay-core`, `paths`, `state`, `handles`, `diag`, `trace`, `guard`                   | parsing NT structures                                                                                                                                                               |
| `nt/`            | NT create/open/query/set-info/directory; NT names; the directory listing cursor, which turns a `WIN32_FIND_DATAW` into an `overlay-core` `nt_listing::Entry`                                                             | `paths`, `state`, `handles`, `diag`, `trace`, `guard`, `overlay-core`                  | Win32 error codes; the entry bytes (`nt_listing` writes them)                                                                                                                       |
| `paths/`         | `MountPaths`, absolute/relative/UTF-16 classification across every mount, exact-name lookups for the listing spelling (`spelling`), `Probe`, `Target`/`Failure`, visible-layer resolution, copy-up and plain disk checks | `overlay-core`, `state`, `diag`, `trace`, `windows-sys` helpers                        | hooked calls                                                                                                                                                                        |
| `search/`        | Merged `FindFirstFile*` listings, the find-handle table, and the entry source of the NT directory hook                                                                                                                   | `paths`, `overlay-core`, `state`, `diag`, `trace`, `win32`                             | Win32 open logic                                                                                                                                                                    |
| `handles/`       | Final-path, mapped-file and module-path mapping from a layer path to the game path                                                                                                                                       | `state`, `guard`, `text`, `win32`, `windows-sys`                                       | path-resolution policy                                                                                                                                                              |
| `process.rs`     | Child start, architecture choice, injection, hook paths, fail-closed policy                                                                                                                                              | `overlay-launch`, `overlay-decisions`, `diag`, `trace`, `win32`, `text`, `guard`         | path decisions                                                                                                                                                                      |
| `early.rs`       | Hands the stage 0 stubs back to the hook (restores their bytes, switches the redirect off) before the detours go in                                                                                                      | `trace`, `windows-sys`                                                                 | path decisions                                                                                                                                                                      |
| `diag/`          | Switches, counts, trace, decision ring, crash handling, session report, module cache, old-file pruning                                                                                                                   | `windows-sys`, `overlay-core`, `overlay-decisions`, `detours`                           | path decisions                                                                                                                                                                      |
| `text.rs`        | UTF-16/UTF-8 conversions and `\\?\` helpers; the NUL-terminated UTF-16 helper lives in `overlay-core`                                                                                                                   | `overlay-core`, `windows-sys`                                                          | policy decisions                                                                                                                                                                    |
| `types.rs`       | Signatures of the hooked exports; the NT structures the hook builds itself (`UnicodeString`, `ObjectAttributes`)                                                                                                         | `windows-sys` (`Wdk_Foundation` and `Win32_Foundation` types are the layout reference) | the hook keeps its own `#[repr(C)]` copies; `const` asserts in `types.rs` and `nt/layout.rs` fail the build if a size or offset differs from the system's structure, on x64 and x86 |
| `macros.rs`      | `original!` fallback lookup                                                                                                                                                                                              | `state`                                                                                | policy                                                                                                                                                                              |
| `bench_alloc.rs` | Allocation counters behind the `bench-alloc` feature                                                                                                                                                                     | `std`                                                                                  | shipped in a release hook                                                                                                                                                           |

Rules:

- A hook lives in exactly one file. If a file grows past ~300-400 lines of production logic, split it into a folder.
  `nt/directory.rs` is the largest hook today. Its byte formatter lives in `overlay-core::nt_listing` (plain logic, tested
  on every platform), so the hook only keeps the cursor and the conversion from `WIN32_FIND_DATAW`.
- Pure decisions (what to do, not how to touch the disk) are separated inside `paths/` and `search/` and are tested
  without a session: `copy_needed`, `name_taken`, `writes_requested`, `parent_rel`, the NT directory layouts.
- No glob imports. No `pub use submodule::*`. Prefer `pub(crate)`.
- Import the module that defines a name, not a re-export.
- `unsafe` is allowed at the FFI boundary: `win32/`, `nt/`, `detours/`, `handles/`, `process.rs`, the hook entry points
  in `search/`, and the pointer helpers in `paths/names.rs`/`paths/probe.rs`. Keep `paths/session.rs`, `paths/disk.rs`,
  `paths/target.rs` and `search/listing.rs` safe Rust where the FFI is not the point.
- A new public item needs a doc comment. A doc comment separated from its item by a `use` is a bug.

## 5. Path model

The full path model is in [docs/path-model.md](docs/path-model.md).

## 6. Hook table and ownership

The full hook table is in [docs/hook-table.md](docs/hook-table.md).

## 7. Diagnostics contract

The full diagnostics contract is in [docs/diagnostics.md](docs/diagnostics.md).

## 8. Platforms and architectures

The platform rules are in [docs/platforms.md](docs/platforms.md).

## 9. Performance rules

- No optimization without a measurement. Use `overlay-bench` (no hook / pass-through / active) and `hdrhistogram`.
- Pass-through budget: 0 heap allocations, 0 locks, 0 file I/O, 0 formatting. The local benchmark notes record 0 hook
  allocations for every pass-through operation and roughly +6% to +35% p50 against the plain process,
  depending on the run. A session is capped at eight mounts; the game mount is checked first, so the common pass-through
  pays one prefix comparison, and each redirect adds one more only when the name reaches it.
- Static import patch: one parse of the program on disk, one PEB read, and per mapped import one allocation near the
  image and one four-byte descriptor write. It is startup work, never a hooked call.
- A read budget: one path classification, one probe per layer until found, one open.
- A write budget: one probe for the source, one probe for the parent, one copy-up only when needed.
- A listing budget: one system listing per layer with the caller's mask; no per-entry metadata open and no manual
  wildcard. The merged name string and lowercase key remain per entry; names outside ASCII fold into a stack buffer,
  with the allocating path only when the folded name does not fit. The Win32 listing reads the game folder (the root
  layer) one entry at a time, with its own find handle, and keeps only the upper and staging entries (`search/listing.rs`,
  `RootStream`); above 8 taken entries the lower spelling comes from one extra root pass, and below it from an exact
  name search. The root pass writes the spelling into the taken entries in place instead of staging a second copy of
  every `WIN32_FIND_DATAW`, and a dot name is rejected in UTF-16 before any `String` is built. The spelling pass reads
  that root in `GetFileInformationByHandleEx` batches when the path is ASCII and the level and flags allow it: one 8 KiB
  buffer for the pass, dropped when it ends, with the caller's mask applied in user mode; otherwise it keeps
  `FindFirstFileExW`/`FindNextFileW`. The program-visible root stream still reads one entry at a time. When no upper or
  staging entry can change the folder's own listing and no whiteout hides the folder or an immediate child
  (`Whiteouts::affects_listing`), `start_search` returns the real `FindFirstFileExW` handle instead; the program's
  `FindNextFileW` and `FindClose` calls then pass through, so an unmodded folder lists as fast as without Gobbo. The NT
  directory query uses the same `ListingLayers`/`RootStream`, pulls at least 64 and at most 128 entries per
  response (more when the caller's buffer is large), and no longer materializes the whole folder. A folder with no
  upper/staging entry matching the query and no whiteout affecting its own listing is remembered as a plain handle: every
  query on it passes to the original `NtQueryDirectoryFile`, and a restart or a changed final path checks the layers
  again. A repeated NT query compares the handle's final path in UTF-16 with the listing's cached path and continues the
  snapshot under one lock;
  only a new or reused handle resolves the layers again, so the folder probes happen once per listing. A listing is
  `Arc<Mutex<Search>>` per handle, so the disk read of a listing never runs under the table lock.
- A handle path budget: `GetFinalPathNameByHandleW` and `GetModuleFileNameW`/`ExW` read the DOS path once into a stack
  buffer, classify and map it in UTF-16, and write the answer without a `String` or a `Vec`. A path outside every mount
  is written back as it is: one system call, no allocation. The `A` forms convert the same UTF-16 answer with a stack
  buffer and fall back to a heap buffer only for a path longer than 2048 bytes. `layer_path_kind` takes the no-allocation
  path for an outside handle, so a pass-through NT directory query allocates nothing.
- The launcher's steps are recorded in `last_run.json` (`launch_steps`): setup, the dumper, the DLL list and program
  read, `CreateProcessW`, stage 0, injection, static imports, resume and the ready wait. The 32-bit dumper helper is
  spawned before the program read and waited for just before the resume, so its startup overlaps the PE work instead
  of adding to the critical path.
- No `format!`, `to_string_lossy`, `Path::display().to_string()` or `Vec<char>` in a hot path.
- The enable transaction assumes the single thread of the early install; counting the process's threads currently walks
  every thread in the system and cost about 30 ms. `GOBBO_OVERLAY_SAFE_THREADS=1` restores the snapshot and the
  suspension for a manual load or an unusual launcher.
- No global/session mutex held across a filesystem call. Whiteouts use an `Arc` snapshot; the journal lock is taken only
  for the append. The one deliberate exception is the per-target copy-up lock: it must cover the copy and the publish,
  because it serializes two writers of the same file; the map lock is released before it is taken.
- No stack walk unless `call`/`full` tracing is explicitly on. Cache module ranges at install.
- Dev builds of `overlay-hook`, `overlay-core` and `overlay-launch` use `opt-level = 2/3`; release hook uses
  `opt-level = 3` and ships PDBs. The release profile sets `debug = "line-tables-only"` and `strip = false` for
  `gobbo-overlay-hook` only, so every other crate stays small.
- Signing is external: the release pipeline uses a certificate that is not in the repository; the repository only
  builds the DLLs and PDBs. Do not add a private key or a signing secret to the tree.
- Add `#[inline]` only to tiny classification helpers; add `#[cold]` to refusal/error/report paths.

## 10. Testing rules

A change to this crate needs the tests that match its risk:

- Pure decision unit tests for new `Probe`/target/whiteout/listing and NT formatter logic.
- A regression test for every bug fixed (especially `LastError`, null/empty paths, missing parent, copy-up errors).
- Integration tests in `src-tauri/src/deploy/overlay/windows/tests/` for end-to-end behavior. A static import of a
  mod DLL is built with `lib.exe` and tested x64 and x86 (`tests/dlls.rs`); the same test covers a mod DLL overriding a
  game DLL and checks that the game's file did not change. The hook's own import table is checked against the common
  proxy DLL names (`tests/imports.rs`).
- Known gap: a game folder spelled with 8.3 names (`C:\Users\RUNNER~1\...`). The loader reports module paths in the
  spelling the process used, while handle APIs report the long form, and the session stores one form. The CI runner's
  default `TEMP` is such a path, so the Windows job points `TEMP` at a long path; a real fix needs both spellings in
  every comparison and a test that starts a game from an 8.3 path.
- Property tests for path normalization; fuzz tests for NT structure parsers are a known gap (no harness yet).
- Benchmark before/after for hot-path changes, with allocation counts.
- `overlay-bench` is the source of the numbers: `cargo build -p gobbo-overlay-hook --features "bench-alloc bench-stats"`,
  then `cargo run -p gobbo-overlay-bench -- --iterations 5000`. Store the result in the local benchmark notes.
- Crash tests: null dereference x64/x86, the no-monitor in-process fallback and a 32-bit crash. Stack overflow and a
  panic _inside the hook_ are not automated: a panic across `extern "system"` aborts, and a deliberate test hook would
  live in the shipped DLL. Keep them as a known gap until a test-only feature exists.
- The `overlay-probe` doctor: run it in a session and require `ok: true`; the app test also checks the physical game folder
  for the probe folder after the run. Keep both the x64 and x86 probe in the build.
- Architecture matrix: x64, x86, and ARM64 compile checks; real x86 test when available.

Commands from `src-tauri`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p gobbo-overlay-hook
```

On Windows, build both hooks, the 32-bit injector, the probes and the dumper (staged in `target/debug`, where the tests
look for them), then run the overlay tests. CI runs exactly this:

```bash
bun run build:overlay-hook
cargo test -p gobbo --lib deploy::overlay::windows
# Compile check for the experimental ARM64 target:
cargo check -p gobbo-overlay-hook -p gobbo-overlay-launch --target aarch64-pc-windows-msvc --all-targets
```

## 11. Change checklist

- [ ] I read this file and the relevant module docs.
- [ ] The change preserves the hard invariants in section 2.
- [ ] I added/updated the decision record and the session status for a new hook or behavior.
- [ ] If the change touches static imports, the loader patch, the hook import table or the launcher, the x64 and x86
      static-import tests and the import-table test pass.
- [ ] I added a regression test, or explained why the behavior cannot be tested.
- [ ] I ran `cargo fmt`, clippy and the tests.
- [ ] I benchmarked a hot-path change and recorded the numbers.
- [ ] I updated this file if the module map, invariants, hook table or diagnostics contract changed.
- [ ] I updated `AGENTS.md` if the repository rules changed.
