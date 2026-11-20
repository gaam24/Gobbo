//! The files the trace and the count report are written to. A dev build keeps the trace file open for the whole run, so a
//! new open for every batch does not stall the game.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

/// The trace file, kept open for the whole run: a new open for every batch would stall the game.
static TRACE_FILE: Mutex<Option<fs::File>> = Mutex::new(None);
/// Trace lines that could not be written. A full or locked disk must not stop the game; the count goes into a crash
/// report so a missing tail is explained.
static DROPPED_LINES: AtomicU64 = AtomicU64::new(0);
/// The size at which the trace file is moved aside, so a run keeps at most two trace files: the current one and the one
/// before it.
const TRACE_LIMIT: u64 = 64 * 1024 * 1024;

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
    // The rotation holds this dev-only lock across metadata/rename/open/write so a batch cannot interleave with the
    // rename of the file it writes. Documented deviation; see docs/diagnostics.md.
    let mut file = TRACE_FILE.lock().unwrap_or_else(PoisonError::into_inner);
    let full = file
        .as_ref()
        .is_some_and(|open| open.metadata().is_ok_and(|meta| meta.len() >= TRACE_LIMIT));
    if full {
        // The handle is closed first, because Windows does not rename a file that is open.
        *file = None;
        let _ = fs::rename(path, rotated_name(path));
    }

    if file.is_none() {
        *file = Some(open_append(path)?);
    }

    match file.as_mut() {
        Some(file) => file.write_all(text.as_bytes()),
        None => Ok(()),
    }
}

/// Appends `text` to the file, after the folder of the file is made. The crash report and the batches of a release build use it.
pub(super) fn append_file(path: &Path, text: &str) -> std::io::Result<()> {
    open_append(path)?.write_all(text.as_bytes())
}

/// Opens the file for appending, after the folder of the file is made.
fn open_append(path: &Path) -> std::io::Result<fs::File> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }

    fs::OpenOptions::new().create(true).append(true).open(path)
}

/// The name a full trace file is moved to: `<game>.<pid>.old.log` for `<game>.<pid>.trace.log`.
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
}
