//! What differs between Windows and Linux, behind one set of names: the process list that says whether a game runs,
//! and Proton on Linux. `linux.rs` and `windows.rs` export the same names, so a function that exists on one platform
//! only is a compile error on the other, not a runtime surprise.

mod efficiency;
#[cfg(not(windows))]
mod linux;
#[cfg(any(not(windows), test))]
mod mount_table;
#[cfg(windows)]
mod windows;

pub use efficiency::set_efficiency_mode;

/// Whether this build can run a game in an overlay layer. One answer for the launch hint, preflight, prepare and start,
/// so a new supported platform is added here and nowhere else.
pub const fn supports_overlay() -> bool {
    cfg!(any(target_os = "linux", windows))
}

/// Whether this build runs on Windows. The behavioral edges ask here instead of reading `cfg!` themselves, so both
/// platform sides give the same answer and a new OS changes one function.
pub const fn is_windows() -> bool {
    cfg!(windows)
}

/// The key of the portable archive in the update manifest for this OS and architecture, such as
/// `linux-x86_64-portable`. `None` where this build has no portable archive. The mapping is pure so both platform
/// modules export the same name and a test can check every target on one machine.
pub fn portable_target(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("windows-x86_64-portable"),
        ("linux", "x86_64") => Some("linux-x86_64-portable"),
        _ => None,
    }
}

#[cfg(all(not(windows), test))]
pub use linux::overlay_session_pids;
#[cfg(not(windows))]
pub use linux::{
    Process, find_proton, game_is_running, installed_protons, overlay_in_use, proton_at,
    proton_named, steam_installs, steam_root, stop_game, store_folders,
    supports_user_data_redirect,
};
#[cfg(windows)]
pub use windows::{
    Process, game_is_running, installed_protons, steam_installs, stop_game, store_folders,
    supports_user_data_redirect,
};

/// Where the game stores other than Steam keep their data on this computer. Each system fills in what it has and
/// leaves the rest empty: Windows has the Epic launcher and GOG Galaxy's registry, Linux has Lutris and the Flatpak
/// folders. The stores read the files; this only says where they are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreFolders {
    /// The Epic launcher's data folder, which holds `Manifests`: `AppDataPath` from the registry, as the launcher writes
    /// it, else `%ProgramData%/Epic/EpicGamesLauncher/Data`.
    pub epic_data: Option<std::path::PathBuf>,
    /// The folder of the older Epic list `LauncherInstalled.dat`: `%ProgramData%/Epic/UnrealEngineLauncher`.
    pub epic_list: Option<std::path::PathBuf>,
    /// Folders whose subfolders may be GOG games, each with its `goggame-<id>.info` files.
    pub gog_libraries: Vec<std::path::PathBuf>,
    /// The `(product id, folder)` pairs GOG Galaxy and GOG's installers leave in the registry.
    pub gog_registry: Vec<(String, std::path::PathBuf)>,
    /// minigalaxy's `config.json`, which names the folder it installs GOG games into.
    pub minigalaxy: Option<std::path::PathBuf>,
    /// Heroic's config folders: the native one and, on Linux, the Flatpak one.
    pub heroic: Vec<std::path::PathBuf>,
    /// Legendary's own config folder, for Epic games installed with Legendary outside Heroic.
    pub legendary: Option<std::path::PathBuf>,
    /// Lutris's folders: the native ones and the Flatpak ones.
    pub lutris: Vec<LutrisFolders>,
}

/// One Lutris install's folders. Lutris keeps its game files in `<config>/games` while `config` exists, else in
/// `<data>/games`, and its database in `<data>/pga.db`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LutrisFolders {
    pub config: std::path::PathBuf,
    pub data: std::path::PathBuf,
}

/// Legendary's config folder, the same rule on every system: `LEGENDARY_CONFIG_PATH`, else
/// `$XDG_CONFIG_HOME/legendary`, else `~/.config/legendary` (also on Windows, where Legendary uses no `%APPDATA%`).
fn legendary_dir(
    home: &std::path::Path,
    configured: Option<std::ffi::OsString>,
    xdg: Option<std::ffi::OsString>,
) -> std::path::PathBuf {
    configured.map_or_else(
        || {
            xdg.map_or_else(|| home.join(".config"), std::path::PathBuf::from)
                .join("legendary")
        },
        std::path::PathBuf::from,
    )
}

/// A Proton build on this computer: the name Steam shows for it, and the folder that holds the `proton` file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ProtonBuild {
    /// The `display_name` of the tool's `compatibilitytool.vdf` when it has one, else the folder's name, for example
    /// `Proton 9.0` or `GE-Proton9-20`.
    pub name: String,
    /// The folder with the `proton` file. The game settings store this folder when the user picks the build.
    pub folder: String,
}

use std::collections::{HashMap, HashSet, VecDeque};

/// The roots and every process below them, with the children first. A game is stopped from its leaves up, so no
/// process can start a child after its parent is gone.
fn descendants(roots: Vec<u32>, parents: &HashMap<u32, u32>) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();

    for (pid, parent) in parents {
        children.entry(*parent).or_default().push(*pid);
    }

    let mut seen = HashSet::new();
    let mut queue: VecDeque<u32> = roots.into();
    let mut ordered = Vec::new();

    while let Some(pid) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }

        ordered.push(pid);

        if let Some(kids) = children.get(&pid) {
            queue.extend(kids);
        }
    }

    ordered.reverse();
    ordered
}

/// Gives a started process a moment, then reports whether the game in `game_root` is seen running, and stops the process.
#[cfg(test)]
fn seen_while_running(game_root: &std::path::Path, mut child: std::process::Child) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(300));
    let running = game_is_running(game_root);
    child.kill().unwrap();
    child.wait().unwrap();
    running
}

#[cfg(test)]
mod store_folder_tests {
    use super::legendary_dir;
    use std::path::Path;

    #[test]
    fn legendary_takes_its_override_then_xdg_then_the_home_config_folder() {
        let home = Path::new("/home/slot");
        assert_eq!(
            legendary_dir(home, Some("/custom/legendary".into()), Some("/xdg".into())),
            Path::new("/custom/legendary")
        );
        assert_eq!(
            legendary_dir(home, None, Some("/xdg".into())),
            Path::new("/xdg/legendary")
        );
        assert_eq!(
            legendary_dir(home, None, None),
            home.join(".config/legendary")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_tree_is_stopped_from_its_leaves_up() {
        let parents = HashMap::from([(2, 1), (3, 1), (4, 2)]);
        let order = descendants(vec![1], &parents);

        assert_eq!(order.last(), Some(&1));
        assert!(order.iter().position(|pid| *pid == 4) < order.iter().position(|pid| *pid == 2));
        assert_eq!(
            order.iter().copied().collect::<HashSet<_>>(),
            HashSet::from([1, 2, 3, 4])
        );
    }
}
