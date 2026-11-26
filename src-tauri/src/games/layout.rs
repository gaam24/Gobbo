//! Where mods go in a game. Everything here answers a question about a definition that has already been parsed and
//! validated.

use std::collections::BTreeSet;
use std::path::Path;

use super::definition::{GameDef, LoadOrder, LoaderPackage};
use crate::domain::pathkey::path_key;
use crate::domain::resolver::DeployPlan;

/// The template that means "the game folder". Used when a definition has no separate mod root.
pub const GAME_TEMPLATE: &str = "{game}";

/// The template for this install: `linux` when a native Linux build uses it and it is not empty, else `base`. A Windows
/// or Proton install uses `base`, because its paths name the Windows build.
pub(super) fn prefer_linux<'a>(base: &'a str, linux: Option<&'a str>, native: bool) -> &'a str {
    native
        .then_some(linux)
        .flatten()
        .filter(|template| !template.trim().is_empty())
        .unwrap_or(base)
}

impl GameDef {
    /// The order file in the settings folder the user chooses, when the game has one. An order file inside the
    /// deployment root (UE4SS's `mods.txt`) is not one of them.
    pub fn settings_order(&self) -> Option<&LoadOrder> {
        self.load_order
            .as_ref()
            .filter(|order| order.uses_settings_folder())
    }

    /// Whether that order file lives in the settings folder the user chooses. Then Gobbo needs that folder. An order
    /// file inside the deployment root (UE4SS's `mods.txt`) needs no folder of its own.
    pub fn has_settings_file(&self) -> bool {
        self.settings_order().is_some()
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
        prefer_linux(
            self.deploy.root.as_deref().unwrap_or(GAME_TEMPLATE),
            self.deploy.root_linux.as_deref(),
            native,
        )
    }

    /// The first required file that neither the game folder nor the deployment plan provides. `placed` holds the path
    /// keys the plan places (see `domain::pathkey`). A mod provides a requirement through a mirror; Gobbo never copies a
    /// required file into the game folder to meet one.
    pub fn unmet_requirement<'a>(
        &'a self,
        game_root: &Path,
        placed: &BTreeSet<String>,
    ) -> Option<&'a str> {
        self.requires
            .iter()
            .find(|file| !game_root.join(file).exists() && !placed.contains(&path_key(file)))
            .map(String::as_str)
    }

    /// The loader package when the plan needs it and it is not installed. It is needed once the plan puts a file under
    /// one of its `needed_by` folders, or any file when it names none; it is installed when every check file is under
    /// `deploy_root` or placed by a mod.
    pub fn missing_loader(
        &self,
        deploy_root: &Path,
        plan: &DeployPlan,
        placed: &BTreeSet<String>,
    ) -> Option<&LoaderPackage> {
        let install = self.install.as_ref()?;
        let needed = if install.needed_by.is_empty() {
            plan.file_count > 0
        } else {
            install
                .needed_by
                .iter()
                .any(|dir| plan.places_under(dir, placed))
        };
        let installed = install
            .check
            .iter()
            .all(|file| deploy_root.join(file).exists() || placed.contains(&path_key(file)));

        (needed && !installed).then_some(install)
    }

    /// Whether an archive is the game's loader package itself, such as the loader's release zip dropped onto the window
    /// instead of a mod. The programs among the check files (`.dll`, `.exe`) identify it: the archive holds a file with
    /// each of their names. A configuration file may be missing from a release, so it does not count. Names are
    /// compared without case, and wherever they sit, so a repack that wraps the release in more folders is found too.
    pub fn is_loader_archive<'a>(&self, entries: impl IntoIterator<Item = &'a str>) -> bool {
        let Some(install) = &self.install else {
            return false;
        };
        let file_name = |path: &str| {
            path.rsplit(['/', '\\'])
                .next()
                .unwrap_or(path)
                .to_ascii_lowercase()
        };
        let wanted: BTreeSet<String> = install
            .check
            .iter()
            .map(|check| file_name(check))
            .filter(|name| {
                Path::new(name).extension().is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("dll") || extension.eq_ignore_ascii_case("exe")
                })
            })
            .collect();

        if wanted.is_empty() {
            return false;
        }

        let present: BTreeSet<String> = entries.into_iter().map(file_name).collect();
        wanted.is_subset(&present)
    }

    /// The executable started when the user does not choose one.
    pub fn default_executable(&self) -> Option<&super::Executable> {
        self.executables.first()
    }
}

pub use crate::domain::naming::is_game_relative;
