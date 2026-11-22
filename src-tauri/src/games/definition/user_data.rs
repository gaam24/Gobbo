use serde::{Deserialize, Serialize};

use crate::domain::user_data::{Isolation, Kind, Mode};
use crate::games::layout::prefer_linux;

/// One folder the game writes outside its own folder: saves, settings or both. The definition names the folder and
/// how Gobbo may isolate it; the profile decides whether it is isolated.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserData {
    /// Stable id, unique in the game. It names the profile's state folder and is not user-visible text.
    pub id: String,
    /// The universal kind the frontend translates. Defaults to `user-data`.
    #[serde(default)]
    pub kind: Kind,
    /// The folder as a path template, for example `{documents}/The Witcher 3`.
    pub path: String,
    /// The template for a native Linux build. Empty or missing falls back to `path`; a Windows path that names
    /// `{local_low}` is not meaningful on Linux.
    #[serde(default)]
    pub path_linux: Option<String>,
    /// How an isolated set is served. Defaults to `redirect`; the app falls back to `copy` when the platform cannot redirect.
    #[serde(default)]
    pub mode: Mode,
    /// What new profiles use when the user does not choose. Defaults to `shared`.
    #[serde(default)]
    pub default: Isolation,
    /// Whether an isolated redirect still sees the existing real folder as a read-only base. Defaults to true.
    #[serde(default = "inherit_by_default")]
    pub inherit: bool,
}

const fn inherit_by_default() -> bool {
    true
}

impl UserData {
    /// The folder template for this platform. Windows uses `path`; Linux prefers a non-empty `path_linux`, because a
    /// Windows path that names `{local_low}` is not meaningful there.
    pub fn path_for_platform(&self) -> &str {
        prefer_linux(&self.path, self.path_linux.as_deref(), !cfg!(windows))
    }

    /// The template a Linux build uses: `path_linux` when it is not empty, otherwise `path`.
    pub fn linux_path(&self) -> &str {
        prefer_linux(&self.path, self.path_linux.as_deref(), true)
    }
}

/// The evidence that a definition was tested on a real installation. A user definition may leave it out; the
/// interface then calls it unverified instead of guessing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct Verified {
    /// The day of the test, `YYYY-MM-DD`.
    pub date: String,
    /// The systems it was tested on, for example `windows` and `linux`.
    #[serde(default)]
    pub os: Vec<String>,
    /// The game build the test used.
    #[serde(default)]
    pub game_version: Option<String>,
    /// The loader build the test used.
    #[serde(default)]
    pub loader_version: Option<String>,
}

/// The store ids, install folders and files that identify a game during detection.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detect {
    #[serde(default)]
    pub steam_app_ids: Vec<u32>,
    #[serde(default)]
    pub steam_install_dirs: Vec<String>,
    /// GOG product ids, for a `goggame-<id>.info` folder and Heroic's GOG store.
    #[serde(default)]
    pub gog_ids: Vec<String>,
    /// Epic `AppName`s, for `LauncherInstalled.dat` and Heroic's legendary store.
    #[serde(default)]
    pub epic_ids: Vec<String>,
    /// Heroic slugs, for a store that keeps a game under its own name.
    #[serde(default)]
    pub heroic_slugs: Vec<String>,
    /// Lutris `game_slug` values.
    #[serde(default)]
    pub lutris_slugs: Vec<String>,
    /// Files that identify the game folder, relative to it.
    #[serde(default)]
    pub markers: Vec<String>,
    /// Files that identify the folder of the game's native Linux build, relative to it. Used only on Linux; when empty,
    /// detection uses `markers` there as it always has.
    #[serde(default)]
    pub linux_markers: Vec<String>,
    /// One marker is enough, for a game whose builds name their files differently. By default every marker must exist.
    /// The same rule applies to `linux_markers`.
    #[serde(default)]
    pub any_marker: bool,
}

/// One launch file of the game in a definition: its name, program and predefined arguments.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executable {
    pub name: String,
    /// Relative to the game folder.
    pub path: String,
    /// The native Linux program, when the game ships one. Empty or missing falls back to `path`. A definition declares
    /// one only when that program starts directly, whatever store the install came from.
    #[serde(default)]
    pub path_linux: Option<String>,
    /// Arguments added before the user's arguments, for example `-windowed`.
    #[serde(default)]
    pub args: Vec<String>,
}

impl Executable {
    /// The program for this platform. Windows always uses `path`; Linux prefers a non-empty `path_linux`.
    pub fn path_for_platform(&self) -> &str {
        prefer_linux(&self.path, self.path_linux.as_deref(), !cfg!(windows))
    }
}

/// The file the game reads the order of its mods from. The priorities the user sets are kept in the range
/// `priority_min..=priority_max`, and a lower number wins.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadOrder {
    /// The only format so far. Reading it still refuses a format Gobbo cannot write.
    #[allow(
        dead_code,
        reason = "one format so far; the field rejects the formats Gobbo cannot write"
    )]
    pub format: OrderFormat,
    /// The file, as a path template. The user can point to another folder; the file name stays.
    pub file: String,
    #[serde(default = "default_priority_min")]
    pub priority_min: u32,
    #[serde(default = "default_priority_max")]
    pub priority_max: u32,
}

const fn default_priority_min() -> u32 {
    1
}

const fn default_priority_max() -> u32 {
    9999
}

/// The format of the file the game reads its mod order from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderFormat {
    /// A section per mod folder, with `Enabled` and `Priority` (The Witcher 3's `mods.settings`).
    ModsSettings,
}
