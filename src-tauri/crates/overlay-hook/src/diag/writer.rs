//! The files the trace and the count report are written to. A dev build keeps the trace file open for the whole run, so a
//! new open for every batch does not stall the game.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

/// The trace file, kept open for the whole run: a new open for every batch would stall the game.
static TRACE_FILE: Mutex<Option<fs::File>> = Mutex::new(None);
/// The bytes written to that file since it was opened. `metadata()` used to be called on every batch only to read this;
/// the counter makes the rotation check a load. It is next to `TRACE_FILE` and only touched under its lock, apart from
/// the load in `needs_rotation`.
static TRACE_BYTES: AtomicU64 = AtomicU64::new(0);
/// Trace lines that could not be written. A full or locked disk must not stop the game; the count goes into a crash
/// report so a missing tail is explained.
static DROPPED_LINES: AtomicU64 = AtomicU64::new(0);
/// How many appends are in progress. The trace and the counts stop while a crash report is being written, so a crash
/// inside them cannot wait for a lock its own thread holds; they resume afterwards, so a first-chance exception the
/// program handles does not stop them for the rest of the process. A counter keeps nested appends from ending the
/// suppression early.
static REPORT_DEPTH: AtomicUsize = AtomicUsize::new(0);
/// The size at which the trace file is moved aside, so a run keeps at most two trace files: the current one and the one
/// before it.
const TRACE_LIMIT: u64 = 64 * 1024 * 1024;

/// Whether a report or trace append is in progress, so callers can avoid a lock that the crash path may hold.
pub(super) fn reporting() -> bool {
    REPORT_DEPTH.load(Ordering::Acquire) != 0
}

/// Test-only control of the append depth, so a test can hold the suppression without running a real file write.
#[cfg(test)]
pub(crate) fn hold_append(active: bool) {
    if active {
        REPORT_DEPTH.fetch_add(1, Ordering::Release);
    } else {
        REPORT_DEPTH.fetch_sub(1, Ordering::Release);
    }
}

/// Writes `text` to the file, after the folder of the file is made. Every write of the report goes through here.
pub(super) fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }

    fs::write(path, text)
}

/// Appends the lines to the trace file, one per line of text. Nothing is written for no lines.
pub(super) fn write_lines(path: &Path, lines: &[String]) {
    if lines.is_empty() {
        return;
    }

    let mut text = String::new();

    for line in lines {
        text.push_str(line);
        text.push('\n');
    }

    let written = if cfg!(debug_assertions) {
        write_kept_open(path, &text)
    } else {
        append_file(path, &text)
    };

    if written.is_err() {
        DROPPED_LINES.fetch_add(lines.len() as u64, Ordering::Relaxed);
    }
}

/// The number of trace lines the hook could not write. The crash report names it, so a missing tail is explained.
pub(super) fn dropped_lines() -> u64 {
    DROPPED_LINES.load(Ordering::Relaxed)
}

/// Appends `text` through the trace file that stays open for this run (a dev build). When the file has reached
/// `TRACE_LIMIT`, it is moved to its old name, which replaces the one before it, and a new file is started. The caller
/// holds the hook's guard, so opening the file is not hooked.
fn write_kept_open(path: &Path, text: &str) -> std::io::Result<()> {
    // The rotation holds this dev-only lock across rename/open/write so a batch cannot interleave with the rename of
    // the file it writes. The size check is a counter load, not a `metadata()` call. Documented deviation; see
    // docs/diagnostics.md.
    let mut file = TRACE_FILE.lock().unwrap_or_else(PoisonError::into_inner);
    if needs_rotation(TRACE_BYTES.load(Ordering::Relaxed)) {
        // The handle is closed first, because Windows does not rename a file that is open.
        *file = None;
        let _ = fs::rename(path, rotated_name(path));
        TRACE_BYTES.store(0, Ordering::Relaxed);
    }

    if file.is_none() {
        let opened = open_append(path)?;
        // A failed rename leaves the old file in place. Start from its size once, which is what `metadata()` read
        // before every batch did, so the rotation threshold is unchanged.
        TRACE_BYTES.store(
            opened.metadata().map_or(0, |meta| meta.len()),
            Ordering::Relaxed,
        );
        *file = Some(opened);
    }

    let written = match file.as_mut() {
        Some(file) => file.write_all(text.as_bytes()),
        None => Ok(()),
    };
    if written.is_ok() {
        TRACE_BYTES.fetch_add(text.len() as u64, Ordering::Relaxed);
    }
    written
}

/// Whether the trace file has reached the size at which it is moved aside.
fn needs_rotation(bytes: u64) -> bool {
    bytes >= TRACE_LIMIT
}

/// Appends `text` to the file, after the folder of the file is made. The crash report and the batches of a release
/// build use it. The report's write is the boundary for suppressing trace and counts (see `REPORT_DEPTH`), so that
/// suppression ends with the report instead of lasting past a handled first-chance exception.
pub(super) fn append_file(path: &Path, text: &str) -> std::io::Result<()> {
    REPORT_DEPTH.fetch_add(1, Ordering::Release);
    let result = open_append(path).and_then(|mut file| file.write_all(text.as_bytes()));
    REPORT_DEPTH.fetch_sub(1, Ordering::Release);
    result
}

/// Opens the file for appending, after the folder of the file is made.
fn open_append(path: &Path) -> std::io::Result<fs::File> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }

    fs::OpenOptions::new().create(true).append(true).open(path)
}

/// The name a full trace file is moved to: `<game>.<pid>.trace.old.log` for `<game>.<pid>.trace.log`.
fn rotated_name(path: &Path) -> PathBuf {
    path.with_extension("old.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_trace_file_is_moved_to_the_old_name() {
        assert_eq!(
            rotated_name(Path::new("C:\\x\\game.12.trace.log")),
            PathBuf::from("C:\\x\\game.12.trace.old.log")
        );
    }

    #[test]
    fn rotation_is_driven_by_the_byte_counter() {
        assert!(!needs_rotation(TRACE_LIMIT - 1));
        assert!(needs_rotation(TRACE_LIMIT));
        assert!(needs_rotation(TRACE_LIMIT + 1));
    }

    /// A first-chance exception the program handles must not leave trace and counts switched off for the rest of the
    /// process. The suppression is bounded by the report's append, not by the sticky crash flag.
    #[test]
    fn reporting_ends_with_the_append() {
        let path = std::env::temp_dir().join(format!("gobbo-writer-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(!reporting());
        append_file(&path, "a line\n").unwrap();
        assert!(
            !reporting(),
            "the append left the trace/count suppression on"
        );
        let _ = std::fs::remove_file(&path);
    }
}
