//! Windows: there is no Proton, so every game runs its own file. The check whether a game runs looks at the executable
//! of each process: it must lie in the game folder.

use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::ProtonBuild;
use crate::domain::pathkey::windows_key;

/// Whether the platform can redirect a user-data folder instead of copying it around a launch.
pub const fn supports_user_data_redirect() -> bool {
    true
}

/// Where the other stores keep their data on Windows: the Epic launcher's folders (its data folder from the registry,
/// as MO2 reads it), GOG's registry entries and default folders, Heroic under `%APPDATA%` and Legendary. Windows has no
/// Lutris and no Flatpak.
pub fn store_folders(home: &Path) -> super::StoreFolders {
    let env_dir = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let program_data = env_dir("PROGRAMDATA");
    let appdata = env_dir("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"));
    let mut gog_libraries: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles"]
        .into_iter()
        .filter_map(env_dir)
        .map(|folder| folder.join("GOG Galaxy").join("Games"))
        .collect();

    // GOG's offline installers default to `<system drive>\GOG Games`.
    if let Some(drive) = std::env::var_os("SystemDrive") {
        let mut root = drive;
        root.push("\\");
        gog_libraries.push(PathBuf::from(root).join("GOG Games"));
    }

    super::StoreFolders {
        epic_data: registry_string(EPIC_KEYS, "AppDataPath")
            .map(PathBuf::from)
            .or_else(|| {
                program_data
                    .as_ref()
                    .map(|folder| folder.join("Epic").join("EpicGamesLauncher").join("Data"))
            }),
        epic_list: program_data.map(|folder| folder.join("Epic").join("UnrealEngineLauncher")),
        gog_libraries,
        gog_registry: gog_registry(),
        minigalaxy: None,
        heroic: vec![appdata.join("heroic")],
        legendary: Some(super::legendary_dir(
            home,
            std::env::var_os("LEGENDARY_CONFIG_PATH"),
            std::env::var_os("XDG_CONFIG_HOME"),
        )),
        lutris: Vec::new(),
    }
}

/// The Epic launcher's key, in the 32-bit view first, where the 32-bit launcher writes it.
const EPIC_KEYS: [&str; 2] = [
    "SOFTWARE\\WOW6432Node\\Epic Games\\EpicGamesLauncher",
    "SOFTWARE\\Epic Games\\EpicGamesLauncher",
];

/// The first non-empty string value `name` under one of `keys` in `HKEY_LOCAL_MACHINE`.
fn registry_string(keys: [&str; 2], name: &str) -> Option<String> {
    keys.into_iter().find_map(|key| {
        let value = windows_registry::LOCAL_MACHINE
            .open(key)
            .ok()?
            .get_string(name)
            .ok()?;
        (!value.trim().is_empty()).then_some(value)
    })
}

/// The install folders GOG leaves in the registry: one subkey per product id with its folder in `path`. Both views are
/// read, because the 32-bit Galaxy writes to `WOW6432Node`. Only numeric subkeys are product ids.
fn gog_registry() -> Vec<(String, PathBuf)> {
    let mut pairs = Vec::new();

    for root in [
        "SOFTWARE\\WOW6432Node\\GOG.com\\Games",
        "SOFTWARE\\GOG.com\\Games",
    ] {
        let Ok(key) = windows_registry::LOCAL_MACHINE.open(root) else {
            continue;
        };
        let Ok(ids) = key.keys() else {
            continue;
        };

        for id in ids.filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())) {
            if let Ok(path) = key.open(&id).and_then(|game| game.get_string("path"))
                && !path.trim().is_empty()
            {
                pairs.push((id, PathBuf::from(path)));
            }
        }
    }

    pairs
}

/// The folders where Steam may be installed on Windows: Program Files, where the 32-bit variable names the x86 folder.
/// A Steam installed elsewhere is not found, and the user adds the game folder.
pub fn steam_installs(_home: &Path) -> Vec<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join("Steam"))
        .collect()
}

/// Every Proton build on this computer. Windows has no Proton, so there is none to offer.
pub const fn installed_protons(_steams: &[PathBuf], _libraries: &[PathBuf]) -> Vec<ProtonBuild> {
    Vec::new()
}

/// Whether a process runs a game from this folder: any process whose executable lies in the folder.
pub fn game_is_running(game_root: &Path) -> bool {
    let folder = windows_key(&game_root.to_string_lossy());
    let mut system = System::new();
    refresh(&mut system);
    system
        .processes()
        .values()
        .filter_map(|process| process.exe())
        .any(|exe| in_game_folder(exe, &folder))
}

/// Stops every process of the game in `game_root`, and every process below it. `taskkill` without `/F` asks a window
/// to close, which lets the game save; a process still there after the grace period is terminated. The process list is
/// read with the same rule as `game_is_running`. Children are included because a launcher can start the game from
/// another folder. Returns whether at least one process was stopped.
pub fn stop_game(game_root: &Path) -> bool {
    let folder = windows_key(&game_root.to_string_lossy());
    let mut system = System::new();
    refresh(&mut system);
    let mut parents = HashMap::new();
    let mut roots = Vec::new();

    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            parents.insert(pid.as_u32(), parent.as_u32());
        }

        if process
            .exe()
            .is_some_and(|exe| in_game_folder(exe, &folder))
        {
            roots.push(pid.as_u32());
        }
    }

    let pids = super::descendants(roots, &parents);
    let mut stopped = false;

    for &pid in &pids {
        let asked = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0800_0000)
            .status()
            .is_ok_and(|status| status.success());
        stopped |= asked;
    }

    let deadline = Instant::now() + Duration::from_secs(2);

    loop {
        refresh(&mut system);
        let alive = pids
            .iter()
            .any(|pid| system.process(Pid::from_u32(*pid)).is_some());
        if !alive || Instant::now() >= deadline {
            break;
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    for pid in pids {
        if let Some(process) = system.process(Pid::from_u32(pid)) {
            stopped |= process.kill();
        }
    }

    stopped
}

/// A handle to a running process Gobbo started, identified by its id. [`Self::stop`] can be called from another
/// thread, so a task's cancel watcher can end a program that is already running. The Linux side exports the same name
/// and methods with the same behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Process {
    pid: u32,
}

impl Process {
    /// Finds the running process whose executable is `program`, if there is one. The path key is case-insensitive and
    /// uses backslashes, the same rule as `game_is_running`.
    pub fn by_program(program: &Path) -> Option<Self> {
        let key = windows_key(&program.to_string_lossy());
        let mut system = System::new();
        refresh(&mut system);
        system
            .processes()
            .iter()
            .find(|(_, process)| {
                process
                    .exe()
                    .is_some_and(|exe| windows_key(&exe.to_string_lossy()) == key)
            })
            .map(|(pid, _)| Self { pid: pid.as_u32() })
    }

    /// Whether the process still exists.
    pub fn exists(self) -> bool {
        let mut system = System::new();
        refresh(&mut system);
        system.process(Pid::from_u32(self.pid)).is_some()
    }

    /// Terminates the process through the process list. A process that is already gone is a success, so a Stop that
    /// arrives just after the program exited never reports a failure.
    pub fn stop(self) -> bool {
        let mut system = System::new();
        refresh(&mut system);
        system
            .process(Pid::from_u32(self.pid))
            .is_none_or(sysinfo::Process::kill)
    }
}

fn refresh(system: &mut System) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
}

/// Whether the executable lies in the game folder. The key is case-insensitive and uses backslashes.
fn in_game_folder(exe: &Path, folder: &str) -> bool {
    crate::domain::pathkey::is_inside(Path::new(folder), exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::seen_while_running;
    use crate::test_support::scratch;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn a_process_started_from_the_game_folder_is_seen_until_it_exits() {
        let base = scratch!("running");
        let game = base.join("Game");
        fs::create_dir_all(game.join("bin")).unwrap();
        // A copy of a system program stands in for the game: it runs for a while, and its path lies in the game folder.
        let exe = game.join("bin/game.exe");
        let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/ping.exe");
        fs::copy(ping, &exe).unwrap();

        let child = std::process::Command::new(&exe)
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();

        assert!(seen_while_running(&game, child));
        assert!(!game_is_running(&game));
    }

    #[test]
    fn a_process_started_from_the_game_folder_is_stopped() {
        let base = scratch!("stopped");
        let game = base.join("Game");
        fs::create_dir_all(game.join("bin")).unwrap();
        let exe = game.join("bin/game.exe");
        let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/ping.exe");
        fs::copy(ping, &exe).unwrap();

        let mut child = std::process::Command::new(&exe)
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(game_is_running(&game));
        assert!(stop_game(&game));
        assert!(!child.wait().unwrap().success());
        assert!(!game_is_running(&game));
    }

    #[test]
    fn a_process_handle_stops_a_running_program_and_a_gone_one_is_a_success() {
        let base = scratch!("process-stop");
        let helper = base.join("helper.exe");
        let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/ping.exe");
        fs::copy(ping, &helper).unwrap();

        let mut child = std::process::Command::new(&helper)
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();

        let process = Process::by_program(&helper).expect("the helper is running");
        assert!(process.exists());
        assert!(process.stop());
        assert!(!child.wait().unwrap().success());
        // A Stop that arrives after the program exited is still a success.
        assert!(process.stop());
    }
}
