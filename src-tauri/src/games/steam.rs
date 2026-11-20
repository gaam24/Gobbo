//! Steam data shared by game detection and launching: VDF values, the compatibility tool of an app, and the Proton
//! prefix of a game. The Steam installation folders stay in `platform`, because they differ by system.

#[cfg(not(windows))]
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::GameDef;
use super::detect::library_of;

/// A Steam game's Proton prefix: the library the game is in, its app id, and its `compatdata` folder.
pub struct SteamPrefix {
    #[cfg_attr(
        windows,
        allow(
            dead_code,
            reason = "only the Proton launch on Linux reads these; Windows still needs the prefix path"
        )
    )]
    pub library: PathBuf,
    #[cfg_attr(
        windows,
        allow(
            dead_code,
            reason = "only the Proton launch on Linux reads these; Windows still needs the prefix path"
        )
    )]
    pub app_id: String,
    pub compatdata: PathBuf,
}

impl SteamPrefix {
    /// The prefix folder. It exists once the game has run through Proton.
    pub fn wine(&self) -> PathBuf {
        self.compatdata.join("pfx")
    }
}

/// The Steam prefix of a game, from its folder in the Steam library. `None` when the folder is not in a Steam library
/// or the definition names no app id.
pub fn steam_prefix(game: &GameDef, game_root: &Path) -> Option<SteamPrefix> {
    let library = library_of(game_root)?;
    let app_id = game.detect.steam_app_ids.first()?.to_string();
    let compatdata = library.join("steamapps/compatdata").join(&app_id);
    Some(SteamPrefix {
        library: library.to_path_buf(),
        app_id,
        compatdata,
    })
}

/// The Proton prefix of the game, when the game lies in a Steam library and the prefix exists.
/// Overlay cannot cover a root outside the game; user-data paths live here.
pub fn proton_prefix(game: &GameDef, game_root: &Path) -> Option<PathBuf> {
    let prefix = steam_prefix(game, game_root)?.wine();
    prefix.is_dir().then_some(prefix)
}

/// Values of a key from a Steam VDF file. Each line has the form "key"<spacing>"value".
pub(super) fn vdf_values(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('"');
            let (_, name, _, value) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            (name == key).then(|| value.replace("\\\\", "\\"))
        })
        .collect()
}

/// The compatibility tool Steam gives an app: the app's own entry in `CompatToolMapping`, else the default entry `"0"`.
/// `config` is the text of Steam's `config/config.vdf`. `None` when Steam names no tool for the app. Only the Linux
/// Proton path reads it, so Windows does not build it.
#[cfg(not(windows))]
pub fn compat_tool(config: &str, app_id: &str) -> Option<String> {
    let mut lines = config.lines().map(str::trim);
    lines.find(|line| *line == "\"CompatToolMapping\"")?;
    let mut tools: HashMap<String, String> = HashMap::new();
    let mut depth = 0usize;
    let mut app: Option<String> = None;

    for line in lines {
        match line {
            "{" => depth += 1,
            "}" => {
                depth = depth.saturating_sub(1);

                if depth == 0 {
                    break;
                }
            }
            _ => match (depth, line.split('"').collect::<Vec<_>>().as_slice()) {
                (1, ["", key, ""]) => app = Some((*key).to_string()),
                (2, ["", "name", _, value, ""]) => {
                    if let Some(app) = &app {
                        tools.insert(app.clone(), (*value).to_string());
                    }
                }
                _ => {}
            },
        }
    }

    tools.get(app_id).or_else(|| tools.get("0")).cloned()
}
