//! The game's base roots, shared by the import, the reinstall and the archive report.

use super::plan::top_level_folders;
use crate::context::{Data, load_settings};
use crate::games::GameDef;

/// The top-level folders of the game's deployment root, for the wrapper rules. A missing game folder gives an empty
/// list; the version and same-name rules still recognise the common archive wrappers.
pub fn base_roots(data: &Data, game: &GameDef) -> Vec<String> {
    let settings = load_settings(data);
    match crate::context::target::deployment_root(data, game, &settings) {
        Ok(root) => top_level_folders(&root),
        Err(error) => {
            crate::infra::log::warn(format!("could not resolve the deployment root: {error}"));
            Vec::new()
        }
    }
}
