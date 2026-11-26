//! The deployment states recorded in a game's profile folders.

use super::{DEPLOYMENT_FILE, Data, JOURNAL_FILE};
use crate::infra::deployment;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The two deployment states of one game, from one scan of the state folder.
#[derive(Debug, Default)]
pub struct DeploymentStates {
    pub prepared: BTreeMap<String, PathBuf>,
    pub incomplete: BTreeMap<String, PathBuf>,
}

/// Whether `state` holds a completion marker this build can trust, and so an apply that reached its end. A missing,
/// empty, damaged or newer marker is not trusted; the journal alone is an interrupted apply.
fn marker_applied(state: &Path) -> bool {
    deployment::read_complete(&state.join(DEPLOYMENT_FILE)).is_some()
}

impl Data {
    /// Every profile with a prepared deployment, by profile id. Only a finished apply counts: the journal alone is an
    /// interrupted apply, not a deployment the game may use.
    pub fn prepared_states(&self) -> BTreeMap<String, PathBuf> {
        self.deployment_states().prepared
    }

    /// Both state maps from one directory scan. A caller that needs both must not read the state folder twice: a
    /// deployment could finish between the scans and be missed.
    pub fn deployment_states(&self) -> super::DeploymentStates {
        let mut states = super::DeploymentStates::default();
        let Ok(entries) = fs::read_dir(self.dir.join("state")) else {
            return states;
        };

        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();

            if !path.is_dir() || !path.join(JOURNAL_FILE).is_file() {
                continue;
            }

            let id = entry.file_name().to_string_lossy().into_owned();

            if marker_applied(&path) {
                states.prepared.insert(id, path);
            } else {
                states.incomplete.insert(id, path);
            }
        }

        states
    }

    /// The state of the active profile, if it is prepared. A marker this build cannot trust is not prepared.
    pub fn deployed_state(&self) -> Option<PathBuf> {
        let state = self.profile_state(&self.active_profile_id());
        marker_applied(&state).then_some(state)
    }

    /// The state of the active profile when its journal exists but its completion marker does not, or the marker is
    /// empty, damaged or from a newer build. This is an apply that stopped in the middle; it may be undone, but it is
    /// never reported as applied.
    pub fn incomplete_state(&self) -> Option<PathBuf> {
        let state = self.profile_state(&self.active_profile_id());
        (state.join(JOURNAL_FILE).is_file() && !marker_applied(&state)).then_some(state)
    }
}
