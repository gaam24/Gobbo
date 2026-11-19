//! A profile entry in the form the deployment understands: the mod family, the selected version, its slot, and
//! whether the version goes into the game. The order of the list decides conflicts.

/// A mod version selected by a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileMod {
    /// Id of the mod family.
    pub id: String,
    /// Version folder in the library.
    pub folder: String,
    /// Id of the slot. Empty in the `mirror` layout.
    pub slot: String,
    pub enabled: bool,
    /// The priority the user set for the mod. `None` means the game decides.
    pub priority: Option<u32>,
}
