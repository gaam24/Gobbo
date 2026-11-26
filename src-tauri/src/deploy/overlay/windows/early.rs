//! What the loader sees before the hook: the stage 0 redirect and the folders an early write needs.

use std::collections::BTreeSet;
use std::path::Path;

use gobbo_overlay_core::Layers;
use gobbo_overlay_launch::stage0;

use super::Session;
use crate::deploy::error::DeployError;
use crate::error::FileError;

/// The early redirect of a session: the game folder served from the mod layer, and a write under the layer sent to
/// `upper`. `None` when the session has no layer.
pub(super) fn config(layers: &Layers) -> Result<Option<stage0::logic::Config>, DeployError> {
    let Some(staging) = layers.staging.as_ref() else {
        return Ok(None);
    };
    stage0::redirect_config(&layers.root, staging, &layers.upper)
        .map(Some)
        .map_err(|error| FileError::new(&layers.root, error).into())
}

/// Creates the `upper` folders that hold a layer DLL and their parents, so stage 0 can send an early write there even
/// before the hook has created a parent folder. A folder that cannot be made is left for the hook's own copy-up; the
/// launch is not stopped for it.
pub(super) fn seed_directories(session: &Session) {
    let mut seen = BTreeSet::new();

    for dll in super::super::session_layer_dlls(session) {
        let mut parent = Path::new(&dll).parent();

        while let Some(dir) = parent {
            if dir.as_os_str().is_empty() || !seen.insert(dir.to_path_buf()) {
                parent = dir.parent();
                continue;
            }

            let target = session.overwrite.join(dir);

            if let Err(error) = std::fs::create_dir_all(&target) {
                crate::infra::log::warn(format!(
                    "overlay: cannot prepare the early write folder {}: {error}",
                    target.display()
                ));
                break;
            }

            parent = dir.parent();
        }
    }
}
