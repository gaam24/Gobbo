# Diagnostics contract

Switches are read once at install, never per call:

- `GOBBO_OVERLAY_LOG` - a path, or `off`/empty. Dev default: the developer folder. Writes decisions and errors, not one
  line per call.
- `GOBBO_OVERLAY_CALLS` - any non-empty value except `off`. Adds one line per hooked call and result and counts them.
  The detailed count report needs `GOBBO_OVERLAY_DIAG` too. Off by default: the stack walk and the line per call are
  too expensive for normal play.
- `GOBBO_OVERLAY_PASS` - any non-empty value except `off`. Traces writes, deletes and renames that pass through because
  their path is outside every mount. Off by default: saves and settings are expected there.
- `GOBBO_OVERLAY_DIAG` - a path, or `off`/empty. Dev default: the developer folder. Names the detailed count report. The
  report needs `GOBBO_OVERLAY_CALLS` to count the calls.
- `GOBBO_OVERLAY_CRASH` - a path, or `off`/empty. Dev default: the developer folder. A text report is written for a
  panic or for an opt-in first-chance exception; an unhandled exception writes the dump and the decision ring.
- `GOBBO_OVERLAY_CRASH_FIRST` - any non-empty value except `off`: also report first-chance exceptions with a vectored
  handler. Off by default: a program handles many exceptions itself, and the handler must stay out of that path.
- `GOBBO_OVERLAY_SESSION` - the JSON session report path. Dev default: the developer folder. Format 3 records the
  layers, the switches, `install_ms` and `install_steps` (the microseconds of each install step, in order), warnings, the ring path, the dumper socket and whether it connected with its handler attached, and the state
  of every hook after installation.
- `GOBBO_OVERLAY_DUMP` - `1`/`on` for a developer minidump, a path for a specific file, or `off`/unset. This is the
  in-process DbgHelp fallback when no out-of-process monitor is available.
- `GOBBO_OVERLAY_DUMPER` - the socket of Gobbo's x64 minidumper. A 64-bit hook sends the crash context to Gobbo, which
  writes the dump out-of-process.
- `GOBBO_OVERLAY_DUMPER32` - the socket of `gobbo_overlay_dump32.exe`. A 32-bit hook sends its 32-bit crash context to
  this helper; the protocol carries pointer-sized fields, so it cannot share the x64 server.
- `GOBBO_OVERLAY_RING` - the base path of the decision ring. Every process writes `<base>.<pid>.ring`. Unset, a dev
  build uses the developer folder; a release build writes none.
- `GOBBO_OVERLAY_DISABLE` - comma-separated hook field names.
- `GOBBO_OVERLAY_TEST_PASSTHROUGH` - any non-empty value except `off`: every call passes through, hooks still
  installed.
- `GOBBO_OVERLAY_TEST_NO_ALIAS` - any non-empty value except `off`, debug builds only: a real `root`/`staging`/`upper`
  path keeps its old pass-through meaning instead of aliasing to the guest view. `overlay-probe` uses it to check the
  physical layers; release builds always alias.
- `GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN` - a child that could not receive the hook may run anyway. Off by default: the
  child is terminated and the start fails with `ERROR_DLL_INIT_FAILED`.
- `GOBBO_OVERLAY_SAFE_THREADS` - count the process's threads with a Toolhelp snapshot before enabling the detours and
  suspend them when there is more than one. Off by default: `GobboOverlayInit` runs from the launcher's stub before the
  program's first instruction, so only the main thread exists; the switch is the fallback for a manual load or an
  unusual launcher.
- `GOBBO_OVERLAY_RUN_ID`, `GOBBO_OVERLAY_PARENT_PID`, `GOBBO_OVERLAY_DEPTH` - planned per-run identity for traces/dumps.

Every mutation hook records a `Decision` with its kind, layer, error and path; the merged NT listing records `Kind::List`.
A decision in a user-data mount prefixes its path with `@<set id>\`, so the report says which profile layer was touched
without changing the ring record. The session report (`format` 3) lists every mount with its guest, base, staging and
upper path. The ring is a fixed-size binary file with a 64-byte header and 256-byte
records (4096 per process); the writer appends the record and then the header index, so the app can read it live without
a lock. The app merges the files of all processes by time and shows the newest ones in the game console. The ring records
writes, copy-ups, whiteouts, missing write targets, refusals and merged NT directory listings by default; pass-through
writes are added only with `GOBBO_OVERLAY_PASS`, because the pass-through budget forbids the ring's lock and I/O there.
The session report is `session.json`; the crash report is a text file. The out-of-process dump is a minidump written by
Gobbo from a `minidumper` client message.
The hook never symbolizes stacks in-process. The report adds `injection` (`early`) and the launcher records how many
static import descriptors it rewrote; a rewrite failure stops the launch and is reported with the process exit code.
`dbghelp.dll` is not in the hook's import table: the in-process dump loads it lazily from `System32` only on a crash.

The ring is a file, not a memory map. An mmap would make a reader and a writer share one mapping lifetime for little
gain: the ring is off in the benchmark and the writer only appends when a mutation or a new listing happens. Revisit this
only if a ring benchmark shows the seek/write dominating a hot path; this paragraph is the deviation record.

Two diagnostics locks are deliberate deviations from section 9's rule that a process-wide mutex must not be held across a
filesystem call. Both paths are off unless a switch names them, both are crash-adjacent dev-only code, and both serialize
two writers of the same file - the rationale behind the per-target copy-up exception. These paragraphs are their
deviation records; do not restructure either lock without moving its record first.

- The decision ring (`RING` in `diag/ring.rs`) is held across `Ring::record`, which writes the record and the index
  file. The path text is built before the lock is taken, so no formatting runs under it.
- The trace file (`TRACE_FILE` in `diag/writer.rs`) is held across `rename`, `open` and `write_all` during a rotation, so
  a batch cannot interleave with the rename of the file it is writing. The rotation size comes from a byte counter next
  to `TRACE_FILE`, not from `metadata()` on every batch; the file is measured once when it is opened.

The crash path is special:

- unhandled exceptions only by default; the text report covers a panic and the opt-in first-chance path
  (`GOBBO_OVERLAY_CRASH_FIRST`), while an unhandled crash writes the dump and the ring;
- `GOBBO_OVERLAY_DUMP=1` or a path also writes an in-process DbgHelp minidump when no out-of-process monitor is connected;
- module ranges are cached at install for `locate_cached`; the opt-in vectored first-chance handler uses it, not the
  last-chance filter;
- `EXCEPTION_CONTINUE_SEARCH` always;
- `CRASHING` is set only after the report writer is ready;
- the production path is an out-of-process `minidumper`/`minidump-writer` in Gobbo; the in-process dump is a dev aid.
- Gobbo starts the listener before the game and passes its short AF_UNIX path in `GOBBO_OVERLAY_DUMPER`. The hook connects
  at install, installs the filter and puts it back on top after 250 ms, 1, 2, 10 and 60 seconds, then every minute: a
  runtime or engine that installs its own unhandled filter while it starts would otherwise replace ours. The previous
  filter is chained. The dump request is sent at most once (`DUMP_REQUESTED`).
- The out-of-process protocol is pointer-sized: x64 matches the Gobbo server, x86 matches the 32-bit helper. Keep both
  sides of one architecture together; do not connect a 32-bit hook to the x64 server.
- A 32-bit hook starts `gobbo_overlay_dump32.exe` through Gobbo before the game and sends its crash context to that
  helper. The helper writes the same `state/dumps` file and `state/last_dump` marker as the x64 server.
- Gobbo keeps the newest 10 dumps in `state/dumps` and names the newest in `state/last_dump`; the game console shows it.
