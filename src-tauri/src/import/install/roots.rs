//! The game's base roots, shared by the import, the reinstall and the archive report.

use crate::context::{Data, load_settings};
use crate::games::GameDef;
use crate::import::install::top_level_folders;

/// The top-level folders of the game's deployment root, for the wrapper rules. A missing game folder gives an empty
/// list; the version and same-name rules still recognise the common archive wrappers.
pub fn base_roots(data: &Data, game: &GameDef) -> Vec<String> {
    let settings = load_settings(data);
    crate::context::target::deployment_root(data, game, &settings)
        .ok()
        .map(|root| top_level_folders(&root))
        .unwrap_or_default()
}
