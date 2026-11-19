//! The data a FOMOD installer leaves in the library: the wizard choices of a mod version. The parser and the file
//! matcher stay in `import::fomod`; these types are pure records so the library and the packs can keep them.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The user's choice from the wizard: an option in a group, by the group's place in the installer. Group names may
/// repeat, so the position — not the name — decides which group the choice belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Choice {
    /// The step in `ModuleConfig.xml`, counted from zero.
    #[serde(default)]
    pub step: u32,
    /// The group inside that step, counted from zero.
    #[serde(default)]
    pub group_index: u32,
    /// The group's name, kept for the list that shows the choice.
    pub group: String,
    /// The chosen option's name.
    pub option: String,
}
