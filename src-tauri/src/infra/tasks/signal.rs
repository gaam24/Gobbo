//! The shared helpers for reading a task's cancellation flag and stopping work that asks to be stopped.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether an optional flag asks the work to stop. The helper keeps the call sites to one line.
pub fn cancelled(flag: Option<&AtomicBool>) -> bool {
    flag.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

/// Stops the work with the stable `code` when the flag asks for it: `stop_if(cancel, IMPORT_CANCELLED)?`.
pub fn stop_if(
    flag: Option<&AtomicBool>,
    code: &'static str,
) -> Result<(), crate::error::AppError> {
    if cancelled(flag) {
        Err(crate::error::AppError::user(code))
    } else {
        Ok(())
    }
}
