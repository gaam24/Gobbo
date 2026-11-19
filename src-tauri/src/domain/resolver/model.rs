//! The types of a deployment plan: what goes in (the mods of a profile) and what comes out (the files, the settings
//! rows, the conflicts and the problems).

use std::borrow::Cow;
use std::sync::Arc;

use crate::error::Message;

/// A mod version from the profile. The order of the mods passed in is the priority order.
///
/// The names borrow the library and the profile, so building the inputs of a status does not clone four strings per mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInput<'a> {
    /// Id of the family. Conflicts name families, because every version of a mod is the same mod.
    pub id: Cow<'a, str>,
    /// Version folder in the library.
    pub folder: Cow<'a, str>,
    /// The slot (its folder) the mod belongs to. Empty for a mod that only mirrors game paths.
    pub slot: Cow<'a, str>,
    /// The name shown to the user; the base of the game folder name.
    pub display_name: Cow<'a, str>,
    pub enabled: bool,
    /// The priority the user set. Only a mod with one gets a row in the game's order file.
    pub priority: Option<u32>,
    /// File paths relative to the library folder, sorted. Files under a mirrored folder are game paths.
    pub files: Arc<[String]>,
    /// The files a summary can materialize, computed when the library was loaded. `None` falls back to checking every
    /// file; a full plan always checks every file.
    pub layout: Option<&'a LayoutProfile>,
}

/// What a summary has to materialize for one file list. The profile is computed once with the game's rules, so a
/// status does not run the mirror, root-file and shared-kind checks for every file of every mod.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LayoutProfile {
    /// The files that can land outside the mod's own folder, in file order. `None` means none of them can; `All` means
    /// every file can (a mirrored-only list or a shared rule over the whole list).
    pub materialized: Materialized,
    /// Every file keeps its game path, so the per-file mirror check is already answered.
    pub mirrored_only: bool,
}

/// How much of a file list a summary materializes: none, all, or the files named by index.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Materialized {
    /// No profile was computed for this list; check every file.
    #[default]
    Unknown,
    /// No file of the list can materialize.
    None,
    /// Every file of the list materializes.
    All,
    /// These file indexes materialize, in file order.
    Some(Vec<u32>),
}

/// One mod of a plan: its family, its folder and the file list that its planned files index into. A plan keeps one
/// of these per mod, so a full plan copies one refcount per mod instead of three strings per file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSource {
    /// Family id; conflicts name families, because every version of a mod is the same mod.
    pub id: std::sync::Arc<str>,
    /// Version folder in the library.
    pub folder: std::sync::Arc<str>,
    /// File paths relative to the library folder, sorted.
    pub files: std::sync::Arc<[String]>,
}

/// One file a plan places, as indexes into the plan's mod sources and file lists with its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    /// Index of the mod in `DeployPlan::sources`.
    pub source: u32,
    /// Index of the file in that mod's file list.
    pub file: u32,
    /// Path relative to the deployment root, with `/`.
    pub dest: String,
    /// The file lands outside the mod's own folder, where the game may have a file of its own.
    pub game_path: bool,
}

/// An entry for the game's order file (for example `mods.settings`). Only mods with a priority the user set have one,
/// disabled ones included, so the game decides the order of the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsRow {
    pub folder: String,
    pub enabled: bool,
    pub priority: u32,
}

/// The same file in several enabled mods. `winner` wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub winner: String,
    pub losers: Vec<String>,
}

/// Why a deployment plan cannot be executed: a repeated mod, a repeated destination or a file-folder clash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Two entries with the same family id. The second is skipped.
    DuplicateMod { id: String },
    /// Two files of one mod land at the same path, for example two folders of a flat slot with the same file name.
    DuplicateDestination { path: String, id: String },
    /// A file and a folder land at paths that cannot coexist (`DLC/x` and `DLC/x/y`).
    FileFolderCollision { path: String },
}

impl Problem {
    /// The text the interface shows for this problem.
    pub fn message(&self) -> Message {
        match self {
            Self::DuplicateMod { id } => Message::new("plan.duplicate_mod").with("id", id),
            Self::DuplicateDestination { path, id } => Message::new("plan.duplicate_destination")
                .with("path", path)
                .with("id", id),
            Self::FileFolderCollision { path } => {
                Message::new("plan.file_folder_collision").with("path", path)
            }
        }
    }
}

/// How much of the plan to materialize. `Full` is for a deployment; `Summary` is for the status and the preview,
/// which need the file count, the conflicts and the problems but not one `PlannedFile` per file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMode {
    Full,
    Summary,
}

/// The outcome of planning a profile's mods: the files, settings rows, conflicts and problems.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeployPlan {
    pub files: Vec<PlannedFile>,
    /// The mods whose files the plan materialized, by the index in `PlannedFile::source`.
    pub(crate) sources: Vec<PlanSource>,
    /// All files the plan places. In `Full` this is `files.len()`; in `Summary` it also counts the files that were not
    /// materialized.
    pub file_count: u32,
    pub settings: Vec<SettingsRow>,
    pub conflicts: Vec<Conflict>,
    pub problems: Vec<Problem>,
}

impl DeployPlan {
    /// The mod a planned file belongs to. A plan built by this module always has it.
    pub(crate) fn source_of(&self, file: &PlannedFile) -> Option<&PlanSource> {
        self.sources.get(file.source as usize)
    }

    /// The family id of a planned file.
    pub(crate) fn family(&self, file: &PlannedFile) -> &str {
        self.source_of(file).map_or("", |source| source.id.as_ref())
    }

    /// The library folder of a planned file.
    pub(crate) fn folder(&self, file: &PlannedFile) -> &str {
        self.source_of(file)
            .map_or("", |source| source.folder.as_ref())
    }

    /// The path of a planned file relative to its library folder.
    pub(crate) fn rel_path(&self, file: &PlannedFile) -> &str {
        self.source_of(file)
            .and_then(|source| source.files.get(file.file as usize))
            .map_or("", String::as_str)
    }

    /// The path keys of the files the plan places in the deployment root. A game's requirement is met when one of them
    /// is the required file (see `GameDef::unmet_requirement`).
    pub fn placed_keys(&self) -> std::collections::BTreeSet<String> {
        self.files
            .iter()
            .map(|file| crate::domain::pathkey::path_key(&file.dest))
            .collect()
    }
}
