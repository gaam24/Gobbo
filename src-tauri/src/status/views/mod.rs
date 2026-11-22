//! The view models of the game's status: what the window reads from the queries.

use serde::{Deserialize, Serialize};

use crate::context::Settings;
use crate::context::profile::history::HistoryState;
use crate::domain::library::ModInfo;
use crate::domain::user_data::{Kind, Mode};
use crate::error::Message;
use crate::import::fomod;
use crate::status::bisect::BisectView;
use crate::status::stage::{BlockerKey, GameStage, NextAction};
use crate::status::steps::StepChoices;

mod source;
mod version;

pub(super) use source::description_preview;
pub use source::{
    ModSource, ModSourceDetails, ProviderDlcs, SourcePreview, SourceRequirementView, SourceSummary,
    SourceSyncView, UpdateReport,
};
pub use version::VersionView;

/// One profile that lists a mod, with the selected version and whether its deployment uses the mod.
#[derive(Debug, Serialize, specta::Type)]
pub struct ModUsageProfile {
    pub id: String,
    pub name: String,
    /// The installed version folder the profile selects.
    pub folder: String,
    pub version: Option<String>,
    pub enabled: bool,
    pub prepared: bool,
    pub running: bool,
}

/// The profiles that use a mod family. The delete confirmation shows them before anything is removed.
#[derive(Debug, Serialize, specta::Type)]
pub struct ModUsage {
    pub profiles: Vec<ModUsageProfile>,
    /// At least one prepared deployment contains the mod.
    pub prepared: bool,
    /// The running game uses the mod. Deleting then would touch files the game may be reading.
    pub running: bool,
}

/// One enabled entry of the last deployment. It carries the family, so the UI can restore the exact selected versions.
#[derive(Debug, Serialize, specta::Type)]
pub struct DeployedRow {
    pub id: String,
    pub folder: String,
}

/// One mod of the active profile, as a row of the mod list with its selected version and counters.
#[derive(Debug, Hash, Serialize, specta::Type)]
pub struct ModView {
    /// Id of the mod family. Every version of a mod shares it.
    pub id: String,
    /// The selected version folder.
    pub folder: String,
    /// The selected version came from a FOMOD installer, so its options can be changed by reinstalling.
    pub fomod_installer: bool,
    /// How many versions of the family are installed. The list carries the count; the picker asks for the versions.
    pub version_count: u32,
    /// Unix time in seconds of the selected version's import.
    pub installed_at: u32,
    /// Slot id. Empty in the `mirror` layout.
    pub slot: String,
    pub enabled: bool,
    /// Position in the list, from 1. The list decides which copy of a shared file wins.
    pub priority: u32,
    /// The load priority the user set, written to the game's settings file. `None` leaves the order to the game.
    pub load_priority: Option<u32>,
    pub files: u32,
    /// Size in KB. Specta does not export `u64` to TypeScript, and KB fit in `u32`.
    pub size_kb: u32,
    /// How many files of this mod win a conflict, and how many are overwritten.
    pub wins: u32,
    pub loses: u32,
    /// The original name of the archive the selected version was unpacked from.
    pub archive: Option<String>,
    /// The name of the archive copy in `archives/`, for example `Mod 1.2.0.zip`.
    pub archive_file: Option<String>,
    /// Archive entries no rule took when the version was imported.
    pub skipped: u32,
    /// Name, version, author and description of the selected version.
    pub info: ModInfo,
    /// The first source with page data, in provider order. The list keeps only what the row draws; the page text is
    /// read by `mod_extras` when a column or the inspector needs it.
    pub source: Option<SourcePreview>,
    /// The external sources of the mod, one per provider. The interface can show them without knowing the provider.
    pub sources: Vec<ModSource>,
    /// The page the part belongs to, for example `nexus:225`. `None` for a local mod.
    pub page_id: Option<String>,
    /// The page's display name, when a provider page was read.
    pub page_name: Option<String>,
    /// The persistent part id, for example a Nexus `mod_file_id`.
    pub part_id: Option<String>,
    /// The part's display name.
    pub part_name: Option<String>,
    /// The provider's category for the part, for example `main` or `optional`.
    pub part_category: Option<String>,
    /// The last saved update check of the mod, read from the library with the version the profile selects now. `None`
    /// when no source was checked.
    pub update: Option<UpdateReport>,
    /// Tags given to the mod in Gobbo, for filtering the list.
    pub tags: Vec<String>,
    /// Instructions for players, shown and exported with the mod list.
    pub instructions: String,
    /// How many of the mod's requirements the profile does not meet: a required mod that is not in the library or is
    /// switched off, and each list of provider alternatives that none of its mods is on. Counted for every mod, on or
    /// off.
    pub unmet_requirements: u32,
    /// The names of the mods in the mod's `requires`, in the same order. A mod the library does not have shows its id.
    pub requires_names: Vec<String>,
    /// The names of the provider requirements, grouped as alternatives: each inner list is one requirement, and its
    /// names are interchangeable. It feeds the requirement column and a copied list without the full requirement data.
    pub requirement_groups: Vec<Vec<String>>,
    /// The names of the profiles of the game whose list has the mod, the active one among them.
    pub profiles: Vec<String>,
    /// The names of the active profile's collections (Nexus collections, Thunderstore modpacks) the mod belongs to.
    pub collections: Vec<String>,
}

/// The page text and counters that only the inspector and the hidden columns read. `mod_extras` answers with it for
/// the rows in the viewport, and `mod_details` carries it for one mod.
#[derive(Debug, Serialize, specta::Type)]
pub struct ModExtras {
    /// The selected version folder, so the caller can match the answer to its row.
    pub folder: String,
    /// The description preview of the installed version's `info.json`.
    pub info_description: Option<String>,
    /// The first source with page data, including the page text and counters.
    pub source: Option<SourceSummary>,
    /// The user's note about the mod.
    pub notes: String,
    /// Mods this mod needs on any provider, with where each stands in the profile.
    pub requirements: Vec<SourceRequirementView>,
    /// DLC notes per provider: one list per requirement.
    pub source_dlcs: Vec<ProviderDlcs>,
    /// When each provider's requirements were read.
    pub source_sync: Vec<SourceSyncView>,
    /// The installer choices made when the selected version was installed.
    pub fomod: Vec<fomod::Choice>,
    /// The selected version came from a FOMOD installer, so its options can be changed by reinstalling.
    pub fomod_installer: bool,
    /// Mods (family ids) this mod needs to work.
    pub requires: Vec<String>,
    /// Mods (family ids) this mod does not work with.
    pub incompatible: Vec<String>,
    /// The names of the mods in `incompatible`, in the same order.
    pub incompatible_names: Vec<String>,
    /// Mods (family ids) this mod overrides: it should stand above them in the list.
    pub overrides: Vec<String>,
    /// The names of the mods in `overrides`, in the same order.
    pub overrides_names: Vec<String>,
    /// The game steps this mod asks for before the game starts, by step id.
    pub steps_before: Vec<String>,
    /// The game steps this mod asks for after the game exits, by step id.
    pub steps_after: Vec<String>,
}

/// One contested file path, the mod that wins it and the mods that lose it.
#[derive(Debug, Serialize, specta::Type)]
pub struct ConflictView {
    pub path: String,
    pub winner: String,
    pub losers: Vec<String>,
}

/// The state of a mod file with respect to conflicts. The color in the UI depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Win,
    Lose,
    None,
    Disabled,
}

/// One file of a mod's selected version with its conflict state and the message the window shows for it.
#[derive(Debug, Serialize, specta::Type)]
pub struct FileView {
    pub path: String,
    pub state: FileState,
    pub result: Message,
}

/// The full detail of one mod for the inspector: its sources, versions, files, conflicts and relations.
#[derive(Debug, Serialize, specta::Type)]
pub struct ModDetails {
    pub folder: String,
    /// The full info of the selected version, including a description the status list only previews.
    pub info: ModInfo,
    /// The external sources of the family, with their full page data.
    pub sources: Vec<ModSourceDetails>,
    /// Every installed version of the family, newest first. The status list carries only the count.
    pub versions: Vec<VersionView>,
    pub files: Vec<FileView>,
    /// Conflicts of this mod with other families, for the inspector.
    pub conflicts: Vec<ConflictView>,
    /// The steps the game defines, and the ones this mod asks for.
    pub steps: StepChoices,
    /// Whether the user can choose which copy of a shared file wins. Only the `Mirror` layout lets Gobbo decide that.
    pub file_winners: bool,
    /// The page text, notes and relation details that the list row leaves out.
    pub extras: ModExtras,
}

/// One user-data set of the game for the active profile. The frontend translates `kind` with one universal label.
#[derive(Debug, Serialize, specta::Type)]
pub struct UserDataView {
    pub id: String,
    pub kind: Kind,
    /// The folder the game sees, resolved for this installation.
    pub path: String,
    /// The effective mode: `redirect` when the overlay serves it, `copy` otherwise.
    pub mode: Mode,
    pub isolated: bool,
    pub inherit: bool,
    /// The profile's writable layer or copy store; "Open folder" opens this.
    pub upper: String,
    /// The profile's upper layer exists, so "Open folder" can reach it.
    pub upper_exists: bool,
    /// The folder or its parent exists, so the path can be used.
    pub available: bool,
}

/// A game profile in the list view. The mod list in `Status` covers only the active profile.
#[derive(Debug, Serialize, specta::Type)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub mod_count: u32,
    pub enabled_count: u32,
    pub active: bool,
    /// This profile has a prepared deployment. Overlay profiles may keep their layers side by side.
    pub deployed: bool,
    /// The profile has its own user-data folders (saves, settings). Deleting it removes them too.
    pub has_user_data: bool,
    /// The collections installed into this profile, for the badge and the collection panel.
    pub collections: Vec<crate::domain::collection::CollectionBadge>,
}

/// The row revisions the window holds, in its order. A mutation that answers with a patch sends back only the rows
/// whose revision differs from these.
#[derive(Debug, Deserialize, Serialize, specta::Type)]
pub struct KnownRows {
    pub ids: Vec<String>,
    pub revisions: Vec<String>,
}

/// The game's status: everything the window reads to draw the list, the header and the next action.
#[derive(Debug, Serialize, specta::Type)]
pub struct Status {
    pub game_name: String,
    /// A short description of the game under its name, for example `Steam 292030`.
    pub game_subtitle: String,
    /// How this install starts on this computer, from the files in its folder. The Proton settings apply only when it
    /// is not `native`, which it always is on Windows.
    pub runtime: crate::games::runtime::Runtime,
    /// The one derived state of the game. Every screen reads this instead of deriving the state from booleans.
    pub stage: GameStage,
    /// The one thing to do next, derived from `stage`.
    pub next_action: NextAction,
    /// The reason an apply cannot run yet, when there is one. The window only translates it.
    pub blocker: Option<BlockerKey>,
    pub settings: Settings,
    pub game_ready: bool,
    pub settings_ready: bool,
    /// Whether the game reads mods from outside its folder and the user can point to that folder.
    pub needs_mods_root: bool,
    /// Whether the configured mods root exists. An automatically resolved root may be missing and will be created on deploy.
    pub mods_ready: bool,
    /// The effective folder that holds the mods: the user's override or the resolved definition path.
    pub mods_root: String,
    /// The game reads `mods.settings`, so Gobbo needs its folder.
    pub needs_settings_dir: bool,
    pub deployed: bool,
    /// Mods whose place differs from the last deployed list (added, removed, or moved). Zero when they match.
    pub draft_changes: u32,
    /// Unix time in seconds of the last deployment, while something is deployed.
    pub deployed_at: Option<u32>,
    /// Entries of the last deployed list, in order. Discarding draft changes restores this list.
    pub deployed_list: Vec<DeployedRow>,
    /// The game's profiles. All of them, including inactive ones.
    pub profiles: Vec<ProfileView>,
    /// The mods of the active profile, in priority order.
    pub mods: Vec<ModView>,
    /// One revision per mod, in the same order. It covers every serialized row field; a window that sends them back
    /// receives only the rows that changed.
    pub revisions: Vec<String>,
    /// The reply number. It grows with each status built, so the window can ignore an answer that is older than the list
    /// it already shows.
    pub build: u32,
    /// Whether `mods` holds only the rows that changed since the revisions the window sent.
    pub patch: bool,
    /// Ids that left the list since the revisions the window sent.
    pub removed: Vec<String>,
    /// The full order, when it differs from the order the window sent.
    pub order: Option<Vec<String>>,
    /// The separators of the active profile's list. Each one sits above the mod it names, or at the end.
    pub separators: Vec<crate::domain::profiles::Separator>,
    /// The groups of the active profile's list.
    pub groups: Vec<crate::domain::profiles::ModGroup>,
    /// Whether a change to the active list can be undone or redone.
    pub history: HistoryState,
    /// The search for the mod that breaks the game, while one runs on the active profile.
    pub bisect: Option<BisectView>,
    /// The game or its tools wrote files into the overlay of the active profile, kept out of the game folder.
    pub has_overwrite: bool,
    /// Number of conflicting file paths among enabled mods. The paths themselves are in `mod_details`.
    pub conflict_count: u32,
    pub problems: Vec<Message>,
    /// Mods of the profile that do not fit together: a required mod missing or off, or two enabled conflicting mods.
    /// They are shown, but do not block deployment.
    pub warnings: Vec<Message>,
    pub library_dir: String,
    /// The folder with the copies of `.zip` archives that the mods were unpacked from.
    pub archives_dir: String,
    /// The state folder of the active profile, once it exists: its deployment journal, overlay layer and overwrite folder.
    pub profile_dir: Option<String>,
    /// Where the game and its tools write files that no mod owns, inside the active profile's state folder.
    pub overwrite_dir: String,
    /// Per-profile data paths declared by the game definition, resolved for this installation.
    pub game_profile_data: Vec<String>,
    /// The game's user-data sets and the active profile's choices.
    pub user_data: Vec<UserDataView>,
    /// The command for Steam launch options that starts the game through Gobbo (Linux only).
    pub launch_hint: String,
    /// Launch files from the game definition. The "Run" buttons come from here.
    pub executables: Vec<ExecutableView>,
    /// Tools from the game definition, with the folder of their program when it is in the game folder.
    pub tools: Vec<ToolView>,
}

/// A tool of the game in the view. `folder` is `None` when its program is not in the game folder.
#[derive(Debug, Serialize, specta::Type)]
pub struct ToolView {
    pub id: String,
    pub name: String,
    /// What the tool makes, for example `generated-mod`.
    pub output: String,
    pub folder: Option<String>,
    /// The provider mod the tool comes from, when the definition names one.
    pub source: Option<crate::domain::source::SourceRef>,
    /// The prerequisites that are not in the Proton prefix yet, by name.
    pub missing: Vec<String>,
}

/// A launch file of the game in the view: the name for the button, whether it is the default, and its
/// predefined arguments (for example `-windowed`).
#[derive(Debug, Serialize, specta::Type)]
pub struct ExecutableView {
    pub name: String,
    pub default: bool,
    pub args: Vec<String>,
}
