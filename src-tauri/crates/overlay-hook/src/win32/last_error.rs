//! The thread's last error, kept across the work a hook does after the original call.
//!
//! A Win32 function leaves its error in the thread on failure. The hook then traces, counts, writes whiteouts or
//! remembers a handle, and those calls have their own errors. Restoring the value before the hook returns keeps the
//! caller's `GetLastError` the one the original function left.

use windows_sys::Win32::Foundation::{GetLastError, SetLastError};

/// Restores the thread's last error when it is dropped. Create it immediately after the original call.
pub(crate) struct PreserveLastError(u32);

impl PreserveLastError {
    /// Captures the error the original call left on this thread.
    pub(crate) fn capture() -> Self {
        // SAFETY: `GetLastError` only reads the error of the calling thread.
        Self(unsafe { GetLastError() })
    }

    /// The captured error, for a trace line.
    pub(crate) fn value(&self) -> u32 {
        self.0
    }
}

impl Drop for PreserveLastError {
    fn drop(&mut self) {
        // SAFETY: `SetLastError` only stores the error of the calling thread.
        unsafe { SetLastError(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_captured_error_is_restored_when_the_guard_is_dropped() {
        // SAFETY: the test runs on its own thread, and it sets only that thread's error.
        unsafe { SetLastError(1234) };
        {
            let _guard = PreserveLastError::capture();
            // SAFETY: same as above.
            unsafe { SetLastError(5678) };
            assert_eq!(_guard.value(), 1234);
        }
        // SAFETY: same as above.
        assert_eq!(unsafe { GetLastError() }, 1234);
    }

    #[test]
    fn an_outer_guard_restores_the_error_after_an_inner_scope_and_a_later_change() {
        // SAFETY: the test runs on its own thread, and it sets only that thread's error.
        unsafe { SetLastError(1111) };
        {
            let outer = PreserveLastError::capture();
            {
                let inner = PreserveLastError::capture();
                // SAFETY: same as above.
                unsafe { SetLastError(2222) };
                drop(inner);
            }
            // SAFETY: same as above; this stands for the whiteout write after the original call.
            unsafe { SetLastError(3333) };
            drop(outer);
        }
        // SAFETY: same as above.
        assert_eq!(unsafe { GetLastError() }, 1111);
    }

    #[test]
    fn a_guard_taken_only_for_the_whiteout_restores_the_incoming_error() {
        // SAFETY: the test runs on its own thread, and it sets only that thread's error.
        unsafe { SetLastError(4444) };
        {
            // The lower-only delete has no original call to preserve: the guard is taken just before the write.
            let guard = PreserveLastError::capture();
            // SAFETY: same as above; this stands for the whiteout journal I/O.
            unsafe { SetLastError(5555) };
            drop(guard);
        }
        // SAFETY: same as above.
        assert_eq!(unsafe { GetLastError() }, 4444);
        // SAFETY: a failure reported after the guard is gone sets its own error.
        unsafe { SetLastError(6666) };
        assert_eq!(unsafe { GetLastError() }, 6666);
    }
}
