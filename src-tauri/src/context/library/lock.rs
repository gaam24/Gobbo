//! The lock of the library index: a change holds it from its read of `library.json` to its last write.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Serializes the changes of the index. A change reads the index whole, changes it and writes it back whole, so two
/// changes must not interleave: each holds this lock from its read of the index to its last write of it. Plain reads
/// take no lock, since a write replaces the file in one step.
static INDEX_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// How many `IndexLock`s this thread holds, so a load on it does not wait for the lock it already holds. A depth,
    /// not a flag: dropping an inner guard must not make the outer guard look released.
    pub(super) static HOLDS_INDEX: Cell<u32> = const { Cell::new(0) };
}

/// Holds the lock of the index. Dropping it lets the next change go on.
pub struct IndexLock {
    _guard: MutexGuard<'static, ()>,
}

/// Takes the lock of the index, waiting for a change that runs now. Hold it from the read to the last write.
pub fn lock_index() -> IndexLock {
    let guard = INDEX_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    HOLDS_INDEX.set(HOLDS_INDEX.get().saturating_add(1));
    IndexLock { _guard: guard }
}

/// Whether this thread holds the index lock, so a test can prove that a read happens under it.
#[cfg(test)]
pub fn index_is_held() -> bool {
    HOLDS_INDEX.get() > 0
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        HOLDS_INDEX.set(HOLDS_INDEX.get().saturating_sub(1));
    }
}
