use std::path::Path;
use std::path::PathBuf;

use crate::context::Data;
use crate::context::data::JOURNAL_FILE;
use crate::deploy::journal::Journal;
use crate::error::AppError;

/// Whether the state folder holds a prepared overlay layer from an earlier deployment.
pub fn has_overlay_session(state: &Path) -> Result<bool, AppError> {
    Journal::new(state.join(JOURNAL_FILE))
        .has_layer_prepared()
        .map_err(AppError::from)
}

/// The state folder when an overlay session is deployed (the mod layer). Otherwise `None`.
pub fn overlay_session_state(data: &Data) -> Result<Option<PathBuf>, AppError> {
    let Some(state) = data.deployed_state() else {
        return Ok(None);
    };
    Ok(has_overlay_session(&state)?.then_some(state))
}
