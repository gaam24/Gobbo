//! The overwrite commands and the multi-path winner edit. The commands read the files the game wrote into the overlay,
//! turn them into a mod or clear them through `deploy::overwrite`, while a single file winner is edited in `library`.
//! This module owns the IPC and the one list edit that sets a winner for several paths at once.

pub mod commands;
mod winners;

pub use winners::set_file_winners;
