//! Gobbo: the mod list, importing archives from mod providers, deployment and launching the game. Each game area is
//! one module that owns its commands, use cases and engine; the pure rules live in `domain`, the shared context in
//! `context`, and the external services in `providers`.

#![forbid(unsafe_code)]
// The app path may not panic: a release builds with `panic = "abort"`, and the user gets no dialog. Tests keep their
// `unwrap`/`expect` assertions.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod acquire;
mod app;
mod collections;
mod commands;
mod context;
mod deploy;
mod diagnostics;
mod domain;
mod error;
mod games;
mod import;
mod infra;
mod launch;
mod library;
mod migrations;
mod mods;
mod overwrite;
mod packs;
mod platform;
mod profiles;
mod providers;
mod settings;
mod setup;
mod snapshots;
mod sources;
mod status;
mod tray;
mod update;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_support;

#[cfg(feature = "bench")]
#[doc(hidden)]
pub mod bench_api;

pub use app::{run, run_cli};
