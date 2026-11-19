//! The guard that keeps the hook's own calls out of the hook. A call made while the hook runs (to check or copy a file, for
//! one) passes through to Windows unchanged.

use std::cell::Cell;

thread_local! {
    /// Set while the hook runs. The file calls the hook makes itself (to check or copy files) pass through.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

/// Marks the thread as inside the hook for as long as it lives.
pub(crate) struct Guard;

impl Guard {
    /// Marks the thread as inside the hook, or returns `None` when it already is.
    pub(crate) fn enter() -> Option<Self> {
        INSIDE.with(|flag| {
            if flag.get() {
                None
            } else {
                flag.set(true);
                Some(Self)
            }
        })
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        INSIDE.with(|flag| flag.set(false));
    }
}

/// Enters the hook for a call that works on the mounts of the session. `None` when the call comes from inside the hook,
/// or when the environment named no mounts, and the call goes ahead unchanged.
pub(crate) fn enter_session() -> Option<Guard> {
    let guard = Guard::enter()?;
    crate::state::mounts()?;
    Some(guard)
}
