//! The lock of `profiles.json`: a change holds it from its read of the profile list to its last write, and the history
//! index that goes with it.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Serializes changes of the profile list. A change reads the list whole, changes it and writes it back whole, so two
/// changes must not interleave. Plain reads take no lock, since a write replaces the file in one step.
static PROFILES_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// How many `ProfileLock`s this thread holds. The mutex is not reentrant, so a nested change takes no second guard
    /// and only the outermost guard unlocks it.
    static HOLDS_PROFILES: Cell<u32> = const { Cell::new(0) };
}

/// Holds the lock of the profile list. Dropping the outermost guard lets the next change go on.
pub struct ProfileLock {
    /// `None` for a nested lock: this thread already holds the mutex through an outer guard.
    _guard: Option<MutexGuard<'static, ()>>,
}

/// Takes the lock of the profile list, waiting for a change that runs on another thread. Hold it from the read to the
/// last write. A nested call on the same thread returns without taking the mutex again.
pub fn lock_profiles() -> ProfileLock {
    let guard = if HOLDS_PROFILES.get() == 0 {
        Some(PROFILES_LOCK.lock().unwrap_or_else(PoisonError::into_inner))
    } else {
        None
    };
    HOLDS_PROFILES.set(HOLDS_PROFILES.get().saturating_add(1));
    ProfileLock { _guard: guard }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        HOLDS_PROFILES.set(HOLDS_PROFILES.get().saturating_sub(1));
    }
}
