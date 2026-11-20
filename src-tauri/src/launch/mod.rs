//! Starting the game: through Proton from Steam (Linux), natively, or in the overlay layer, plus the
//! running-check the poller uses.

pub mod changes;
pub mod commands;
pub mod executable;
pub mod logs;
pub mod output;
mod pipeline;
#[cfg(not(windows))]
pub mod proton;
pub mod report;
pub mod start;
pub mod steps;
pub mod tools;
pub mod watch;

use std::path::Path;

use crate::context::Data;
use crate::context::Settings;
use crate::context::load_profiles;
use crate::context::profile::state as profile_state;
use crate::context::profile::user_data;
use crate::deploy::overlay::Session;
use crate::deploy::target::deployment_root;
use crate::error::AppError;
use crate::games::GameDef;
use crate::infra::clock::now_seconds;

/// The overlay session for a deployed state: the active profile's user-data sets and the deployment root, with the
/// game folder as the fallback root. The caller decides whether a session is wanted at all.
pub fn overlay_session(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    state: &Path,
    game_root: &Path,
) -> Result<Session, AppError> {
    let profiles = load_profiles(data)?;
    let sets = user_data::sets(
        game,
        settings,
        profiles.active(),
        &data.profile_state(&profiles.active),
    )?;
    let root = deployment_root(game, settings).unwrap_or_else(|_| game_root.to_path_buf());
    Ok(Session::with_roots(state, &root, game_root, sets))
}

/// Saves the profile's data after the game or a tool exits, and closes the launch report. A failure to save data
/// is reported on stderr; it must not hide the game's exit. `state_root` is the app data root, because `data` is a
/// game-level handle and its own folder does not hold `gobbo.lock`.
fn finish_launch(data: &Data, game: &GameDef, settings: &Settings, state_root: &Path) {
    // The wait thread runs after the command returned, so no lock from the launch is still held: take the one that
    // serializes the real profile files against an apply, undo or restore in this or another Gobbo process.
    let saved = crate::context::lock_state(state_root)
        .and_then(|_state| profile_state::capture_current(data, game, settings));
    if let Err(error) = saved {
        crate::infra::log::error(format!("could not save the profile state: {error}"));
    }

    report::update(data, |report| report.finished_at = Some(now_seconds()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::naming;
    use crate::test_support::{scratch, write};
    use std::fs;

    #[test]
    fn the_post_exit_save_locks_the_app_root_and_reenters_it_on_one_thread() {
        let root = Data::new(scratch!("finish-launch-lock"));
        let game = crate::test_support::games::slot();
        let data = root.game(&game.id);
        let real = root.dir.join("outside/save.sav");
        write(&real, "after the game");
        let owner = data.dir.join("state/data_owner.json");
        fs::create_dir_all(owner.parent().unwrap()).unwrap();
        fs::write(&owner, r#"{"profile":"p1"}"#).unwrap();
        let settings = Settings {
            game_root: root.dir.join("game").display().to_string(),
            profile_data: vec![real.display().to_string()],
            ..Settings::default()
        };
        let saved = data
            .profile_state("p1")
            .join("data")
            .join(naming::path_hash(&real.display().to_string()));
        let _held = crate::context::lock_state(&root.dir).unwrap();

        // A same-thread call re-enters the root lock and saves the data.
        finish_launch(&data, &game, &settings, &root.dir);
        assert!(saved.is_file());
        fs::remove_file(&saved).unwrap();

        // The app root is the lock, not the game folder: another thread sees it held and writes nothing.
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                finish_launch(&data, &game, &settings, &root.dir);
                sender.send(()).unwrap();
            });
        });
        receiver.recv().unwrap();
        assert!(
            !saved.exists(),
            "the blocked save must not write the profile store"
        );
    }
}
