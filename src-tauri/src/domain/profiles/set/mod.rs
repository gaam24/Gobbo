//! The set of profiles of a game: the active one, the launch memory and the operations that create, rename, delete
//! and remap them. It also holds the pure rules of the mod-list edits (page-block moves, group sizes and separator
//! consistency), so a command can decide an edit with plain data and no disk access.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorKind, MOD_MISSING};

use super::profile::{Entry, Profile};

mod edits;
mod ids;
#[cfg(test)]
mod tests;

pub use edits::{
    MoveSide, PageKeys, add_group, move_block, normalize_groups, remove_group, set_block_enabled,
    set_mod_group, set_separator,
};
pub use ids::{next_group_id, next_separator_id};

/// The name of the profile a game starts with.
pub const DEFAULT_NAME: &str = "Default";
/// The longest profile name, in characters.
pub const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profiles {
    /// The on-disk format. An older file is read as the current version; a newer one is refused.
    #[serde(default = "crate::domain::state_schema")]
    pub schema_version: u32,
    /// Id of the profile that is active.
    pub active: String,
    /// Id of the profile whose deployment was used for the last launch. It tells undo which running
    /// overlay layer must not be touched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_launched: Option<String>,
    pub profiles: Vec<Profile>,
}

impl Profiles {
    /// A set with one empty profile, at the first start.
    pub fn with_default(mods: Vec<Entry>) -> Self {
        let profile = Profile::new("p1".to_string(), DEFAULT_NAME.to_string(), mods);
        Self {
            schema_version: crate::domain::STATE_SCHEMA,
            active: profile.id.clone(),
            last_launched: None,
            profiles: vec![profile],
        }
    }

    /// After loading, there must be at least one profile. When the active one is missing, the first one becomes active.
    pub(crate) fn check(mut self) -> Result<Self, AppError> {
        if self.schema_version > crate::domain::STATE_SCHEMA {
            return Err(AppError::new("state.schema_newer")
                .with("found", self.schema_version)
                .with("supported", crate::domain::STATE_SCHEMA)
                .kind(ErrorKind::Permanent));
        }
        self.schema_version = crate::domain::STATE_SCHEMA;
        if self.profiles.is_empty() {
            return Err(AppError::new("profiles.no_profiles").kind(ErrorKind::Permanent));
        }
        if !self.profiles.iter().any(|p| p.id == self.active) {
            self.active = self.profiles[0].id.clone();
        }
        Ok(self)
    }

    /// The active profile, or the first one when the id is stale.
    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active)
            .unwrap_or(&self.profiles[0])
    }

    /// The active profile for an edit, with the same fallback as `active`.
    pub fn active_mut(&mut self) -> &mut Profile {
        let index = self
            .profiles
            .iter()
            .position(|p| p.id == self.active)
            .unwrap_or(0);
        &mut self.profiles[index]
    }

    /// One entry of the active list. A missing family is a user error.
    pub fn active_entry_mut(&mut self, id: &str) -> Result<&mut Entry, AppError> {
        self.active_mut()
            .mods
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| {
                AppError::new(MOD_MISSING)
                    .with("folder", id)
                    .kind(ErrorKind::Permanent)
            })
    }

    /// The error for a profile id the set does not hold. The user can pick another profile.
    pub fn not_found(id: &str) -> AppError {
        AppError::user("profiles.not_found").with("id", id)
    }

    /// The profile `id`, or a user error when the set does not hold it.
    pub fn get(&self, id: &str) -> Result<&Profile, AppError> {
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| Self::not_found(id))
    }

    /// Creates an empty profile named `wanted`, or `wanted (2)` and so on when a profile has that name. A recipe
    /// (a pack or a collection) names its profile this way, since the user did not type the name.
    pub fn create_unique(&mut self, wanted: &str) -> Result<String, AppError> {
        let name = self.free_name(wanted);
        self.create(&name, None)
    }

    /// New profile: empty, or a copy of the given one. Does not change the active one. Returns its id.
    pub fn create(&mut self, name: &str, copy_from: Option<&str>) -> Result<String, AppError> {
        let name = self.checked_name(name, None)?;
        let source = copy_from.map(|id| self.get(id)).transpose()?;
        let list = source.map(Profile::list_state);
        let user_data = source.map(|profile| profile.user_data.clone());
        let pack = source.and_then(|profile| profile.pack.clone());
        let collections = source
            .map(|profile| profile.collections.clone())
            .unwrap_or_default();
        let id = self.next_id();
        let mut profile = Profile::new(id.clone(), name, Vec::new());
        if let Some(state) = list {
            profile.restore_list(state);
        }
        profile.collections = collections;
        if let Some(user_data) = user_data {
            profile.user_data = user_data;
        }
        profile.pack = pack;
        self.profiles.push(profile);
        Ok(id)
    }

    /// Renames the profile `id` after checking the name and that the profile exists.
    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), AppError> {
        self.get(id)?;
        let name = self.checked_name(name, Some(id))?;
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.id == id) {
            profile.name = name;
        }
        Ok(())
    }

    /// Deletes a profile. The last one cannot be deleted. When the active one goes, the first one becomes active.
    pub fn delete(&mut self, id: &str) -> Result<(), AppError> {
        self.get(id)?;
        if self.profiles.len() == 1 {
            return Err(AppError::user("profiles.last_profile"));
        }
        self.profiles.retain(|p| p.id != id);
        if self.active == id {
            self.active = self.profiles[0].id.clone();
        }
        Ok(())
    }

    /// Makes the profile `id` active, after checking that the set holds it.
    pub fn switch(&mut self, id: &str) -> Result<(), AppError> {
        self.get(id)?;
        self.active = id.to_string();
        Ok(())
    }

    /// Removes a family from every profile's list, because all its versions have disappeared. Its deployment ledger
    /// and the file winners that name it go with it, so a later import of the same family starts clean.
    pub fn remove_mod(&mut self, id: &str) {
        for profile in &mut self.profiles {
            profile.remove_mods(|entry| entry.id == id);
            // The list may already be a draft, so the family can still be in the ledger without a current entry.
            profile.deployed_mods.retain(|entry| entry.id != id);
            profile.deployed_winners.retain(|_, family| family != id);
            profile.winners.retain(|_, family| family != id);
        }
    }

    /// The profile `id` for an edit, or a user error when the set does not hold it.
    pub fn get_mut(&mut self, id: &str) -> Result<&mut Profile, AppError> {
        self.profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| Self::not_found(id))
    }

    /// Points every profile that used `old` at `new`.
    pub fn replace_version(&mut self, old: &str, new: &str) {
        for profile in &mut self.profiles {
            for entry in profile.mods.iter_mut().filter(|entry| entry.folder == old) {
                entry.folder = new.to_string();
            }
        }
    }

    /// Replaces a family id in every profile list and in the file-winner maps. An entry that becomes a duplicate is
    /// kept once, in its first position. Returns whether anything changed.
    pub fn rename_family(&mut self, from: &str, to: &str) -> bool {
        let mut changed = self.remap_entries(|entry| (entry.id == from).then(|| to.to_string()));
        changed |= self.remap_winners(|family| (family == from).then(|| to.to_string()));
        changed
    }

    fn remap_entries(&mut self, mut map: impl FnMut(&Entry) -> Option<String>) -> bool {
        let mut changed = false;
        for profile in &mut self.profiles {
            for entries in [&mut profile.mods, &mut profile.deployed_mods] {
                for entry in entries.iter_mut() {
                    if let Some(new) = map(entry)
                        && entry.id != new
                    {
                        entry.id = new;
                        changed = true;
                    }
                }
                let mut seen = std::collections::BTreeSet::new();
                let before = entries.len();
                entries.retain(|entry| seen.insert(entry.id.clone()));
                changed |= entries.len() != before;
            }
        }
        changed
    }

    fn remap_winners(&mut self, mut map: impl FnMut(&str) -> Option<String>) -> bool {
        let mut changed = false;
        for profile in &mut self.profiles {
            for winners in [&mut profile.winners, &mut profile.deployed_winners] {
                for family in winners.values_mut() {
                    if let Some(new) = map(family.as_str())
                        && *family != new
                    {
                        *family = new;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// A name without surrounding spaces, non-empty, short, and unique regardless of letter case.
    /// `except` is the profile that is being renamed, so it may keep its own name.
    fn checked_name(&self, name: &str, except: Option<&str>) -> Result<String, AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::user("profiles.name_empty"));
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(AppError::user("profiles.name_too_long").with("max", MAX_NAME_CHARS));
        }
        if self.name_taken(name, except) {
            return Err(AppError::user("profiles.name_taken").with("name", name));
        }
        Ok(name.to_string())
    }

    /// Whether another profile (not `except`) already has this name. Case is ignored for every alphabet, so
    /// `free_name` and the check in `create` and `rename` can never disagree.
    fn name_taken(&self, name: &str, except: Option<&str>) -> bool {
        let lower = name.trim().to_lowercase();
        self.profiles
            .iter()
            .any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == lower)
    }
}
