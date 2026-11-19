//! The `overlay` strategy: the game folder stays untouched, and the game sees the mods through a layer. On Linux the
//! layer is overlayfs in a namespace, as in Mod Organizer 2 (`overlay/linux.rs`). On Windows a hook DLL, loaded into
//! the game, redirects the game's file calls (`overlay/windows.rs`). This file holds what both have in common: the
//! session folders, the mod layer, undo, and the steps of a game. Both platform files export `check`, `command` and
//! `execute` with the same signatures.
//!
//! `prepare` builds the layer with the mods (hardlinks to the library) and writes the settings with a journal entry.
//! The game folder does not change. `run` starts the program in the layers, and the game's writes in this session go to
//! `overwrite/`. `finish` keeps the mod layer for the next deployment of this profile, and `overwrite/` stays, because it
//! holds the game's writes.

#[cfg(not(windows))]
mod linux;
#[cfg(windows)]
mod windows;

mod place;
mod report;
mod run;
mod session;

#[cfg(not(windows))]
use crate::domain::resolver::DeployPlan;
#[cfg(not(windows))]
use crate::error::Message;
#[cfg(not(windows))]
use std::path::Path;

#[cfg(not(windows))]
pub use crate::platform::overlay_in_use as in_use;
#[cfg(all(not(windows), test))]
pub use crate::platform::overlay_session_pids as session_pids;
#[cfg(all(not(windows), test))]
pub use linux::command;
#[cfg(not(windows))]
pub use linux::{Running, start};
#[cfg(windows)]
pub use windows::{Running, in_use, start};
#[cfg(windows)]
pub use windows::{dll_report, reserved_dll_warnings};

#[cfg(windows)]
pub use report::clear_decisions;
// The overlay root keeps the report's names after the split, including the ones that no caller reads through it.
#[allow(
    unused_imports,
    reason = "the overlay root keeps the report's names as its public surface"
)]
pub use report::{
    CRASH_DUMP_FILE, DECISIONS_BASE, DllReport, InjectedDll, LAST_DUMP_FILE, LAYER_DLLS_FILE,
    LaunchTimings, decisions, decisions_base, latest_dump, layer_dlls, session_layer_dlls,
};
#[cfg(test)]
pub use run::run;
// The same for the run names: `execute`, `run_with_env` and `start_with_env` have no reader outside their module.
#[allow(
    unused_imports,
    reason = "the overlay root keeps the run names as its public surface"
)]
pub use run::{
    Launcher, clear_outputs, execute, run_step, run_with_env, run_with_output, start_with_env,
};
#[cfg(all(test, not(windows)))]
pub(super) use session::matches_plan;
pub use session::{
    Session, check_support, finish, make_room, prepare_with_progress, wait_released,
};
#[cfg(windows)]
pub use session::{mount_config, mounts_path};

/// Overlayfs on Linux virtualizes every file, so no DLL name is reserved there.
#[cfg(not(windows))]
pub const fn reserved_dll_warnings(_plan: &DeployPlan) -> Vec<Message> {
    Vec::new()
}
/// The DLLs of a launch on a platform without the Windows import patch: the mod layer is listed, but nothing is
/// injected through import descriptors.
#[cfg(not(windows))]
pub fn dll_report(session: &Session, _program: &Path) -> DllReport {
    DllReport {
        layer: session_layer_dlls(session),
        injected: Vec::new(),
    }
}
