//! The games this process started and that are still running, shared by deployment, launch and the status.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// The flags of one launch Gobbo started. `running` clears when its last process has ended; `finished` is set once
/// the post-exit steps and the save of the profile data are done.
struct Live {
    running: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

static LIVE: Mutex<BTreeMap<String, Live>> = Mutex::new(BTreeMap::new());

/// The live table, even after a panic while holding it: a poisoned mutex holds no partial state here, so a running
/// game must not look stopped and a mark must not be dropped.
fn live() -> MutexGuard<'static, BTreeMap<String, Live>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How often `wait_finished` looks at the completion flag again.
const FINISHED_POLL: Duration = Duration::from_millis(20);

/// The flags of one launch Gobbo started. They belong to this launch alone: relaunching the game replaces the live
/// entry, so finishing this run can never mark the next one finished.
pub struct Started {
    /// Clears when the last process of this launch has ended.
    pub running: Arc<AtomicBool>,
    /// Set once the post-exit work and the save of the profile data are done.
    pub finished: Arc<AtomicBool>,
}

/// Whether Gobbo started this game and its wait thread has not cleared the flag yet.
pub fn started_here_and_running(game_id: &str) -> bool {
    live()
        .get(game_id)
        .is_some_and(|entry| entry.running.load(Ordering::Acquire))
}

/// Whether any game this process started is still running. A portable update replaces the program files, and a game
/// with the overlay hook loaded keeps those files open, so an update waits until none runs.
pub fn any_started_here_and_running() -> bool {
    live()
        .values()
        .any(|entry| entry.running.load(Ordering::Acquire))
}

/// Marks a game as started here and returns both flags of this launch. The wait thread clears `running` when the game
/// exits and sets `finished` after the post-exit work. A later launch of the same game replaces the live entry, so
/// `wait_finished` follows the newest run.
pub fn mark_started_with_finished(game_id: &str) -> Started {
    let entry = Live {
        running: Arc::new(AtomicBool::new(true)),
        finished: Arc::new(AtomicBool::new(false)),
    };
    let started = Started {
        running: Arc::clone(&entry.running),
        finished: Arc::clone(&entry.finished),
    };

    live().insert(game_id.to_string(), entry);

    started
}

/// Marks a game as started here and gives the flag its wait thread clears when the game exits.
#[cfg(test)]
pub fn mark_started(game_id: &str) -> Arc<AtomicBool> {
    mark_started_with_finished(game_id).running
}

/// Marks the post-exit work of a game as complete, so a caller waiting to quit knows its saves were captured. A game
/// Gobbo never started has no entry and needs no mark. The pipeline sets its own launch's flag instead of calling this.
#[cfg(test)]
pub fn mark_finished(game_id: &str) {
    if let Some(entry) = live().get(game_id) {
        entry.finished.store(true, Ordering::Release);
    }
}

/// Waits up to `timeout` for a game's post-exit work to finish. Returns `true` once it is done, or at once when Gobbo
/// never started that game; `false` means the timeout expired first.
pub fn wait_finished(game_id: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let done = live()
            .get(game_id)
            .is_none_or(|entry| entry.finished.load(Ordering::Acquire));
        if done || Instant::now() >= deadline {
            return done;
        }
        thread::sleep(FINISHED_POLL);
    }
}

/// Whether the game of this id runs now, from Gobbo or from Steam. The game folder comes from the settings. Only the id
/// is checked here: reading and checking the whole definition would cost about 80 µs for one answer.
pub fn game_running(
    root: &crate::context::Data,
    game_id: &str,
) -> Result<bool, crate::error::AppError> {
    if !crate::games::is_valid_id(game_id) {
        return Err(crate::error::AppError::user("games.unknown").with("id", game_id));
    }

    let settings = crate::context::load_settings(&root.game(game_id));
    Ok(game_running_at(game_id, &settings.game_root))
}

/// Whether the game is running now, from Gobbo or from outside it. The process list is searched only when Gobbo did
/// not start this game itself.
pub fn game_running_at(game_id: &str, game_root: &str) -> bool {
    if started_here_and_running(game_id) {
        return true;
    }

    let game_root = game_root.trim();
    !game_root.is_empty() && crate::platform::game_is_running(Path::new(game_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_started_game_counts_as_running_until_its_flag_clears() {
        let flag = mark_started("running-test-game");
        assert!(started_here_and_running("running-test-game"));
        flag.store(false, Ordering::Release);
        assert!(!started_here_and_running("running-test-game"));
    }

    #[test]
    fn the_post_exit_wait_ends_when_the_game_is_marked_finished() {
        let game = "running-finished-test-game";
        let flag = mark_started(game);
        assert!(!wait_finished(game, Duration::from_millis(20)));
        mark_finished(game);
        assert!(wait_finished(game, Duration::ZERO));
        flag.store(false, Ordering::Release);
    }

    #[test]
    fn a_game_gobbo_never_started_needs_no_post_exit_wait() {
        assert!(wait_finished(
            "running-not-started-test-game",
            Duration::ZERO
        ));
    }

    #[test]
    fn a_relaunch_has_its_own_finished_flag() {
        let game = "running-relaunch-test-game";
        let first = mark_started_with_finished(game);
        let second = mark_started_with_finished(game);
        assert!(started_here_and_running(game));

        // Only the newest entry is waited on: the old run finishing must not release a quit waiting on the new one.
        first.finished.store(true, Ordering::Release);
        assert!(!wait_finished(game, Duration::ZERO));
        assert!(first.running.load(Ordering::Acquire));

        second.finished.store(true, Ordering::Release);
        assert!(wait_finished(game, Duration::ZERO));

        first.running.store(false, Ordering::Release);
        second.running.store(false, Ordering::Release);
    }
}
