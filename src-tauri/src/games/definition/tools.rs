use serde::{Deserialize, Serialize};

use crate::domain::mod_steps::Phase;
use crate::domain::source::SourceRef;
use crate::error::AppError;

/// A program run in the overlay, for example a script compiler that must see the mods.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub phase: Phase,
    /// The program, as a path inside the overlay layers, for example `engine/tools/scc.exe`.
    pub program: String,
    /// The arguments. `{game}` is replaced by the game folder.
    #[serde(default)]
    pub args: Vec<String>,
    /// The step runs only when some layer has this folder, for example `r6/scripts`.
    #[serde(default)]
    pub when_dir: Option<String>,
    /// Files in the overwrite layer removed before the step, so that it never reads its own old output.
    #[serde(default)]
    pub clean: Vec<String>,
    /// Files the step writes into the overwrite layer. They go into the journal, so undo removes them.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// A log the program writes, shown in the interface after the step: its last lines. Relative to the game folder.
    #[serde(default)]
    pub log: Option<String>,
}

/// The universal kind of a log file. The interface translates it once for every game; a definition never carries
/// user-visible text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
    /// A script compiler or its cache.
    Script,
    /// A mod loader or one of its plugins.
    Loader,
    /// A game console, for example Cyber Engine Tweaks.
    Console,
    /// A crash report or a dump.
    Crash,
    /// A log written by one mod.
    Mod,
    /// The game's own log.
    Game,
}

impl LogKind {
    /// The stable name the report carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Loader => "loader",
            Self::Console => "console",
            Self::Crash => "crash",
            Self::Mod => "mod",
            Self::Game => "game",
        }
    }
}

/// A log file or a folder of log files in the game folder. The newest matching files are read from the layers the
/// game actually wrote to, so an overlay keeps the game folder untouched and the log is still where the console looks.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogSource {
    pub kind: LogKind,
    /// A file or a folder, relative to the game folder. `.` is the game folder itself.
    pub path: String,
    /// The file name pattern used when `path` is a folder. Only `*` is a wildcard; matching ignores ASCII case.
    #[serde(default = "default_log_pattern")]
    pub pattern: String,
    /// How many of the newest matching files a folder contributes. A single file ignores it.
    #[serde(default = "default_log_limit")]
    pub limit: u32,
    /// Whether a folder is searched below its subfolders too. The game folder itself should set this to false.
    #[serde(default = "default_true")]
    pub recursive: bool,
}

fn default_log_pattern() -> String {
    "*.log".to_string()
}

const fn default_log_limit() -> u32 {
    4
}

const fn default_true() -> bool {
    true
}

impl Default for LogSource {
    fn default() -> Self {
        Self {
            kind: LogKind::Game,
            path: String::new(),
            pattern: default_log_pattern(),
            limit: default_log_limit(),
            recursive: true,
        }
    }
}

/// A program that comes with the game and makes files for it, for example a script merger. Its `detect` paths are
/// relative to the game folder. Gobbo runs it in the overlay (`run_tool`), and installs it when its source mod is
/// given.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub detect: Vec<String>,
    /// What the tool makes, for example `generated-mod`.
    pub output: String,
    /// The tool's mod on an external provider. Its archive, downloaded from the mod page, is unpacked into the folder
    /// of its program instead of the mod library.
    #[serde(default)]
    pub source: Option<SourceRef>,
    /// Installers the program needs in the Proton prefix, for example the .NET runtime it runs on.
    #[serde(default)]
    pub prerequisites: Vec<Prerequisite>,
}

impl Tool {
    /// The error when the definition names no detectable program.
    pub fn missing_error(&self) -> AppError {
        AppError::user("tools.missing").with("tool", &self.name)
    }
}

/// An installer that a tool needs in the Proton prefix of a Steam game on Linux. Gobbo downloads it from `url`, checks
/// its SHA-512 and runs it through Proton with `args`. It counts as installed when the folder `check` in the prefix
/// holds a folder whose name starts with `check_starts_with`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prerequisite {
    pub name: String,
    /// An https link to the installer, from its publisher.
    pub url: String,
    /// The SHA-512 of the installer as lowercase hex.
    pub sha512: String,
    #[serde(default)]
    #[cfg_attr(
        windows,
        allow(dead_code, reason = "only Proton installs prerequisites")
    )]
    pub args: Vec<String>,
    /// A folder in the prefix, relative to its `pfx` folder.
    pub check: String,
    pub check_starts_with: String,
}
