use crate::context::Data;
#[cfg(debug_assertions)]
use crate::context::load_settings;
use crate::launch::report;
#[cfg(debug_assertions)]
use crate::launch::start::overlay_session_state;
use crate::launch::watch;
use std::path::Path;
use std::path::PathBuf;

/// How many changes of the game folder the run report keeps. The total is kept too.
const MAX_GAME_CHANGES: usize = 200;

/// The game folder before a launch in an overlay session, where the folder must not change. Without a session there is
/// nothing to watch.
#[cfg_attr(
    not(debug_assertions),
    expect(
        clippy::missing_const_for_fn,
        reason = "the release branch is always `None`; the debug build reads the environment and snapshots the folder"
    )
)]
pub(super) fn watch_game_folder(data: &Data) -> Option<(PathBuf, watch::Snapshot)> {
    // A full snapshot of a large game before every launch is expensive. It is a diagnostic for layer leaks,
    // so it is compiled into debug builds only and off unless the developer asks for it.
    #[cfg(not(debug_assertions))]
    {
        let _ = data;
        None
    }
    #[cfg(debug_assertions)]
    {
        std::env::var_os("GOBBO_WATCH_GAME_FOLDER")?;

        if !matches!(overlay_session_state(data), Ok(Some(_))) {
            return None;
        }

        let root = PathBuf::from(load_settings(data).game_root.trim());

        if root.as_os_str().is_empty() {
            return None;
        }

        let before = watch::take(&root);
        Some((root, before))
    }
}

/// Compares the game folder with the record taken before the launch, and writes what is left into the run report.
/// An overlay run writes no file into the game folder, so every change is a leak.
pub(super) fn record_game_changes(data: &Data, root: &Path, before: &watch::Snapshot) {
    let after = watch::take(root);
    let found = watch::changes(before, &after);
    report::update(data, |r| {
        r.game_changes_total = u32::try_from(found.len()).unwrap_or(u32::MAX);
        r.game_changes = found.into_iter().take(MAX_GAME_CHANGES).collect();
    });
}
