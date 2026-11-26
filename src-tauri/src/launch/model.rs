//! Shared data types of the launch module: the kind of start a launch uses, and the game logs the console shows. They
//! live here so the pipeline and the start code both depend on one lower module instead of on each other.

use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::deploy::overlay::Session;

/// How a game starts: as a plain command, or as the program in the overlay layers. On Windows the overlay starts the
/// game in this process with the hook loaded; on Linux it starts it under `unshare`.
#[derive(Debug)]
pub enum Launch {
    Plain(Command),
    Layered {
        session: Session,
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

/// One log file of the game or its mods, as the console shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct GameLog {
    /// The universal kind: `script`, `loader`, `console`, `crash`, `mod` or `game`.
    pub kind: String,
    /// The file relative to the game folder, with `/` separators.
    pub path: String,
    /// The last lines of the file, oldest first.
    pub lines: Vec<String>,
    /// When the file was last written, in Unix seconds.
    pub modified_at: Option<u32>,
}
