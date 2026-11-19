//! The app side of the shared minidumper: a server in the Gobbo process for x64 games, and a 32-bit helper for x86.

pub(super) use gobbo_overlay_dump::ENV_DUMPER;
pub(super) use gobbo_overlay_dump::{Monitor, clear_previous};

use std::path::Path;

/// The in-process `DbgHelp` fallback. A hook uses it when its out-of-process monitor is missing.
pub(super) const ENV_IN_PROCESS_DUMP: &str = "GOBBO_OVERLAY_DUMP";

/// Starts the x64 server in this process. A failure leaves the hook with the in-process fallback.
pub(super) fn start(state: &Path) -> Option<Monitor> {
    gobbo_overlay_dump::start(state)
}
