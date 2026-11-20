//! The deployment rules of a game definition: where a mod may put its files and which methods the game allows. These
//! are pure values and questions; `games` parses and validates a `game.toml` into them, and `domain::resolver` uses
//! them to build a plan without depending on the definition loader.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::domain::pathkey::path_key;

/// The deployment rules of one game.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deploy {
    /// The folder that holds the slots and mirrors. `{game}` by default. A game may keep mods outside its folder, for
    /// example `{appdata}/MyGame`; the overlay covers that root too. The user can override the resolved path.
    #[serde(default)]
    pub root: Option<String>,
    /// The root of a native Linux install, which has no Proton prefix. Falls back to `root`.
    #[serde(default)]
    pub root_linux: Option<String>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub mirror: Vec<Mirror>,
    /// Bare file names that keep their name in the folder that holds the slots and mirrors, for example a loader's
    /// proxy DLL such as `winhttp.dll`. A bare file of an archive with one of these names goes there, before any slot
    /// takes it. With overlay the file stays in the mod layer.
    #[serde(default)]
    pub root_files: Vec<String>,
    /// The game resolves loose files by resource name: two files with the same file name conflict even when they sit in
    /// different folders. Set this only for a game with one global override tree; a game that keeps every mod in its
    /// own folder must leave it off.
    #[serde(default)]
    pub resource_names: bool,
    /// Kinds of file in mod folders that the game reads from one mod only (see `Shared`).
    #[serde(default)]
    pub shared: Vec<Shared>,
    /// Game folders that an archive may place files into only through a mirror. An archive with other files in them is
    /// refused, so a stray file cannot silently take over a folder the game owns.
    #[serde(default)]
    pub protected: Vec<String>,
    /// Folder names that mods use at the archive root, mapped to the game folder they stand for. A mod packed as
    /// `plugins/<Mod>/<Mod>.dll` for a game whose slot is `BepInEx/plugins` goes there with `plugins = "BepInEx/plugins"`.
    /// The key is one folder name (case is ignored); the target is a folder inside a slot or mirror. Without an alias the
    /// archive is read as it is.
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// Whether every archive path is a game path. The whole deployment root is then a mirror: a file the game already
    /// has is covered by the layer, and the game folder stays untouched. Bare files at the archive root still need
    /// `root_files` or a move rule.
    #[serde(default)]
    pub full_root: bool,
    /// Bare files at the archive root with one of these extensions go to `to`, keeping their file name. This is the
    /// MO2-style rule for a game that loads loose archives from one folder, for example `.archive` and `.xl` on
    /// Cyberpunk 2077.
    #[serde(default, rename = "move")]
    pub moves: Vec<Move>,
}

impl Deploy {
    /// The slot that a library index names by its folder.
    pub fn slot(&self, dir: &str) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.dir == dir)
    }

    /// The mirrored folder that holds a game path, for example `DLC` for `DLC/dlcfoo/x.bundle`. Case is ignored, and a
    /// mirror that names a file holds only that file.
    pub fn mirror_of(&self, rel: &str) -> Option<&Mirror> {
        self.mirror
            .iter()
            .find(|mirror| is_inside(rel, &mirror.dir))
    }

    /// The root file that `rel` is: the name as the definition spells it, when `rel` is a bare name (no folder) that
    /// compares equal to one of `root_files`. Names compare like paths do.
    pub fn root_file_of(&self, rel: &str) -> Option<&str> {
        if rel.contains(['/', '\\']) || self.root_files.is_empty() {
            return None;
        }
        let key = path_key(rel);
        self.root_files
            .iter()
            .find(|file| path_key(file) == key)
            .map(String::as_str)
    }

    /// The DLL files the definition names for the game: the root files that are DLLs, and the mirrored single files that
    /// are DLLs, such as `bin/x64/version.dll` (a loader next to the program). Under Proton such a file is loaded native
    /// when it is there (see `games::proxy::native_dlls`).
    pub fn proxy_dlls(&self) -> Vec<String> {
        let roots = self.root_files.iter().map(String::as_str);
        let mirrored = self
            .mirror
            .iter()
            .map(|mirror| mirror.dir.as_str())
            .map(|dir| dir.rsplit('/').next().unwrap_or(dir));
        roots
            .chain(mirrored)
            .filter(|name| name.to_ascii_lowercase().ends_with(".dll"))
            .map(str::to_string)
            .collect()
    }

    /// Whether a file of a mod keeps its game path: every file does with `full_root`, else it lies under a mirror or is
    /// one of the root files.
    pub fn keeps_game_path(&self, rel: &str) -> bool {
        if self.full_root {
            return true;
        }
        if self.mirror.is_empty() && self.root_files.is_empty() {
            return false;
        }
        self.mirror_of(rel).is_some() || self.root_file_of(rel).is_some()
    }

    /// Whether a non-empty list keeps every game path, so the mod has no folder of its own in a slot. An empty list is
    /// not mirrored-only: a mod with no files still gets a folder.
    pub fn mirrored_only(&self, files: &[String]) -> bool {
        if self.full_root {
            return !files.is_empty();
        }
        !files.is_empty() && files.iter().all(|rel| self.keeps_game_path(rel))
    }

    /// The game path of a bare archive file that a move rule takes, for example `archive/pc/mod/foo.archive`.
    pub fn move_destination(&self, rel: &str) -> Option<String> {
        if rel.contains(['/', '\\']) {
            return None;
        }
        self.moves
            .iter()
            .find(|rule| {
                rule.extensions
                    .iter()
                    .any(|extension| has_extension(rel, extension))
            })
            .map(|rule| format!("{}/{}", rule.to.trim_end_matches('/'), rel))
    }

    /// Whether some files reach the same place in the game, so the user can choose which mod's copy wins.
    pub fn shares_files(&self) -> bool {
        self.full_root
            || !self.mirror.is_empty()
            || !self.moves.is_empty()
            || !self.root_files.is_empty()
            || self.resource_names
            || !self.shared.is_empty()
            || self.slots.iter().any(|slot| slot.flat)
    }
}

/// Whether `name` ends with `extension`, ignoring ASCII case. The definition stores extensions lowercase with the dot.
fn has_extension(name: &str, extension: &str) -> bool {
    name.len() > extension.len()
        && name
            .get(name.len() - extension.len()..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(extension))
}

/// A folder where each mod gets a folder of its own, for example `Mods/<mod>/`. The game loads every folder itself.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    /// Relative to the deployment root. It also names the slot in the library index.
    pub dir: String,
    /// The start of the mod folder names in the game, for example `mod`.
    #[serde(default)]
    pub prefix: String,
    /// A file or folder that marks a mod root in an archive, for example `ModInfo.xml` or `content`. A marker at the
    /// archive root makes the whole archive one mod, named after the archive.
    #[serde(default)]
    pub marker: Option<String>,
    /// Only these files go into the slot (lowercase, with the dot). Empty means all files.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Files at the archive root with these extensions go into the slot too, for example a bare `.dll`.
    #[serde(default)]
    pub root_extensions: Vec<String>,
    /// A folder at the archive root that holds a file with one of these extensions is a mod: `<Plugin>/<plugin>.dll`
    /// goes into the slot as `<dir>/<Plugin>/...`. A folder named as a game folder is never one. In a flat slot the
    /// folder names the mod in the library, but its files deploy directly into `dir`.
    #[serde(default)]
    pub root_folder_extensions: Vec<String>,
    /// The files go directly into `dir`, not into a folder per mod. For games that load loose `.pak` files.
    #[serde(default)]
    pub flat: bool,
}

/// A game folder (or file) whose files keep their game paths, for example `archive/pc/mod` or `DLC`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mirror {
    /// Relative to the deployment root.
    pub dir: String,
    /// Only these files are taken (lowercase, with the dot). Empty means all files.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// A folder directly under `dir` gets this prefix when it lacks it, for example `DLC/Corvo` becomes `DLC/dlcCorvo`.
    #[serde(default)]
    pub prefix: String,
}

/// A bare file at the archive root that the game reads from one folder, for example a `.archive` for
/// `archive/pc/mod`. The file keeps its name; only the folder changes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Move {
    /// Lowercase, with the dot.
    pub extensions: Vec<String>,
    /// Relative to the deployment root, with `/`.
    pub to: String,
}

/// A kind of file that the game reads from one mod only, although each mod keeps it in its own folder, for example
/// The Witcher 3's scripts. Two mods with the same path conflict, and only the winner's copy is deployed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shared {
    /// Folder inside the mod, for example `content`. Empty means anywhere in the mod. Case is ignored.
    #[serde(default)]
    pub dir: String,
    /// Lowercase, with the dot, for example `.ws`. Empty means every file in `dir`.
    #[serde(default)]
    pub extensions: Vec<String>,
}

/// Whether `rel` is `dir` or lies inside it. Both use `/`; case is ignored.
fn is_inside(rel: &str, dir: &str) -> bool {
    // The first byte rejects almost every file of a mod before the whole prefix is compared.
    if let Some(first) = dir.as_bytes().first()
        && !rel
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.eq_ignore_ascii_case(first))
    {
        return false;
    }
    rel.get(..dir.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(dir))
        && matches!(rel.as_bytes().get(dir.len()), None | Some(b'/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deploy() -> Deploy {
        Deploy {
            root: None,
            root_linux: None,
            slots: vec![Slot {
                dir: "Mods".into(),
                prefix: "mod".into(),
                marker: None,
                extensions: Vec::new(),
                root_extensions: Vec::new(),
                root_folder_extensions: Vec::new(),
                flat: false,
            }],
            mirror: vec![Mirror {
                dir: "DLC".into(),
                extensions: Vec::new(),
                prefix: "dlc".into(),
            }],
            root_files: vec!["winhttp.dll".into()],
            resource_names: false,
            shared: Vec::new(),
            protected: Vec::new(),
            aliases: BTreeMap::new(),
            full_root: false,
            moves: Vec::new(),
        }
    }

    #[test]
    fn a_mirror_matches_its_folder_and_not_a_name_that_only_starts_like_it() {
        let deploy = deploy();
        assert!(deploy.mirror_of("DLC/dlcfoo/x.bundle").is_some());
        assert!(deploy.mirror_of("dlc").is_some());
        assert!(deploy.mirror_of("DLCX/x.bundle").is_none());
        assert!(deploy.mirror_of("DL").is_none());
    }

    #[test]
    fn a_mirrored_or_root_file_keeps_its_game_path() {
        let deploy = deploy();
        assert!(deploy.keeps_game_path("DLC/dlcfoo/x.bundle"));
        assert!(deploy.keeps_game_path("winhttp.dll"));
        assert!(!deploy.keeps_game_path("content/x.ws"));
    }

    #[test]
    fn a_list_is_mirrored_only_when_every_file_keeps_its_game_path() {
        let deploy = deploy();
        let files = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_string()).collect()
        };

        assert!(deploy.mirrored_only(&files(&["DLC/dlcfoo/x.bundle", "winhttp.dll"])));
        assert!(!deploy.mirrored_only(&files(&["DLC/dlcfoo/x.bundle", "content/x.ws"])));
        assert!(!deploy.mirrored_only(&files(&[])));
        assert!(!Deploy::default().mirrored_only(&files(&["DLC/dlcfoo/x.bundle"])));
    }
}
