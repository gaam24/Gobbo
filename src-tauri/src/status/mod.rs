//! The game's status for the window and the read-only queries that go with it. The builder is in `build`, the view
//! models in `views`, and the per-mod queries in `details`.

mod bisect;
mod build;
pub mod details;
pub mod health;
mod mod_views;
pub mod preflight;
pub mod stage;
pub mod steps;
pub mod views;

use crate::context::Data;
use crate::error::AppError;
use build::PlanOutcome;
use rustc_hash::{FxHashMap, FxHashSet};
use views::Status;

/// The command for Steam launch options: starts the game through Gobbo, that is, in the overlay layer.
pub fn launch_hint(game_id: &str) -> String {
    if !crate::platform::supports_overlay() {
        return String::new();
    }
    let exe = std::env::current_exe()
        .map_or_else(|_| "gobbo".to_string(), |path| path.display().to_string());
    format!("\"{exe}\" run {game_id} -- %command%")
}

/// Keeps only the rows whose revision the window does not have, plus the removals and the new order. The window sends
/// the revisions it holds with a list edit; a mismatch (or a window without revisions) sends the whole list again.
pub fn patch(mut status: views::Status, known: &views::KnownRows) -> views::Status {
    if known.ids.len() != known.revisions.len() {
        return status;
    }
    let held: FxHashMap<&str, &str> = known
        .ids
        .iter()
        .zip(&known.revisions)
        .map(|(id, revision)| (id.as_str(), revision.as_str()))
        .collect();
    let mods = std::mem::take(&mut status.mods);
    let revisions = std::mem::take(&mut status.revisions);
    let mut ids = Vec::with_capacity(mods.len());
    let mut changed = Vec::new();
    let mut changed_revisions = Vec::new();
    for (view, revision) in mods.into_iter().zip(revisions) {
        ids.push(view.id.clone());
        if held.get(view.id.as_str()) != Some(&revision.as_str()) {
            changed.push(view);
            changed_revisions.push(revision);
        }
    }
    let current: FxHashSet<&str> = ids.iter().map(String::as_str).collect();
    let removed: Vec<String> = known
        .ids
        .iter()
        .filter(|id| !current.contains(id.as_str()))
        .cloned()
        .collect();
    status.order = (known.ids != ids).then_some(ids);
    status.mods = changed;
    status.revisions = changed_revisions;
    status.removed = removed;
    status.patch = true;
    status
}

/// The game's status: the folders the game needs, the profiles, the mods with their conflicts and the problems the
/// window reports. It does not scan folders, so the app starts fast. Each part is built by a helper in `build`, so this
/// reads as the list of what the window shows. A read never writes the profile list.
pub fn status(root: &Data, game_id: &str) -> Result<Status, AppError> {
    build::status_inner(root, game_id, false)
}

/// The same status after bringing every profile in step with the library and saving the profiles. The window calls it
/// when a change added library entries without a profile entry (an import), or when it opens a game whose files may have
/// changed outside Gobbo. `status` itself stays a query.
pub fn sync_profiles(root: &Data, game_id: &str) -> Result<Status, AppError> {
    build::status_inner(root, game_id, true)
}
