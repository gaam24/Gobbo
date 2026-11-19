//! The generic family fields copied out for the views and the packs.

use super::Family;

/// The notes, lists and requirements of a family, copied out for the views and the pack. A mod without a family in the
/// library gets the empty ones.
#[derive(Default)]
pub struct FamilyFields {
    pub notes: String,
    pub instructions: String,
    pub requires: Vec<String>,
    pub incompatible: Vec<String>,
    pub overrides: Vec<String>,
}

impl FamilyFields {
    pub fn of(family: Option<&Family>) -> Self {
        family.map_or_else(Self::default, |family| Self {
            notes: family.notes.clone(),
            instructions: family.instructions.clone(),
            requires: family.requires.clone(),
            incompatible: family.incompatible.clone(),
            overrides: family.overrides.clone(),
        })
    }
}
