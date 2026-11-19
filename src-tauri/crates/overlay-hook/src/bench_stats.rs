//! Probe, lock and file-I/O counters for `overlay-bench`, behind the `bench-stats` feature.
//!
//! The hook's own calls are invisible to the benchmark process, so these counters prove the pass-through budget:
//! zero probes, zero locks and zero file I/O when a path is outside the game folder. A release hook never enables the
//! feature; the counters cost one relaxed atomic only in the benchmark DLL.

use std::sync::atomic::{AtomicU64, Ordering};

static PROBES: AtomicU64 = AtomicU64::new(0);
static LOCKS: AtomicU64 = AtomicU64::new(0);
static IO: AtomicU64 = AtomicU64::new(0);
static SPELLING: AtomicU64 = AtomicU64::new(0);

/// One attribute query or directory enumeration made by the hook.
#[inline]
pub(crate) fn probe() {
    PROBES.fetch_add(1, Ordering::Relaxed);
    IO.fetch_add(1, Ordering::Relaxed);
}

/// One lock taken on a hook table or the whiteout snapshot.
#[inline]
pub(crate) fn lock() {
    LOCKS.fetch_add(1, Ordering::Relaxed);
}

/// One file-system call made by the hook that is not already a `probe` (copy, journal, ring).
#[inline]
pub(crate) fn io() {
    IO.fetch_add(1, Ordering::Relaxed);
}

/// One exact-name or root-pass lookup made to keep the game's spelling in a listing.
#[inline]
pub(crate) fn spelling() {
    SPELLING.fetch_add(1, Ordering::Relaxed);
}

/// The number of probes since the last reset, for the benchmark worker.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_probes() -> u64 {
    PROBES.load(Ordering::Relaxed)
}

/// The number of locks since the last reset, for the benchmark worker.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_locks() -> u64 {
    LOCKS.load(Ordering::Relaxed)
}

/// The number of file-system calls since the last reset, for the benchmark worker.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_io() -> u64 {
    IO.load(Ordering::Relaxed)
}

/// The number of spelling lookups since the last reset, for the benchmark worker.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_spelling() -> u64 {
    SPELLING.load(Ordering::Relaxed)
}

/// Resets all stats counters. `bench_alloc` has its own reset for allocations.
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_overlay_bench_reset_stats() {
    PROBES.store(0, Ordering::Relaxed);
    LOCKS.store(0, Ordering::Relaxed);
    IO.store(0, Ordering::Relaxed);
    SPELLING.store(0, Ordering::Relaxed);
}
