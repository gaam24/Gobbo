//! Starting the game: through Proton from Steam or in a prefix Gobbo owns (Linux), natively, or in the overlay layer,
//! plus the running-check the poller uses.

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

/// Starts a game through the one launch use case and does the window work that follows it: `exit_notice` tells the
/// window when the game ended and `after_start` runs once it started. Both closures carry the caller's app handle, so
/// the command and the tray share the sequence without either owning a copy of it.
pub fn launch_game_with_notice(
    root: &Data,
    game_id: &str,
    executable: Option<&str>,
    apply_draft: bool,
    home: &Path,
    exit_notice: impl FnOnce(Option<i32>) + Send + 'static,
    after_start: impl FnOnce(),
) -> Result<bool, AppError> {
    let launched = start::launch_game(
        root,
        game_id,
        executable,
        apply_draft,
        home,
        Some(Box::new(exit_notice)),
    )?;
    after_start();
    Ok(launched)
}

/// Stops a game through the one stop use case and records that the user asked, so the last run is not treated as a
/// crash. `stopped` tells the caller which case it was; a game that was not running leaves the last run untouched.
pub fn stop_game_with_notice(
    root: &Data,
    game_id: &str,
    stopped: impl FnOnce(),
) -> Result<bool, AppError> {
    let did_stop = start::stop_game(root, game_id)?;
    if did_stop {
        report::mark_stopped(root, game_id);
        stopped();
    }
    Ok(did_stop)
}

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
        data,
        game,
        settings,
        profiles.active(),
        &data.profile_state(&profiles.active),
    )?;
    let root = deployment_root(data, game, settings).unwrap_or_else(|_| game_root.to_path_buf());
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
