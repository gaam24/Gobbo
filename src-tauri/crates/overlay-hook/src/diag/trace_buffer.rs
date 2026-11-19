//! The trace lines: the ones waiting for their batch, the last ones kept for a crash report, and the thread that writes the
//! batches of a dev build every 100 ms. A file is written for each batch, not for each line: opening a file for every
//! traced call stalled the game.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Threading::GetCurrentThreadId;

use super::settings::settings;
use super::writer::write_lines;
use crate::guard::Guard;

/// The trace lines not written yet. The lines still in the buffer when the game is killed are lost.
static TRACE: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The trace is written after this many lines.
const TRACE_BATCH: usize = 200;
/// Starts the thread that writes the trace of a dev build every 100 ms, once. It starts only after the detours are enabled
/// (`allow_flusher`): until then the install must see one thread, and that is what makes the enable cheap.
static FLUSHER_ALLOWED: AtomicBool = AtomicBool::new(false);
static FLUSHER: Once = Once::new();
/// The last trace lines, kept in a dev build for a crash report, oldest first.
static RECENT: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// How many of the last trace lines are kept.
const RECENT_LINES: usize = 512;
/// The number of the next trace line.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
/// When the first trace line was made, for the time of each line.
static START: OnceLock<Instant> = OnceLock::new();

/// Adds one line to the trace, in batches, with its number, its time and its thread. A line added while the trace is
/// being written (the write's own calls, for one) only joins the buffer, and goes out with a later batch.
pub(crate) fn trace_line(line: &str) {
    if settings().log.is_none() || super::crashing() {
        return;
    }
    // A dev build also writes the trace from a thread of its own, so the calls just before a crash are in the file.
    if cfg!(debug_assertions) && FLUSHER_ALLOWED.load(Ordering::Acquire) {
        FLUSHER.call_once(start_flusher);
    }
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let millis = START.get_or_init(Instant::now).elapsed().as_millis();
    // SAFETY: `GetCurrentThreadId` takes no arguments and only reads the id of the calling thread.
    let thread = unsafe { GetCurrentThreadId() };
    let stamped = stamp(sequence, millis, thread, line);
    if cfg!(debug_assertions) {
        let mut recent = RECENT.lock().unwrap_or_else(PoisonError::into_inner);
        keep_last(&mut recent, stamped.clone(), RECENT_LINES);
    }
    let full = {
        let mut buffer = TRACE.lock().unwrap_or_else(PoisonError::into_inner);
        buffer.push(stamped);
        buffer.len() >= TRACE_BATCH
    };
    if full {
        flush_trace();
    }
}

/// One trace line with its number, its time in milliseconds since the first line, and the thread that made it.
fn stamp(sequence: u64, millis: u128, thread: u32, line: &str) -> String {
    format!("#{sequence} {millis}ms thread={thread} {line}")
}

/// Adds `line` as the newest of `lines`, and drops the oldest lines beyond `capacity`.
fn keep_last(lines: &mut VecDeque<String>, line: String, capacity: usize) {
    lines.push_back(line);
    while lines.len() > capacity {
        lines.pop_front();
    }
}

/// The last trace lines, oldest first, for a crash report. Empty while the lock is held: the crash may have come from
/// inside the trace, and the report must not wait for a lock its own thread holds.
pub(super) fn recent_lines() -> Vec<String> {
    match RECENT.try_lock() {
        Ok(lines) => lines.iter().cloned().collect(),
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().iter().cloned().collect(),
        Err(TryLockError::WouldBlock) => Vec::new(),
    }
}

/// Lets a dev build start its trace thread. Called once the detours are enabled, so the install does not run beside it.
pub(crate) fn allow_flusher() {
    FLUSHER_ALLOWED.store(true, Ordering::Release);
    // The lines of the install are written now: a short run can end before the trace thread has its first turn.
    flush_trace();
    if cfg!(debug_assertions) {
        FLUSHER.call_once(start_flusher);
    }
}

/// The thread that writes the trace of a dev build every 100 ms. Its writes are the trace's own, so they go through the
/// guard like any other write.
fn start_flusher() {
    let _ = std::thread::Builder::new()
        .name("gobbo-trace".into())
        .spawn(|| {
            loop {
                std::thread::sleep(Duration::from_millis(100));
                flush_trace();
            }
        });
}

/// Writes the trace lines still waiting in the buffer. The process calls it when it exits: a batch that never filled
/// would otherwise be lost, and the end of a run is what the trace is for. Lines stay in the buffer while the guard is
/// held elsewhere, so a write that cannot start now is not lost.
pub(crate) fn flush_trace() {
    let Some(path) = &settings().log else {
        return;
    };
    let Some(_guard) = Guard::enter() else {
        return;
    };
    let lines = std::mem::take(&mut *TRACE.lock().unwrap_or_else(PoisonError::into_inner));
    write_lines(path, &lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_carries_its_number_its_time_and_its_thread() {
        assert_eq!(
            stamp(7, 1250, 42, "call CreateFileW"),
            "#7 1250ms thread=42 call CreateFileW"
        );
    }

    #[test]
    fn the_oldest_lines_leave_the_ring_first() {
        let mut lines = VecDeque::new();
        for number in 0..5 {
            keep_last(&mut lines, number.to_string(), 3);
        }
        let kept: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_eq!(kept, ["2", "3", "4"]);
    }
}
