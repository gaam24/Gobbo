use std::path::{Component, Path, PathBuf, Prefix};

use crate::domain::pathkey;

use super::entry::JournalOp;

/// The places one deployment may touch. A journal is data on disk that anyone can edit, so undo checks every entry
/// against these before it removes or writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRoots {
    /// The profile's state folder: the journal, the overlay layer and the profile's user-data layers.
    pub state: PathBuf,
    /// The deployment root, where `FileDeployed` and `DirCreated` put the game's paths.
    pub deployment: PathBuf,
    /// The game's order file, when the game has one. A redirect writes it in the profile state instead.
    pub settings: Option<PathBuf>,
}

impl JournalRoots {
    /// The roots of one deployment: the profile state, the deployment root and the game's order file.
    pub const fn new(state: PathBuf, deployment: PathBuf, settings: Option<PathBuf>) -> Self {
        Self {
            state,
            deployment,
            settings,
        }
    }

    /// The same game roots for another profile's state folder. The overlay layers and the settings redirects are per
    /// profile, so a make-room undo of another profile uses that profile's state.
    pub fn at_state(&self, state: PathBuf) -> Self {
        Self {
            state,
            deployment: self.deployment.clone(),
            settings: self.settings.clone(),
        }
    }

    /// The first path of `entry` that lies outside the roots, or `None` when every path is allowed. A path with `..`,
    /// a relative path, a drive or UNC prefix that escapes, a path in another game or settings folder, or a path in
    /// the profile state outside the overlay layer folders is refused.
    pub fn unsafe_path<'a>(&self, entry: &'a JournalOp) -> Option<&'a Path> {
        match entry {
            JournalOp::DirCreated { path } | JournalOp::FileDeployed { path } => {
                (!self.allows_deployed(path)).then_some(path)
            }
            JournalOp::SettingsSections { path, .. } => {
                (!self.allows_settings(path)).then_some(path)
            }
            JournalOp::LayerPrepared { mods, overwrite } => {
                if !is_layer_path(&self.state.join("mods"), mods) {
                    Some(mods)
                } else if !is_layer_path(&self.state.join("overwrite"), overwrite) {
                    Some(overwrite)
                } else {
                    None
                }
            }
        }
    }

    fn allows_deployed(&self, path: &Path) -> bool {
        safe_journal_path(path)
            && (pathkey::is_inside(&self.deployment, path)
                // An overlay step journals its outputs in the game session's upper layer, which is `overwrite/`.
                // The mod layer is Gobbo's too, so the same entry kinds may name it.
                || pathkey::is_inside(&self.state.join("overwrite"), path)
                || pathkey::is_inside(&self.state.join("mods"), path))
    }

    fn allows_settings(&self, path: &Path) -> bool {
        let Some(settings) = &self.settings else {
            return false;
        };

        if !safe_journal_path(path) {
            return false;
        }

        if is_same_path(settings, path) {
            return true;
        }
        // A redirect writes the order file in `<state>/userdata/<set>/upper/`; the copy store and the base folder are
        // not settings targets.
        let userdata = self.state.join("userdata");
        let Some(relative) = pathkey::relative_to(&userdata, path) else {
            return false;
        };
        let Some(Component::Normal(set)) = relative.components().next() else {
            return false;
        };
        pathkey::is_inside(&userdata.join(set).join("upper"), path)
    }
}

/// A path a journal may name: absolute, with no `.` or `..`, and with only a normal disk or UNC prefix on Windows.
/// A relative path would resolve against the process working directory, and `..` would escape a lexical root check.
fn safe_journal_path(path: &Path) -> bool {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return false;
    }

    path.components().all(|component| match component {
        Component::Normal(_) | Component::RootDir => true,
        Component::Prefix(prefix) => {
            matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _))
        }
        Component::CurDir | Component::ParentDir => false,
    })
}

/// Whether `path` is exactly `root`, following the platform's case rule.
fn is_same_path(root: &Path, path: &Path) -> bool {
    pathkey::relative_to(root, path).is_some_and(|relative| relative.as_os_str().is_empty())
}

/// A layer folder must be exactly the one Gobbo owns under the profile's state folder; no child may be named.
fn is_layer_path(root: &Path, path: &Path) -> bool {
    safe_journal_path(path) && is_same_path(root, path)
}
