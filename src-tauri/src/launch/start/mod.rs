//! Starting and stopping the game, and the command that builds a launch. A launch is a plain command or a program in
//! the overlay layers; its environment, proxy DLLs and overlay session are prepared here and reused by the tools.

mod env;
mod launch;
mod model;
mod session;
#[cfg(test)]
mod tests;

pub use super::model::Launch;
pub use env::{game_env, native_dlls_for_launch};
#[cfg(test)]
pub use launch::launch_command;
pub(in crate::launch) use launch::program_launch;
pub use launch::{launch_game, stop_game};
#[allow(
    unused_imports,
    reason = "the façade keeps the existing `launch::start::Running` path; the pipeline names it only through inference"
)]
pub use model::Running;
pub use session::{has_overlay_session, overlay_session_state};
