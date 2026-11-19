//! The lines of the trace that the hooked calls add: a call, its status, and its result. Each one is added only when
//! `GOBBO_OVERLAY_LOG` is set (see `diag`), and the message is built only then.

use crate::diag;
use crate::text::display_name;

/// Counts one call of a hooked function and adds it to the trace with the detail that `detail` builds. Both happen
/// only when `GOBBO_OVERLAY_CALLS` is set: a line per call with a stack walk is too expensive for normal play, and it
/// made the dev build unrepresentative. Decisions and errors are still traced without it.
pub(crate) fn call_with(operation: &str, detail: impl FnOnce() -> String) {
    if !diag::settings().calls {
        return;
    }
    let detail = detail();
    diag::count(operation, || detail.clone());
    trace(|| format!("call {operation} {detail} from {}", diag::caller()));
}

/// Counts one call of a hooked function that takes a name, and adds it to the trace (see `call_with`).
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn trace_call(operation: &str, name: *const u16) {
    call_with(operation, || unsafe { display_name(name) });
}

/// Logs a write, delete or rename that leaves the game folder untouched because its path is outside the layers. This
/// is opt-in (`GOBBO_OVERLAY_PASS`): a game normally writes its saves and settings outside the game folder, and the
/// line is only useful when a new game writes somewhere unexpected.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn pass_through(
    operation: &str,
    name: *const u16,
    detail: impl FnOnce() -> String,
) {
    if !diag::settings().pass_writes {
        return;
    }
    let name = unsafe { display_name(name) };
    log(&format!("pass-through {operation} {name} {}", detail()));
}

/// Logs the status an NT call returned, when `GOBBO_OVERLAY_LOG` is set.
pub(crate) fn trace_status(operation: &str, status: i32) {
    if !diag::settings().calls {
        return;
    }
    trace(|| format!("status {operation} 0x{:08x}", status as u32));
}

/// The same as `trace_status`, for a call site that has no prebuilt operation name. The line is built only when per-call
/// tracing is on, so a loader that asks for attributes does not allocate for a message nobody reads.
pub(crate) fn trace_status_named(operation: &str, status: i32) {
    if !diag::settings().calls {
        return;
    }
    trace(|| format!("status {operation} 0x{:08x}", status as u32));
}

/// Logs whether a Win32 call succeeded and the error it left, when `GOBBO_OVERLAY_LOG` is set. The error is read by the
/// caller before this call, because the log's own file write may change it.
pub(crate) fn trace_result(operation: &str, succeeded: bool, error: u32) {
    if !diag::settings().calls {
        return;
    }
    trace(|| format!("result {operation} succeeded={succeeded} error={error}"));
}

/// Adds the line that `message` builds to the trace, when `GOBBO_OVERLAY_LOG` is set (see `diag::trace_line`).
pub(crate) fn trace(message: impl FnOnce() -> String) {
    if diag::settings().log.is_none() {
        return;
    }
    diag::trace_line(&message());
}

/// Adds a line to the trace, when `GOBBO_OVERLAY_LOG` is set (see `diag::trace_line`).
pub(crate) fn log(message: &str) {
    diag::trace_line(message);
}
