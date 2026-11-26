//! The store side of the user-data sets: switching, capturing and restoring isolated copy sets, and the owner file they
//! share. Redirect sets are already per profile and take no part in the copy loops.

use crate::context::profile::store::{self, MaterializedPath, OwnerFile, ProfileStore};
use crate::context::target::settings_file;
use crate::context::{Data, Settings, load_profiles};
use crate::domain::profiles::Profile;
use crate::domain::user_data::Set;
use crate::error::AppError;
use crate::games::GameDef;

use super::resolve::{prepare, seed_redirect, sets};

const OWNER: OwnerFile = OwnerFile {
    name: "user_data_owner.json",
    invalid_code: "user_data.owner_invalid",
    journal: "profile-switch-user-data.json",
};

/// Switches the profile whose copy-mode sets are materialized. Redirect sets are untouched; they are already per
/// profile. The first call seeds the target from the real data instead of removing it.
pub fn switch(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    target: &Profile,
) -> Result<(), AppError> {
    store::switch(
        &UserDataStore {
            game,
            settings,
            target: Some(target),
        },
        data,
        &target.id,
    )
}

/// Saves the current owner's copy-mode sets after a launch or before an undo.
pub fn capture_current(data: &Data, game: &GameDef, settings: &Settings) -> Result<(), AppError> {
    store::capture_current(
        &UserDataStore {
            game,
            settings,
            target: None,
        },
        data,
    )
}

/// Restores a profile's copy-mode sets, for the undo path. Redirect sets are left alone.
pub fn restore_profile(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    remove_missing: bool,
) -> Result<(), AppError> {
    store::restore_profile(
        &UserDataStore {
            game,
            settings,
            target: None,
        },
        data,
        profile,
        remove_missing,
    )
}

/// Captures the settings file after a deployment wrote it. Returns whether the file belongs to a user-data set; when it
/// does, the legacy `profile_data` store must not also copy it. A redirect set already holds the file in its upper
/// layer, so there is nothing to copy.
pub fn capture_settings(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
) -> Result<bool, AppError> {
    let Some(file) = settings_file(game, settings) else {
        return Ok(false);
    };
    let state = data.profile_state(&profile.id);
    let sets = sets(data, game, settings, profile, &state)?;
    let Some(set) = sets.iter().find(|set| set.relative(&file).is_some()) else {
        return Ok(false);
    };

    if set.is_redirected() || !set.isolated {
        return Ok(true);
    }

    let Some(relative) = set.relative(&file) else {
        return Ok(true);
    };
    store::capture(&[MaterializedPath {
        store: set.store.join(relative),
        real: file,
    }])?;
    Ok(true)
}

/// Whether the profile currently owns the copy stores. The owner file is shared by all copy-mode sets of a profile.
pub fn owner(data: &Data) -> Result<Option<String>, AppError> {
    OWNER.read(data)
}

/// The `user_data` sets as a store: only isolated copy sets take part, because a redirect already lives in the
/// profile's upper layer and a shared set is the real folder itself.
struct UserDataStore<'a> {
    game: &'a GameDef,
    settings: &'a Settings,
    /// The target profile when the caller already has it, so a switch does not read it again.
    target: Option<&'a Profile>,
}

impl UserDataStore<'_> {
    fn resolved(&self, data: &Data, profile: &str) -> Result<Vec<Set>, AppError> {
        let state = data.profile_state(profile);

        if let Some(target) = self.target.filter(|target| target.id == profile) {
            return sets(data, self.game, self.settings, target, &state);
        }

        let profiles = load_profiles(data)?;
        sets(
            data,
            self.game,
            self.settings,
            profiles.profile(profile)?,
            &state,
        )
    }
}

impl ProfileStore for UserDataStore<'_> {
    fn owner(&self) -> OwnerFile {
        OWNER
    }

    fn is_active(&self) -> bool {
        !self.game.user_data.is_empty()
    }

    fn entries(&self, data: &Data, profile: &str) -> Result<Vec<MaterializedPath>, AppError> {
        Ok(entries_of(&self.resolved(data, profile)?))
    }

    fn prepare(&self, data: &Data, profile: &str) -> Result<(), AppError> {
        let sets = self.resolved(data, profile)?;
        prepare(&sets)?;

        for set in sets.iter().filter(|set| set.is_redirected()) {
            seed_redirect(set)?;
        }

        Ok(())
    }

    fn exists(&self, data: &Data, profile: &str) -> Result<bool, AppError> {
        if self.target.is_some_and(|target| target.id == profile) {
            return Ok(true);
        }

        Ok(load_profiles(data)?.profile(profile).is_ok())
    }
}

/// The copy entries of the isolated copy sets. A redirect set already lives in the profile's upper layer and a shared
/// set is the real folder itself.
fn entries_of(sets: &[Set]) -> Vec<MaterializedPath> {
    sets.iter()
        .filter(|set| set.isolated && !set.is_redirected())
        .map(|set| MaterializedPath {
            real: set.real.clone(),
            store: set.store.clone(),
        })
        .collect()
}

/// The copy loop of the module tests, which check one set without a profile switch.
#[cfg(test)]
pub(super) fn capture_copy(sets: &[Set]) -> Result<(), AppError> {
    store::capture(&entries_of(sets))
}

/// The restore loop of the module tests, which check one set without a profile switch.
#[cfg(test)]
pub(super) fn restore_copy(sets: &[Set], remove_missing: bool) -> Result<(), AppError> {
    store::restore(&entries_of(sets), remove_missing)
}
