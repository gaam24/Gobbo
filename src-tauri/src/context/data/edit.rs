//! The write transactions of the context: the index and the profile list are read, changed and saved under the lock
//! that keeps two changes from interleaving.

use super::{Data, load_library, load_profiles};
use crate::context::library::{Library, lock_index};
use crate::context::profile::lock_profiles;
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::games::GameDef;

/// Changes the game's library index and saves it when `edit` succeeds. The index lock is held from the read to the
/// save, so two changes cannot interleave; a failed edit leaves the index as it was.
pub fn edit_library<T>(
    root: &Data,
    game_id: &str,
    edit: impl FnOnce(&Data, &GameDef, &mut Library) -> Result<T, AppError>,
) -> Result<T, AppError> {
    edit_library_with(root, game_id, edit, |_, _| Ok(()))
}

/// Changes one family of the library and saves the index. The family must be in the library.
pub fn edit_family<T>(
    root: &Data,
    game_id: &str,
    id: &str,
    edit: impl FnOnce(&mut crate::domain::library::Family) -> Result<T, AppError>,
) -> Result<T, AppError> {
    edit_library(root, game_id, |_, _, library| edit(library.family_mut(id)?))
}

/// `edit_library` with the archive cleanup every library mutation needs: copies no version uses are removed under the
/// same index lock.
pub fn edit_library_clean<T>(
    root: &Data,
    game_id: &str,
    edit: impl FnOnce(&Data, &GameDef, &mut Library) -> Result<T, AppError>,
) -> Result<T, AppError> {
    edit_library_with(root, game_id, edit, |data, library| {
        let kept = library.forget_unused_archives();
        crate::infra::store::clean_archives(&data.archives(), &kept)
    })
}

/// `edit_library` with a step between the edit and the save, for a caller that also cleans up what the edit changed.
pub fn edit_library_with<T>(
    root: &Data,
    game_id: &str,
    edit: impl FnOnce(&Data, &GameDef, &mut Library) -> Result<T, AppError>,
    before_save: impl FnOnce(&Data, &mut Library) -> Result<(), AppError>,
) -> Result<T, AppError> {
    let _index = lock_index();
    let game = root.definition(game_id)?;
    let data = root.game(game_id);
    let mut library = load_library(&data, &game)?;
    let value = edit(&data, &game, &mut library)?;
    before_save(&data, &mut library)?;
    data.save_library(library)?;
    Ok(value)
}

/// Changes the game's profiles and saves them. The definition is loaded once for the edit. A change to a profile's
/// mod list goes through `context::profile::history::save_profiles` instead, so the change can be undone.
pub fn edit_profiles<T>(
    root: &Data,
    game_id: &str,
    edit: impl FnOnce(&Data, &GameDef, &mut Profiles) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let _profiles = lock_profiles();
    let game = root.definition(game_id)?;
    let data = root.game(game_id);
    let mut profiles = load_profiles(&data)?;
    let value = edit(&data, &game, &mut profiles)?;
    data.save_profiles(&profiles)?;
    Ok(value)
}

/// The game's data, refused while a deployment is prepared. Changes that move files wait for it to be undone.
pub fn idle_data(root: &Data, game_id: &str) -> Result<Data, AppError> {
    let data = root.game(game_id);
    data.ensure_idle()?;
    Ok(data)
}

impl Data {
    /// Refuses a change that moves files while a deployment of the game is prepared or an apply was interrupted.
    pub fn ensure_idle(&self) -> Result<(), AppError> {
        let states = self.deployment_states();

        if states.prepared.is_empty() && states.incomplete.is_empty() {
            Ok(())
        } else {
            Err(crate::error::deployment_active())
        }
    }
}
