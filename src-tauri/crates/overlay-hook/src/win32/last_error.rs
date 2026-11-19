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
}
