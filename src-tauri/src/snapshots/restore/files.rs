//! Where a snapshot's sets are written back, and how a recreated profile is named.

use std::path::PathBuf;

use crate::domain::profiles::Profile;
use crate::error::AppError;
use crate::games::GameDef;
use crate::snapshots::manifest::{LEGACY_ID, Manifest, ManifestSet};
use crate::snapshots::{Settings, SnapshotScope, profile_state, user_data};

/// The current target of every set in the manifest, by set id. A set that the definition no longer declares, or one
/// that is no longer isolated, cannot be restored into a profile store.
pub(super) fn targets(
    scope: &SnapshotScope,
    game: &GameDef,
    settings: &Settings,
    manifest: &Manifest,
    profile: &Profile,
) -> Result<Vec<(ManifestSet, PathBuf)>, AppError> {
    let state = scope.game.profile_state(&profile.id);
    let sets = user_data::sets(&scope.game, game, settings, profile, &state)?;
    let mut targets = Vec::with_capacity(manifest.sets.len() + 1);

    for set in &manifest.sets {
        if set.id == LEGACY_ID {
            targets.push((set.clone(), profile_state::legacy_data(&state)));
            continue;
        }

        let Some(current) = sets.iter().find(|current| current.id == set.id) else {
            return Err(AppError::user("snapshot.set_missing").with("set", &set.id));
        };

        if !current.isolated {
            return Err(AppError::user("snapshot.set_shared").with("set", &set.id));
        }

        let target = if current.is_redirected() {
            current.upper.clone()
        } else {
            current.store.clone()
        };
        targets.push((set.clone(), target));
    }

    if let Some(overwrite) = &manifest.overwrite {
        targets.push((overwrite.clone(), profile_state::overwrite(&state)));
    }

    Ok(targets)
}

/// The name a recreated profile starts from. It is made unique when it is added.
pub(super) fn restored_name(name: &str) -> String {
    let name = name.trim();

    if name.is_empty() {
        "Restored profile".to_string()
    } else {
        name.chars().take(60).collect()
    }
}
