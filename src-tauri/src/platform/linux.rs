//! Linux: the process list and the check whether a game runs, from the command lines in `/proc`. The Proton lookup,
//! which also differs by system, is in `linux/proton.rs`.

mod proton;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use super::mount_table::mounted_overlay_at;

pub use proton::{
    find_proton, installed_protons, proton_at, proton_named, steam_installs, steam_root,
};

/// Whether the platform can redirect a user-data folder instead of copying it around a launch.
pub const fn supports_user_data_redirect() -> bool {
    false
}

/// Linux has no UAC and no elevated token in the Windows sense.
pub const fn is_elevated() -> bool {
    false
}

/// Where the other stores keep their data on Linux: Heroic and Lutris, native and Flatpak, Legendary, minigalaxy and
/// the GOG folders in the home folder. Linux has no Epic launcher and no registry.
pub fn store_folders(home: &Path) -> super::StoreFolders {
    store_folders_from(
        home,
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("LEGENDARY_CONFIG_PATH"),
    )
}

/// `store_folders` with the environment given, so a test can check the rule without changing the process's variables.
fn store_folders_from(
    home: &Path,
    config: Option<std::ffi::OsString>,
    data: Option<std::ffi::OsString>,
    legendary: Option<std::ffi::OsString>,
) -> super::StoreFolders {
    let config_home = config
        .clone()
        .map_or_else(|| home.join(".config"), PathBuf::from);
    let data_home = data.map_or_else(|| home.join(".local/share"), PathBuf::from);
    let flatpak =
        |app: &str, kind: &str, store: &str| home.join(".var/app").join(app).join(kind).join(store);

    super::StoreFolders {
        epic_data: None,
        epic_list: None,
        gog_libraries: vec![home.join("GOG Games"), home.join("Games")],
        gog_registry: Vec::new(),
        minigalaxy: Some(config_home.join("minigalaxy/config.json")),
        heroic: vec![
            config_home.join("heroic"),
            flatpak("com.heroicgameslauncher.hgl", "config", "heroic"),
        ],
        legendary: Some(super::legendary_dir(home, legendary, config)),
        lutris: vec![
            super::LutrisFolders {
                config: config_home.join("lutris"),
                data: data_home.join("lutris"),
            },
            super::LutrisFolders {
                config: flatpak("net.lutris.Lutris", "config", "lutris"),
                data: flatpak("net.lutris.Lutris", "data", "lutris"),
            },
        ],
    }
}

/// Whether an overlay of the game folder `root` is still mounted in a namespace that has a process. The namespace lives on
/// as long as the program that started the overlay, or a process of its tree (Wine's services, for example), and the
/// writable layer stays locked meanwhile. So a session is seen here even after its launcher has exited. Each namespace
/// is read once.
pub fn overlay_in_use(root: &Path) -> bool {
    let Ok(processes) = fs::read_dir("/proc") else {
        return false;
    };
    let mut namespaces = HashSet::new();
    processes
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let namespace = fs::read_link(entry.path().join("ns/mnt")).ok()?;
            namespaces.insert(namespace).then_some(entry.path())
        })
        .filter_map(|process| fs::read_to_string(process.join("mountinfo")).ok())
        .any(|table| mounted_overlay_at(&table, root))
}

/// The processes of the game's session: every process that shares a mount namespace with an overlay of `root`. A process
/// of the session may name no game file in its command line (Wine's services and server do not), and it outlives its
/// launcher, so a stop ends the session by namespace, not by command line. Each namespace is read once.
pub fn overlay_session_pids(root: &Path) -> Vec<u32> {
    let Ok(processes) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let members: Vec<(u32, PathBuf)> = processes
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse().ok()?;
            let namespace = fs::read_link(entry.path().join("ns/mnt")).ok()?;
            Some((pid, namespace))
        })
        .collect();
    let mut overlaid: HashMap<&Path, bool> = HashMap::new();

    for (pid, namespace) in &members {
        overlaid.entry(namespace.as_path()).or_insert_with(|| {
            fs::read_to_string(format!("/proc/{pid}/mountinfo"))
                .is_ok_and(|table| mounted_overlay_at(&table, root))
        });
    }

    members
        .iter()
        .filter(|(_, namespace)| overlaid.get(namespace.as_path()) == Some(&true))
        .map(|(pid, _)| *pid)
        .collect()
}

/// Whether a process runs a game from this folder: a Proton process or the game's own file (a native launch).
/// Gobbo and Steam both start Proton games through `proton`, so the process list tells the state also after
/// Gobbo restarts. A Wine game can outlive its `proton` process, though, and then its command line names a Windows
/// path that `runs_game` does not match. The overlay of the folder is still mounted while such a game runs, so it counts too.
pub fn game_is_running(game_root: &Path) -> bool {
    let folder = game_root.display().to_string();
    let root = folder.trim_end_matches('/');
    // The launch chain of Gobbo names the game in its own command line (`gobbo run <game> -- <proton> <game file>`),
    // and so do `timeout` and Steam's launch reaper that start it. Those are launchers, not the game.
    let chain = launch_chain();
    let listed = fs::read_dir("/proc").is_ok_and(|processes| {
        processes
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .bytes()
                    .all(|byte| byte.is_ascii_digit())
            })
            .filter(|entry| !chain.contains(&entry.file_name()))
            // The poller calls this every few seconds, so the raw bytes go straight to `runs_game`
            // without building a `String` per argument per process.
            .filter_map(|entry| fs::read(entry.path().join("cmdline")).ok())
            .any(|raw| runs_game(&raw, root.as_bytes()))
    });
    listed || overlay_in_use(game_root)
}

/// This process and its parents, by pid, up to the first process below init. The parent is the second field after the
/// command name in `/proc/<pid>/stat`, which is in parentheses and may hold spaces, so the name is skipped by its last
/// closing parenthesis.
fn launch_chain() -> Vec<std::ffi::OsString> {
    let mut chain = Vec::new();
    let mut pid = std::process::id();

    for _ in 0..64 {
        chain.push(std::ffi::OsString::from(pid.to_string()));
        let parent = fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| {
                let (_, rest) = stat.rsplit_once(')')?;
                rest.split_whitespace().nth(1)?.parse::<u32>().ok()
            });
        match parent {
            Some(parent) if parent > 1 => pid = parent,
            _ => break,
        }
    }

    chain
}

/// Stops every process of the game in `game_root`, and every process below it. `SIGTERM` asks the game to save and
/// quit; a process still there after the grace period gets `SIGKILL`. The command line is read with the same rule as
/// `game_is_running`, so a Proton launch, a native game and a game in an overlay all count. The processes of the game's
/// overlay session count too, even when their command lines name no game file: Wine's services and server outlive the
/// game's own process, and they keep the overlay mounted. Returns whether at least one process was stopped.
pub fn stop_game(game_root: &Path) -> bool {
    let processes = game_processes(game_root);
    stop_processes(&processes, Duration::from_secs(2))
}

/// `stop_game` with an explicit grace period, so a test does not wait two seconds. `SIGTERM` for every process, a
/// bounded wait, then `SIGKILL` for the ones that are still alive.
fn stop_processes(processes: &[u32], grace: Duration) -> bool {
    let mut stopped = false;
    let mut alive: Vec<u32> = Vec::new();

    for &pid in processes {
        if pid == std::process::id() {
            continue;
        }

        if kill(Pid::from_raw(pid as i32), Signal::SIGTERM).is_ok() {
            stopped = true;
            alive.push(pid);
        }
    }

    let deadline = Instant::now() + grace;

    while !alive.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        alive.retain(|pid| kill(Pid::from_raw(*pid as i32), None::<Signal>).is_ok());
    }

    for pid in alive {
        if kill(Pid::from_raw(pid as i32), Signal::SIGKILL).is_ok() {
            stopped = true;
        }
    }

    stopped
}

/// A handle to a running process Gobbo started, identified by its pid. [`Self::stop`] can be called from another
/// thread, so a task's cancel watcher can end a program that is already running. The Windows side exports the same name
/// and methods with the same behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Process {
    pid: u32,
}

impl Process {
    /// Finds the running process that executes `program`, if there is one. The program is the command itself (index 0),
    /// or the script an interpreter runs (index 1). A launcher such as `unshare` or Proton names it only as a later
    /// argument, so it is deliberately ignored: stopping the launcher would leave the tool running. The path is
    /// compared as raw bytes, because `/proc` may hold paths that are not valid UTF-8.
    pub fn by_program(program: &Path) -> Option<Self> {
        let needle = program.as_os_str().as_bytes();
        let processes = fs::read_dir("/proc").ok()?;
        let mut best: Option<(u32, usize)> = None;

        for entry in processes.filter_map(Result::ok) {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(raw) = fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let position = raw
                .split(|byte| *byte == 0)
                .position(|arg| arg == needle)
                .filter(|position| *position <= 1);
            if let Some(position) = position
                && best.is_none_or(|(_, best_position)| position < best_position)
            {
                best = Some((pid, position));
            }
        }

        best.map(|(pid, _)| Self { pid })
    }

    /// Whether the process still exists. A process that has exited but has not been reaped yet still answers here; the
    /// thread that waits for it reaps it promptly.
    pub fn exists(self) -> bool {
        kill(Pid::from_raw(self.pid as i32), None).is_ok()
    }

    /// Stops the process: `SIGTERM` first, then `SIGKILL` if it is still there after a short grace. A process that is
    /// already gone is a success, so a Stop that arrives just after the program exited never reports a failure.
    ///
    /// Both signals are sent through the same checked primitive as [`stop_game`], so this stays safe code.
    pub fn stop(self) -> bool {
        if !self.exists() {
            return true;
        }
        // The process can exit between the check and the signal; in that case nothing was left to stop and it is still
        // a success.
        stop_processes(&[self.pid], Duration::from_millis(200)) || !self.exists()
    }
}

/// The game's processes and their descendants, children first. A process belongs to the game when its command line
/// runs a file of the folder, exactly as `game_is_running` decides it, or when it runs in the game's overlay session. A
/// parent that cannot be read is left out of the tree; the process itself is still stopped when it belongs to the game.
fn game_processes(game_root: &Path) -> Vec<u32> {
    let folder = game_root.display().to_string();
    let root = folder.trim_end_matches('/');
    let Ok(processes) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut parents = HashMap::new();
    let mut roots = Vec::new();
    // Gobbo and its parents may name the game folder too; they launch the game and are never stopped with it.
    let chain = launch_chain();

    for entry in processes.filter_map(std::result::Result::ok) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let path = entry.path();

        if let Some(parent) = parent_of(&path) {
            parents.insert(pid, parent);
        }

        if !chain.contains(&entry.file_name())
            && fs::read(path.join("cmdline")).is_ok_and(|raw| runs_game(&raw, root.as_bytes()))
        {
            roots.push(pid);
        }
    }

    roots.extend(overlay_session_pids(game_root));
    super::descendants(roots, &parents)
}

/// The parent of a process from `/proc/<pid>/status`. The `comm` field of `/proc/<pid>/stat` may hold spaces and
/// parentheses, so the status file is simpler to read.
fn parent_of(process: &Path) -> Option<u32> {
    let status = fs::read_to_string(process.join("status")).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:")?.trim().parse().ok())
}

/// Whether the command line (`/proc/<pid>/cmdline`: the arguments, each ended by NUL) runs a file of the game folder.
/// A Proton launch counts when some argument is the `proton` file and some argument (possibly another one) lies inside
/// the game folder. A native game counts when its own file is the first argument, or when the command is Gobbo's
/// overlay launcher for the folder (`mm-overlay`), which runs before the game's own process exists. The folder must
/// match a whole path, so `Game Mods` is not `Game`. The bytes are compared directly: paths from `/proc` may not be
/// valid UTF-8.
fn runs_game(cmdline: &[u8], root: &[u8]) -> bool {
    let starts_inside = |arg: &[u8]| {
        arg.strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.first() == Some(&b'/'))
    };
    let is_proton = |arg: &[u8]| {
        Path::new(String::from_utf8_lossy(arg).as_ref())
            .file_name()
            .is_some_and(|name| name == "proton")
    };
    let mut proton = false;
    let mut overlay = false;
    let mut inside = false;

    for arg in cmdline
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
    {
        proton = proton || is_proton(arg);
        overlay = overlay || arg == b"mm-overlay";
        inside = inside || starts_inside(arg);
    }

    if inside && (proton || overlay) {
        return true;
    }
    // A native game runs without Proton: its own file is the first thing on the command line.
    cmdline
        .split(|byte| *byte == 0)
        .find(|arg| !arg.is_empty())
        .is_some_and(|arg| {
            arg.strip_prefix(root)
                .is_some_and(|rest| rest.first() == Some(&b'/'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::seen_while_running;
    use crate::test_support::scratch;

    #[test]
    fn the_store_folders_follow_xdg_and_include_the_flatpak_installs() {
        let home = Path::new("/home/slot");
        let folders = store_folders_from(home, None, None, None);
        assert_eq!(
            folders.heroic,
            [
                home.join(".config/heroic"),
                home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"),
            ]
        );
        assert_eq!(folders.lutris[0].config, home.join(".config/lutris"));
        assert_eq!(folders.lutris[0].data, home.join(".local/share/lutris"));
        assert_eq!(
            folders.lutris[1].data,
            home.join(".var/app/net.lutris.Lutris/data/lutris")
        );
        assert_eq!(folders.legendary, Some(home.join(".config/legendary")));
        assert_eq!(
            folders.minigalaxy,
            Some(home.join(".config/minigalaxy/config.json"))
        );
        assert_eq!(folders.epic_data, None);

        let custom = store_folders_from(
            home,
            Some("/custom/config".into()),
            Some("/custom/data".into()),
            None,
        );
        assert_eq!(custom.heroic[0], Path::new("/custom/config/heroic"));
        assert_eq!(custom.lutris[0].data, Path::new("/custom/data/lutris"));
        assert_eq!(
            custom.legendary,
            Some(PathBuf::from("/custom/config/legendary"))
        );
    }

    /// The command line the way `/proc/<pid>/cmdline` stores it: arguments separated by NUL.
    fn command(parts: &[&str]) -> Vec<u8> {
        let mut raw = parts.join("\u{0}").into_bytes();
        raw.push(0);
        raw
    }

    #[test]
    fn a_proton_process_on_a_file_of_the_game_means_it_runs() {
        let steam = command(&[
            "reaper",
            "SteamLaunch",
            "--",
            "/steam/proton",
            "waitforexitandrun",
            "/games/Game/bin/game.exe",
        ]);
        assert!(runs_game(&steam, b"/games/Game"));
    }

    #[test]
    fn a_folder_that_only_starts_with_the_same_name_is_another_game() {
        let other = command(&[
            "/steam/proton",
            "waitforexitandrun",
            "/games/Game Mods/bin/tool.exe",
        ]);
        assert!(!runs_game(&other, b"/games/Game"));
    }

    #[test]
    fn opening_the_game_folder_is_not_a_running_game() {
        let opened = command(&["xdg-open", "/games/Game"]);
        assert!(!runs_game(&opened, b"/games/Game"));
    }

    #[test]
    fn a_native_process_running_the_game_file_means_it_runs() {
        let native = command(&["/games/Game/bin/x64/game.exe", "-windowed"]);
        assert!(runs_game(&native, b"/games/Game"));
    }

    #[test]
    fn an_overlay_launcher_for_the_game_means_it_runs() {
        let overlay = command(&[
            "unshare",
            "--user",
            "--mount",
            "--",
            "sh",
            "-c",
            "mount -t overlay overlay -o \"$1\" \"$2\" || exit 1; shift 2; exec \"$@\"",
            "mm-overlay",
            "lowerdir=/mods:/games/Game",
            "/games/Game",
            "/games/Game/bin/x64/game",
        ]);
        assert!(runs_game(&overlay, b"/games/Game"));
    }

    #[test]
    fn a_file_of_the_game_passed_to_another_program_is_not_a_running_game() {
        // A native game's own file is the first thing on the command line; a file given to another program is not.
        let copying = command(&["cp", "/tmp/a.ws", "/games/Game/bin/x64/game.exe"]);
        assert!(!runs_game(&copying, b"/games/Game"));
    }

    /// Starts a test program, retrying the transient `ETXTBSY` a copied or just-written script can report while the
    /// kernel still has the writer open. Linux only; the test suite runs these in parallel.
    fn spawn_retrying(command: &mut std::process::Command) -> std::process::Child {
        for _ in 0..50 {
            match command.spawn() {
                Ok(child) => return child,
                Err(error) if error.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("could not start the test program: {error}"),
            }
        }

        panic!("the test program stayed busy");
    }

    #[cfg(unix)]
    #[test]
    fn a_game_and_the_processes_below_it_are_stopped() {
        let base = scratch!("stop");
        let game = base.join("Game");
        fs::create_dir_all(game.join("bin")).unwrap();
        // A shell stands in for the game; the sleep below it stands for a Wine service that has no game path in its
        // command line. The shell is copied, so the first argument of its command line is inside the game folder.
        let shell = game.join("bin/game");
        fs::copy("/bin/sh", &shell).unwrap();

        let mut child =
            spawn_retrying(std::process::Command::new(&shell).args(["-c", "sleep 30 & wait"]));
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(game_is_running(&game));

        assert!(stop_game(&game));
        assert!(!child.wait().unwrap().success());
        assert!(!game_is_running(&game));
    }

    #[test]
    fn nothing_is_stopped_when_the_game_does_not_run() {
        let base = scratch!("stop-none");
        let game = base.join("Game");
        fs::create_dir_all(&game).unwrap();

        assert!(!stop_game(&game));
    }

    #[cfg(unix)]
    #[test]
    fn a_proton_process_started_for_the_game_is_seen_until_it_exits() {
        use std::os::unix::fs::PermissionsExt;
        let base = scratch!("running");
        let game = base.join("Game");
        fs::create_dir_all(game.join("bin")).unwrap();
        let script = base.join("proton");
        fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let child = spawn_retrying(
            std::process::Command::new(&script)
                .arg("waitforexitandrun")
                .arg(game.join("bin/game.exe")),
        );

        assert!(seen_while_running(&game, child));
        assert!(!game_is_running(&game));
    }

    #[test]
    fn a_process_that_ignores_sigterm_is_killed_after_the_grace() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap '' TERM; sleep 10")
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(stop_processes(&[pid], Duration::from_millis(250)));
        assert!(!child.wait().unwrap().success());
    }

    #[test]
    fn the_launch_chain_starts_with_this_process_and_goes_up_to_its_parents() {
        let chain = launch_chain();
        assert_eq!(
            chain.first(),
            Some(&std::ffi::OsString::from(std::process::id().to_string()))
        );
        // The test runner is started by another process, so the chain has a parent too.
        assert!(chain.len() >= 2);
    }

    #[cfg(unix)]
    #[test]
    fn a_process_handle_stops_a_running_program_and_a_gone_one_is_a_success() {
        use std::os::unix::fs::PermissionsExt;

        let base = scratch!("process-stop");
        let helper = base.join("helper");
        fs::write(&helper, "#!/bin/sh\nsleep 30\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let mut child = spawn_retrying(&mut std::process::Command::new(&helper));

        let process = Process::by_program(&helper).expect("the helper is running");
        assert!(process.exists());
        assert!(process.stop());
        assert!(!child.wait().unwrap().success());
        assert!(!process.exists());
        // A Stop that arrives after the program exited is still a success.
        assert!(process.stop());
    }
}
