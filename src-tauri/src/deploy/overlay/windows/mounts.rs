//! The mount file and mount set the Windows hook reads before the game starts.

use std::path::{Path, PathBuf};

use gobbo_overlay_core::{MountConfig, MountSpec};

use crate::deploy::error::DeployError;

use super::Session;

/// The path of the mount file in a profile's state folder.
#[cfg(windows)]
pub fn mounts_path(state: &Path) -> PathBuf {
    state.join(crate::infra::deployment::MOUNTS_FILE)
}

/// The mount set of this session: the game folder plus every isolated redirect. The game is always the first mount.
/// A set the hook could not use safely is an unavailable overlay, not a file the game might run against.
#[cfg(windows)]
pub fn mount_config(session: &Session) -> Result<MountConfig, DeployError> {
    let mut mounts = vec![MountSpec {
        id: MountSpec::GAME_ID.to_string(),
        guest: session.view_root().to_path_buf(),
        root: session.view_root().to_path_buf(),
        staging: Some(session.mods.clone()),
        upper: session.overwrite.clone(),
    }];
    mounts.extend(session.redirects().map(|set| MountSpec {
        id: set.id.clone(),
        guest: set.real.clone(),
        root: set.base.clone(),
        staging: None,
        upper: set.upper.clone(),
    }));
    MountConfig::try_new(MountConfig::VERSION, mounts).map_err(|error| {
        DeployError::OverlayUnavailable {
            detail: error.to_string(),
        }
    })
}
