//! Switches and counts for finding what the hook does. The environment is read once, at the first use, so a hooked call
//! never reads it. Each switch is an environment variable, and in a dev build it is on by default:
//! - `GOBBO_OVERLAY_LOG`: a trace with the decisions and errors of the hook (see `trace_line`). Written in batches.
//!   Each line has its number, its time, its thread, and for a call the place in the program that made it.
//! - `GOBBO_OVERLAY_CALLS`: one line per hooked call and result, plus the detailed count report. Off by default even in
//!   a dev build: the stack walk and the line per call made games unplayable and distorted measurements.
//! - `GOBBO_OVERLAY_PASS`: a line for every write, delete or rename that passes through because its path is outside the
//!   game folder. Off by default: games write saves and settings outside the folder normally.
//! - `GOBBO_OVERLAY_RING`: a file-backed ring of the decisions (write, copy-up, whiteout, missing, refuse and
//!   pass-through), one file per process. The app reads it while the game runs. Off unless the app names a base path; a
//!   dev build defaults to `%LOCALAPPDATA%\Gobbo\overlay\overlay-decisions`.
//! - `GOBBO_OVERLAY_DIAG`: a report of the most frequent calls, rewritten every 1000 calls, when `GOBBO_OVERLAY_CALLS`
//!   is set. When one call is more than a quarter of all calls, the report says that a loop is likely.
//! - `GOBBO_OVERLAY_CRASH`: a crash report for a panic, or for an opt-in first-chance exception: the place and the last
//!   512 trace lines (see `crash`). An unhandled crash is covered by the dump and the decision ring.
//! - `GOBBO_OVERLAY_CRASH_FIRST`: also report first-chance exceptions with a vectored handler. Off by default.
//! - `GOBBO_OVERLAY_DUMP`: a minidump of a crash, via DbgHelp. `1`/`on` selects the developer file; unset/`off` writes
//!   no dump. This is the in-process fallback when the out-of-process monitor is missing.
//! - `GOBBO_OVERLAY_DUMPER`: the socket of Gobbo's x64 minidumper. A 64-bit hook sends the crash context to Gobbo.
//! - `GOBBO_OVERLAY_DUMPER32`: the socket of the 32-bit dumper helper. A 32-bit hook sends its 32-bit crash context
//!   there, because the minidumper protocol carries pointer-sized fields.
//! - `GOBBO_OVERLAY_SESSION`: the JSON report of the installed hooks, the layers and the switches; written once at start.
//! - `GOBBO_OVERLAY_DISABLE`: a comma-separated list of detours to leave off, by their field name in `Hooks` (for
//!   example `create_file`). Turning them off one at a time finds the one that makes the program misbehave.
//! - `GOBBO_OVERLAY_TEST_PASSTHROUGH`: every call passes through unchanged, with the hook still loaded.
//! - `GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN`: a child that could not receive the hook may run anyway. Off by default:
//!   a child that might touch the game folder is stopped instead.
//!
//! A switch set to a path writes there. Set to `off` (or empty), it writes nothing, also in a dev build. Unset, a dev
//! build writes next to the other Gobbo files, in `%LOCALAPPDATA%\Gobbo\overlay`: `<game>.<pid>.trace.log`,
//! `<game>.<pid>.counts.txt` and `<game>.<pid>.crash.txt`, where `<game>` is the name of the program that loaded the hook
//! and `<pid>` its process id. A trace file that reaches 64 MB is moved to `<game>.<pid>.old.log`. A new process removes
//! the files older than three days, and all but the 200 newest (see `prune`).
//!
//! - `settings`: the switches, read once.
//! - `counts`: the counts of the calls, and the report they are written to.
//! - `trace_buffer`: the trace lines waiting for their batch, the last ones for a crash report, and the thread that writes
//!   the batches in a dev build.
//! - `crash`: the crash report, and the handlers that write it.
//! - `ring`: the file-backed decisions that the app reads while the game runs.
//! - `module`: the module that holds an address, and the place of a call in the program.
//! - `prune`: the removal of the developer files of earlier runs.
//! - `writer`: the files the trace, the report and the crash report are written to.

mod counts;
mod crash;
mod module;
mod prune;
mod ring;
mod session;
mod settings;
mod trace_buffer;
mod writer;

use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) use counts::count;
pub(crate) use crash::{
    install as install_crash_reporting, start_background as start_crash_background,
};
pub(crate) use module::{caller, own_module, remember_modules, remember_own};
pub(crate) use prune::prune_old_files;
pub(crate) use ring::{layer as decision_layer, open as open_ring, record as record_decision};
pub(crate) use session::{HookStatus, Timeline, write as write_session};
pub(crate) use settings::settings;
pub(crate) use trace_buffer::{allow_flusher, flush_trace, trace_line};

/// Starts the threads that wait for the program: the crash filter thread and, in a dev build, the trace thread. They start
/// after the detours are enabled, so that the install runs with one thread in the process and the enable needs no suspension.
pub(crate) fn start_background_threads() {
    start_crash_background();
    allow_flusher();
}

/// Set when a crash report starts. The trace and the counts stop then, so a crash inside them cannot wait for a lock that
/// its own thread holds.
static CRASHING: AtomicBool = AtomicBool::new(false);

/// Whether a crash report is being written.
pub(crate) fn crashing() -> bool {
    CRASHING.load(Ordering::Relaxed)
}

/// Marks the start of a crash report (see `crash`).
fn start_crash_report() {
    CRASHING.store(true, Ordering::Relaxed);
}
