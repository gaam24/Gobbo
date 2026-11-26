//! The lock of the library index: a change holds it from its read of `library.json` to its last write.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Serializes the changes of the index. A change reads the index whole, changes it and writes it back whole, so two
/// changes must not interleave: each holds this lock from its read of the index to its last write of it. Plain reads
/// take no lock, since a write replaces the file in one step.
static INDEX_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// How many `IndexLock`s this thread holds. The mutex is not reentrant, so a nested change takes no second guard
    /// and only the outermost guard unlocks it. A depth, not a flag: dropping an inner guard must not make the outer
    /// guard look released.
    pub(super) static HOLDS_INDEX: Cell<u32> = const { Cell::new(0) };
}

/// Holds the lock of the index. Dropping the outermost guard lets the next change go on.
pub struct IndexLock {
    /// `None` for a nested lock: this thread already holds the mutex through an outer guard.
    guard: Option<MutexGuard<'static, ()>>,
}

/// Takes the lock of the index, waiting for a change that runs on another thread. Hold it from the read to the last
/// write. A nested call on the same thread returns without taking the mutex again.
pub fn lock_index() -> IndexLock {
    let guard = if HOLDS_INDEX.get() == 0 {
        Some(INDEX_LOCK.lock().unwrap_or_else(PoisonError::into_inner))
    } else {
        None
    };
    HOLDS_INDEX.set(HOLDS_INDEX.get().saturating_add(1));
    IndexLock { guard }
}

/// Whether this thread holds the index lock, so a test can prove that a read happens under it.
#[cfg(test)]
pub fn index_is_held() -> bool {
    HOLDS_INDEX.get() > 0
}

impl IndexLock {
    /// Whether this guard is nested and so holds no mutex of its own.
    #[cfg(test)]
    const fn is_nested(&self) -> bool {
        self.guard.is_none()
    }
}

impl Drop for IndexLock {
    fn drop(&mut self) {
        let _ = self.guard.take();
        HOLDS_INDEX.set(HOLDS_INDEX.get().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_nested_lock_is_reentrant_and_only_the_outer_drop_releases_the_mutex() {
        let outer = lock_index();
        let inner = lock_index();
        assert!(inner.is_nested(), "a nested lock takes no second guard");
        assert!(index_is_held());
        drop(inner);
        assert!(index_is_held(), "the inner drop must not release the lock");

        let (sender, receiver) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                sender.send(INDEX_LOCK.try_lock().is_err()).unwrap();
            });
        });
        assert!(
            receiver.recv().unwrap(),
            "another thread cannot take the mutex"
        );

        drop(outer);
        assert!(!index_is_held());

        let (sender, receiver) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let guard = lock_index();
                sender.send(index_is_held()).unwrap();
                drop(guard);
            });
        });
        assert!(
            receiver.recv().unwrap(),
            "another thread takes the released lock"
        );
    }
}
