//! Steam, the store adapter for its libraries: the installations (`platform` says where they are on each system), the
//! libraries their `libraryfolders.vdf` lists, a game's folder from its `appmanifest_<id>.acf`, and what the launch
//! needs from Steam: the compatibility tool of an app and the Proton prefix of a game. Steam is not a candidate list like
//! the other stores, because a game is found by its own app ids and folder names, not by scanning every install.

#[cfg(not(windows))]
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::GameDef;

/// The folders where Steam may be installed on this computer.
pub fn steam_installs(home: &Path) -> Vec<PathBuf> {
    crate::platform::steam_installs(home)
}

/// All Steam libraries of this computer: each installation and the folders from its `libraryfolders.vdf`, without repeats.
pub fn libraries(home: &Path) -> Vec<PathBuf> {
    libraries_in(&steam_installs(home))
}

/// The Steam libraries of the given installations, with the folders from each `libraryfolders.vdf`, without repeats.
pub fn libraries_in(steams: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();

    for steam in steams {
        if !steam.join("steamapps").is_dir() {
            continue;
        }

        let vdf =
            fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
        let extra = vdf_values(&vdf, "path").into_iter().map(PathBuf::from);
        for library in std::iter::once(steam.clone()).chain(extra) {
            if library.join("steamapps").is_dir() {
                push_unique(&mut libraries, &mut seen, library);
            }
        }
    }

    libraries
}

/// Adds a library if it is not there yet. Compares paths after resolving links, because `~/.steam/steam` is often a link.
fn push_unique(libraries: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, library: PathBuf) {
    let key = fs::canonicalize(&library).unwrap_or_else(|_| library.clone());

    if seen.insert(key) {
        libraries.push(library);
    }
}

/// Looks for the game in the libraries: first by `appmanifest_<id>.acf` (the `installdir` field), then by name
/// from `steam_install_dirs`. A folder counts when it has the files the marker rule asks for on this platform.
#[cfg(test)]
pub fn find(game: &GameDef, libraries: &[PathBuf]) -> Option<super::super::detect::Found> {
    find_all(game, libraries).into_iter().next()
}

/// Every Steam hit for the game, in library order. The first hit is the preferred install.
pub fn find_all(game: &GameDef, libraries: &[PathBuf]) -> Vec<super::super::detect::Found> {
    libraries
        .iter()
        .flat_map(|library| {
            let steamapps = library.join("steamapps");
            let mut dirs: Vec<PathBuf> = Vec::new();

            for id in &game.detect.steam_app_ids {
                let manifest =
                    fs::read_to_string(steamapps.join(format!("appmanifest_{id}.acf"))).ok();
                if let Some(text) = manifest
                    && let Some(dir) = vdf_values(&text, "installdir").into_iter().next()
                {
                    dirs.push(PathBuf::from(dir));
                }
            }

            dirs.extend(game.detect.steam_install_dirs.iter().map(PathBuf::from));
            dirs.into_iter()
                .map(|dir| steamapps.join("common").join(dir))
                .filter(|root| super::super::detect::has_markers(root, game))
                .map(|root| super::super::detect::Found {
                    game_root: root.display().to_string(),
                    library: library.display().to_string(),
                    store: "steam".to_string(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The Steam library that holds the game folder, that is the directory above `steamapps/common/<game>`.
pub fn library_of(game_root: &Path) -> Option<&Path> {
    let common = game_root.parent()?;
    let steamapps = common.parent()?;
    let library = steamapps.parent()?;
    (common.file_name()? == "common" && steamapps.file_name()? == "steamapps").then_some(library)
}

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
pub fn vdf_values(text: &str, key: &str) -> Vec<String> {
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;
    use std::fmt::Write as _;

    /// A Steam installation in `home` with the libraries from `extra` added to `libraryfolders.vdf`.
    pub fn steam_home(home: &Path, extra: &[&Path]) -> PathBuf {
        let steam = home.join(".local/share/Steam");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let mut vdf = String::from("\"libraryfolders\"\n{\n");

        for (index, library) in extra.iter().enumerate() {
            write!(
                vdf,
                "\t\"{index}\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n",
                library.display()
            )
            .unwrap();
        }

        vdf.push_str("}\n");
        fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        steam
    }

    /// A slot-shaped game folder in a library, with the markers its definition needs.
    pub fn install_slot(library: &Path, dir: &str) -> PathBuf {
        let root = library.join("steamapps").join("common").join(dir);
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("content/base")).unwrap();
        fs::write(root.join("bin/game.exe"), b"").unwrap();
        root
    }

    #[cfg(not(windows))]
    #[test]
    fn steam_names_the_tool_of_an_app_or_the_default_tool() {
        let config = "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"CompatToolMapping\"\n\t\t{\n\t\t\t\"0\"\n\t\t\t{\n\t\t\t\t\"name\"\t\t\"Proton-CachyOS Latest\"\n\t\t\t\t\"config\"\t\t\"\"\n\t\t\t}\n\t\t\t\"1091500\"\n\t\t\t{\n\t\t\t\t\"name\"\t\t\"proton_hotfix\"\n\t\t\t}\n\t\t}\n\t}\n}\n";
        assert_eq!(
            compat_tool(config, "1091500").as_deref(),
            Some("proton_hotfix")
        );
        assert_eq!(
            compat_tool(config, "42").as_deref(),
            Some("Proton-CachyOS Latest")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_config_without_a_compat_tool_mapping_names_no_tool() {
        assert!(compat_tool("\"InstallConfigStore\"\n{\n}\n", "1091500").is_none());
    }

    #[test]
    fn reads_the_libraries_listed_in_libraryfolders_vdf() {
        let home = scratch!("libraries");
        let second = home.join("games-disk");
        fs::create_dir_all(second.join("steamapps")).unwrap();
        let steam = steam_home(&home, &[&second]);
        assert_eq!(
            libraries_in(std::slice::from_ref(&steam)),
            vec![steam, second]
        );
    }

    #[test]
    fn finds_a_game_through_its_manifest_install_dir() {
        let home = scratch!("manifest");
        let library = home.join("games-disk");
        fs::create_dir_all(library.join("steamapps")).unwrap();
        let steam = steam_home(&home, &[&library]);
        let root = install_slot(&library, "Slot Remastered");
        fs::write(
            library.join("steamapps/appmanifest_1001.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1001\"\n\t\"installdir\"\t\t\"Slot Remastered\"\n}\n",
        )
        .unwrap();

        let found =
            find(&test_games::slot(), &libraries_in(&[steam])).expect("the game should be found");
        assert_eq!(found.game_root, root.display().to_string());
        assert_eq!(found.library, library.display().to_string());
    }

    #[test]
    fn falls_back_to_the_install_folder_name_without_a_manifest() {
        let home = scratch!("fallback");
        let steam = steam_home(&home, &[]);
        let root = install_slot(&steam, "Slot Game");

        let found =
            find(&test_games::slot(), &libraries_in(&[steam])).expect("the game should be found");
        assert_eq!(found.game_root, root.display().to_string());
    }

    #[test]
    fn the_library_is_the_folder_above_steamapps_common() {
        assert_eq!(
            library_of(Path::new("/a/steamapps/common/Slot Game")),
            Some(Path::new("/a"))
        );
        assert_eq!(library_of(Path::new("/a/games/Slot Game")), None);
    }
}
