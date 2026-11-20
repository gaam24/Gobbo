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

#[cfg(all(not(windows), test))]
pub use linux::overlay_session_pids;
#[cfg(not(windows))]
pub use linux::{
    Process, find_proton, game_is_running, installed_protons, overlay_in_use, proton_at,
    proton_named, steam_installs, steam_root, stop_game, supports_user_data_redirect,
};
#[cfg(windows)]
pub use windows::{
    Process, game_is_running, installed_protons, steam_installs, stop_game,
    supports_user_data_redirect,
};

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
