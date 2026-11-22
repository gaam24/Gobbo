//! The import types the window reads: progress, the import result and the archive preview.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::domain::library::SkippedFile;
use crate::import::fomod as fomod_model;

/// The archive and FOMOD answers needed before an import can finish. The acquire outcomes and the collection views
/// share this one shape, so a new field has a single owner.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ChoiceNeeded {
    pub path: PathBuf,
    pub module_name: String,
    pub steps: Vec<FomodStepView>,
}

/// The result of an import.
#[derive(Debug, Serialize, specta::Type)]
pub struct ImportView {
    pub folders: Vec<String>,
    pub files: u32,
    /// Entries no rule took. Documentation and installer metadata are not counted here.
    pub skipped: u32,
    /// Every archive entry that was not unpacked, with the reason.
    pub skipped_files: Vec<SkippedFile>,
    /// The mods the import added, with the family each one belongs to. A family that already had a version keeps its
    /// selected one, so the window can offer the new version instead of switching silently.
    pub mods: Vec<ImportedModView>,
}

/// What an archive holds against what its installed folder holds now.
#[derive(Debug, Serialize, specta::Type)]
pub struct ArchiveReport {
    pub folder: String,
    /// The file name of the archive copy in `archives/`.
    pub archive: String,
    /// Every entry that the current rules and FOMOD choices do not unpack.
    pub skipped: Vec<SkippedFile>,
    /// Entries no rule takes.
    pub skipped_total: u32,
    /// Files the rules place but the installed folder does not have.
    pub missing: Vec<String>,
    /// Files in the installed folder that the archive does not place.
    pub extra: Vec<String>,
    /// Files whose size on disk differs from the archive entry.
    pub changed: Vec<String>,
    pub expected: u32,
    pub installed: u32,
}

/// One mod that an import added to the library.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ImportedModView {
    /// Id of the mod family. A version of an existing mod shares it with the selected version.
    pub family: String,
    /// The digest of the archive this folder came from, so a pack can match a fresh import to its recipe.
    pub sha256: String,
    /// The folder of the imported version.
    pub folder: String,
    pub name: String,
    pub version: Option<String>,
}

/// A stage of importing one archive. Each starts at 0%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ImportStage {
    /// Computing the SHA-256 sum of the source file.
    Hashing,
    /// Unpacking mods into the library.
    Extracting,
    /// Copying the archive to `archives/`. An import hashes the archive while copying it into its working folder, so
    /// this stage stays in the IPC type for the frontend but is no longer reported.
    #[expect(
        dead_code,
        reason = "the stage stays in the IPC type so a frontend build keeps compiling"
    )]
    Copying,
}

/// The import progress, sent to the window while it works.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ImportProgress {
    pub stage: ImportStage,
    pub percent: u32,
}

/// A preview of the archive before import: a plain zip, or a FOMOD installer with steps for the wizard.
#[derive(Debug, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ZipInspect {
    Plain,
    Fomod {
        module_name: String,
        steps: Vec<FomodStepView>,
    },
}

/// One step of the FOMOD installer, with its groups, as the wizard shows it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct FomodStepView {
    pub name: String,
    /// The step's `<visible>` condition. `None` means the step is always shown.
    pub visible: Option<FomodDependencies>,
    pub groups: Vec<FomodGroupView>,
}

/// One group of a FOMOD step as the wizard shows it, with its options and selection rule.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct FomodGroupView {
    pub name: String,
    pub kind: fomod_model::GroupKind,
    pub options: Vec<FomodOptionView>,
}

/// One option of a FOMOD group as the wizard shows it, with its kind, flags and destinations.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct FomodOptionView {
    pub name: String,
    pub description: String,
    pub kind: fomod_model::OptionKind,
    /// Flags this option sets when picked, from `<conditionFlags>`.
    pub flags: BTreeMap<String, String>,
    /// Normalized destinations of this option's files, for `<visible>` file dependencies.
    pub destinations: Vec<String>,
}

/// A `<visible>` condition tree. The wizard evaluates it as the user picks options, so hidden steps are skipped.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FomodDependencies {
    All(Vec<FomodCondition>),
    Any(Vec<FomodCondition>),
}

/// One child of a FOMOD condition tree: a leaf check or a nested group with its own operator.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FomodCondition {
    Dependency(FomodDependency),
    Group(FomodDependencies),
}

/// One leaf check of a FOMOD dependency tree: a file, a flag or the game version.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FomodDependency {
    File { file: String, active: bool },
    Flag { flag: String, value: String },
    Game,
}
