//! Creating and checking a game definition from the window: a folder inspection that fills the form, the loader
//! presets the form offers, and the one path that writes `<data>/definitions/games/<id>/game.toml`. A definition is
//! data: this module writes the same TOML a user could write by hand, and the loader preset is merged exactly as for a
//! shipped game.
//!
//! The entry points are small; the work lives in `check.rs` (the draft, the save and the source), `inspect.rs` (what a
//! game folder suggests) and `presets.rs` (the loader list). The public types stay here because the commands and the
//! import preview both name them.

mod check;
mod inspect;
mod package;
mod presets;

#[cfg(test)]
mod tests;

use serde::Serialize;
use specta::Type;

use super::GameOrigin;
use super::view::GameView;

pub use check::{check, merged, save, source};
pub use inspect::inspect_folder;
pub use package::{
    DefinitionPackagePreview, ImportedDefinitionPackage, import_package, inspect_package,
};
pub use presets::loader_presets;

/// The problems reported while a definition is edited. Every problem names the section of the form that owns it, so
/// the window can put the message beside the fields instead of at the bottom of the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct DefinitionProblemView {
    pub section: String,
    pub message: String,
}

/// What the editor gets after it checks a draft: whether it can be saved, the problems, and what the definition will
/// do once the loader preset is merged under it.
#[derive(Debug, Serialize, Type)]
pub struct DefinitionCheck {
    pub ok: bool,
    pub id: String,
    pub name: String,
    pub problems: Vec<DefinitionProblemView>,
    pub summary: Option<DefinitionSummary>,
}

/// The effect of a definition, in the same shape the game settings show. `game` carries the places, methods, loader
/// and providers; `requires` and `executables` are what the editor shows in its short summary.
#[derive(Debug, Serialize, Type)]
pub struct DefinitionSummary {
    pub game: GameView,
    pub requires: Vec<String>,
    pub executables: Vec<ExecutableSummary>,
    pub user_data: Vec<String>,
}

/// One program the game starts with.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ExecutableSummary {
    pub name: String,
    pub path: String,
}

/// A definition read for editing, with the folder it came from.
#[derive(Debug, Serialize, Type)]
pub struct DefinitionSource {
    pub id: String,
    pub text: String,
    pub origin: GameOrigin,
}

/// A definition that was written. The window uses the name for its notice and the id to add the game.
#[derive(Debug, Serialize, Type)]
pub struct SavedDefinition {
    pub id: String,
    pub name: String,
}

/// A loader preset the editor can offer. `places` and `requires` are the base rules; each variant has the rules of the
/// preset with that variant merged over it, which is what the game would get.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LoaderPresetView {
    pub id: String,
    pub name: String,
    pub origin: GameOrigin,
    pub default_variant: Option<String>,
    pub requires: Vec<String>,
    pub places: Vec<String>,
    pub root_files: Vec<String>,
    pub variants: Vec<LoaderVariantView>,
    pub problems: Vec<String>,
}

/// One variant of a loader preset.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LoaderVariantView {
    pub name: String,
    pub requires: Vec<String>,
    pub places: Vec<String>,
    pub root_files: Vec<String>,
}

/// One program found in a game folder, as a suggestion for the launch step.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ExecutableSuggestion {
    /// Relative to the game folder, with `/` separators.
    pub path: String,
    /// The file name without its folder.
    pub name: String,
}

/// The Steam manifest that describes a game folder, when the folder is in a Steam library.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SteamSuggestion {
    pub app_id: u32,
    pub name: Option<String>,
    pub install_dir: Option<String>,
}

/// A loader preset that seems to be installed in a game folder. The editor preselects it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct LoaderSuggestion {
    pub id: String,
    pub name: String,
    /// The variant whose files were found, when the preset has variants.
    pub variant: Option<String>,
}

/// The store ids that point at a game folder, by the detect list each belongs to. A GOG or Epic game installed through
/// Heroic, Legendary or Lutris carries its GOG product id or Epic `AppName`, so it lands in `gog` or `epic`.
#[derive(Debug, Clone, Default, Serialize, Type)]
pub struct StoreIds {
    pub gog: Vec<String>,
    pub epic: Vec<String>,
    pub lutris: Vec<String>,
}

/// What the editor learns from a chosen game folder. Everything is read-only; the folder is never changed.
#[derive(Debug, Serialize, Type)]
pub struct FolderInspection {
    /// The folder as the user gave it.
    pub folder: String,
    /// The name from the Steam manifest, or the folder name.
    pub name: String,
    /// The id suggested from `name`; the user can change it before the first save.
    pub suggested_id: String,
    /// Programs that look like the game's own, best first.
    pub executables: Vec<ExecutableSuggestion>,
    /// Relative folders at the first two levels, for the mirror picker.
    pub directories: Vec<String>,
    /// The first executable path, a marker for the folder check.
    pub markers: Vec<String>,
    pub steam: Option<SteamSuggestion>,
    /// Loader presets whose files were found in the folder.
    pub loaders: Vec<LoaderSuggestion>,
    pub stores: StoreIds,
}
