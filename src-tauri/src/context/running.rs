//! The games this process started and that are still running, shared by deployment, launch and the status.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static LIVE: Mutex<BTreeMap<String, Arc<AtomicBool>>> = Mutex::new(BTreeMap::new());

/// Whether Gobbo started this game and its wait thread has not cleared the flag yet.
pub fn started_here_and_running(game_id: &str) -> bool {
    LIVE.lock().is_ok_and(|live| {
        live.get(game_id)
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    })
}

/// Whether any game this process started is still running. A portable update replaces the program files, and a game
/// with the overlay hook loaded keeps those files open, so an update waits until none runs.
pub fn any_started_here_and_running() -> bool {
    LIVE.lock()
        .is_ok_and(|live| live.values().any(|flag| flag.load(Ordering::Acquire)))
}

/// Marks a game as started here and gives the flag its wait thread clears when the game exits.
pub fn mark_started(game_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(true));
    if let Ok(mut live) = LIVE.lock() {
        live.insert(game_id.to_string(), Arc::clone(&flag));
    }
    flag
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
}
