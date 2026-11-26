//! User data a game keeps outside its folder: saves, settings, configuration. A definition names one or more sets;
//! a profile decides whether a set is shared with the other profiles or isolated. The set carries only resolved paths,
//! so both the deployment and the overlay can use it without knowing which game it comes from.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::pathkey;

/// The kind of data a set holds. The frontend translates this value once for every game; a definition never carries
/// user-visible text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A folder with several kinds of data, for example saves and settings together.
    #[default]
    UserData,
    Settings,
    Saves,
    Config,
    Mods,
}

/// How Gobbo keeps an isolated set apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The overlay hook (or overlayfs) serves the set from the profile's upper layer. No copy is made.
    #[default]
    Redirect,
    /// The files are copied in before a launch and copied back after it. Used when the platform cannot redirect.
    Copy,
}

/// Whether a profile shares a set with the other profiles or keeps its own copy/layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    /// The real folder is used by every profile.
    #[default]
    Shared,
    /// The profile sees its own upper layer (redirect) or its own copy (copy mode).
    Isolated,
}

impl Isolation {
    /// Whether this choice isolates the profile.
    pub const fn is_isolated(self) -> bool {
        matches!(self, Self::Isolated)
    }
}

/// One resolved user-data set for one profile. The service fills the paths; the deployment and the overlay only read
/// them. `real` is the folder the game knows, `upper` the profile's writable layer and `store` the copy used when
/// the platform cannot redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    /// The stable id from the definition. It is not user-visible text.
    pub id: String,
    /// The universal kind the frontend translates.
    pub kind: Kind,
    /// The folder the game sees, already resolved from its template.
    pub real: PathBuf,
    /// The effective mode for the next launch or deployment.
    pub mode: Mode,
    /// Whether this profile keeps the set apart.
    pub isolated: bool,
    /// Whether an isolated redirect also sees the existing real folder as a read-only base.
    pub inherit: bool,
    /// The read-only lower layer of a redirect: `real` when inherit is on, an empty folder otherwise.
    pub base: PathBuf,
    /// The profile's writable layer for a redirect.
    pub upper: PathBuf,
    /// The profile's copy of the set for copy mode.
    pub store: PathBuf,
}

impl Set {
    /// Whether the set is an isolated redirect, so the caller must write to `upper` instead of `real`.
    pub const fn is_redirected(&self) -> bool {
        self.isolated && matches!(self.mode, Mode::Redirect)
    }

    /// The path of `path` inside the set, when `path` is the set folder or lies below it. The result keeps the spelling
    /// of `path`; the comparison follows the platform.
    pub fn relative(&self, path: &Path) -> Option<PathBuf> {
        pathkey::relative_to(&self.real, path)
    }

    /// Where Gobbo writes a file of this set. A redirect uses the profile's upper layer; a shared set or a copy set
    /// keeps the real path, because the copy is made around the launch.
    pub fn write_path(&self, path: &Path) -> Option<PathBuf> {
        let relative = self.relative(path)?;
        Some(if self.is_redirected() {
            self.upper.join(relative)
        } else {
            path.to_path_buf()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(real: &str) -> Set {
        Set {
            id: "documents".into(),
            kind: Kind::UserData,
            real: PathBuf::from(real),
            mode: Mode::Redirect,
            isolated: true,
            inherit: true,
            base: PathBuf::from("/documents/game"),
            upper: PathBuf::from("/state/upper"),
            store: PathBuf::from("/state/copy"),
        }
    }

    #[test]
    fn the_set_folder_and_its_children_are_inside() {
        let set = set("/documents/game");
        assert_eq!(
            set.relative(Path::new("/documents/game")),
            Some(PathBuf::new())
        );
        assert_eq!(
            set.relative(Path::new("/documents/game/saves/a.sav")),
            Some(PathBuf::from("saves/a.sav"))
        );
        assert_eq!(set.relative(Path::new("/documents/other/a.sav")), None);
    }

    #[test]
    fn a_redirect_writes_to_the_upper_layer_and_a_copy_stays_real() {
        let mut set = set("/documents/game");
        assert_eq!(
            set.write_path(Path::new("/documents/game/saves/a.sav")),
            Some(PathBuf::from("/state/upper/saves/a.sav"))
        );
        set.mode = Mode::Copy;
        assert_eq!(
            set.write_path(Path::new("/documents/game/saves/a.sav")),
            Some(PathBuf::from("/documents/game/saves/a.sav"))
        );
        set.isolated = false;
        assert_eq!(
            set.write_path(Path::new("/documents/game/saves/a.sav")),
            Some(PathBuf::from("/documents/game/saves/a.sav"))
        );
    }
}
