//! Where mods go in a game. Everything here answers a question about a definition that has already been parsed and
//! validated.

use std::path::Path;

use super::definition::GameDef;

/// The template that means "the game folder". Used when a definition has no separate mod root.
pub const GAME_TEMPLATE: &str = "{game}";

impl GameDef {
    /// Whether the game reads an order file that Gobbo writes. Then Gobbo needs the folder of that file.
    pub const fn has_load_order(&self) -> bool {
        self.load_order.is_some()
    }

    /// Places in the game that the UI shows: the slot folders and the mirrored folders.
    pub fn places(&self) -> Vec<String> {
        let slots = self.deploy.slots.iter().map(|slot| slot.dir.clone());
        slots
            .chain(self.deploy.mirror.iter().map(|mirror| mirror.dir.clone()))
            .collect()
    }

    /// The slot of a library folder found by a rescan: the first slot whose prefix the folder name has, else the first
    /// slot. Empty for a game without slots, whose mods only mirror game paths.
    pub fn slot_for_folder(&self, folder: &str) -> String {
        let lower = folder.to_ascii_lowercase();
        let slots = &self.deploy.slots;
        slots
            .iter()
            .find(|slot| {
                !slot.prefix.is_empty() && lower.starts_with(&slot.prefix.to_ascii_lowercase())
            })
            .or_else(|| slots.first())
            .map(|slot| slot.dir.clone())
            .unwrap_or_default()
    }

    /// Whether the deployment root is outside the game folder, for example `{appdata}/Game`. The overlay covers such a
    /// root too; the flag still says the mods live in a separate folder the user may need to point at.
    pub fn has_external_root(&self) -> bool {
        [
            self.deploy.root.as_deref(),
            self.deploy.root_linux.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .any(|root| !(root == GAME_TEMPLATE || root.starts_with("{game}/")))
    }

    /// The deployment root template for this platform: `root_linux` on a native Linux install, otherwise `root`.
    /// Falls back to `{game}`.
    pub fn root_template_for(&self, native: bool) -> &str {
        let linux = native
            .then_some(self.deploy.root_linux.as_deref())
            .flatten();
        linux
            .or(self.deploy.root.as_deref())
            .unwrap_or(GAME_TEMPLATE)
    }

    /// The first required file that neither the game folder nor the deployment plan provides. `placed` tells whether the
    /// plan places a file at a path key (see `domain::pathkey`). A mod provides a requirement through a mirror; Gobbo
    /// never copies a required file into the game folder to meet one.
    pub fn unmet_requirement<'a>(
        &'a self,
        game_root: &Path,
        placed: impl Fn(&str) -> bool,
    ) -> Option<&'a str> {
        self.requires
            .iter()
            .find(|file| {
                !game_root.join(file).exists() && !placed(&crate::domain::pathkey::path_key(file))
            })
            .map(String::as_str)
    }

    /// The executable started when the user does not choose one.
    pub fn default_executable(&self) -> Option<&super::Executable> {
        self.executables.first()
    }
}

pub use crate::domain::naming::is_game_relative;
