//! The mounts a game currently runs through, and the user-data layers no profile uses any more.
//! Cleaning is explicit: the window lists candidates, the user confirms them, and a second check rejects anything
//! that stopped being an orphan or belongs to a live session.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::context::profile::user_data;
use crate::context::{Data, load_profiles, load_settings, lock_state};
use crate::deploy::overlay;
use crate::error::{AppError, Message, io_at};
use crate::games::GameDef;
use crate::infra::fs as files;
use gobbo_overlay_core::MountSpec;

/// One prefix the game reads through a layer of the active deployment.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MountView {
    /// `game` for the game folder, or the id of a user-data set.
    pub id: String,
    /// The path the game sees.
    pub guest: String,
    /// The profile layer that receives the game's writes.
    pub upper: String,
    /// Whether a game process still holds this mount.
    pub active: bool,
}

/// A layer folder that no current profile or definition uses.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OrphanLayerView {
    /// The folder to remove, absolute and owned by this game's data.
    pub path: String,
    /// Why it is listed: `profile`, `set` or `recovery`.
    pub reason: String,
    pub size_bytes: u32,
}

/// What the diagnostics panel shows for one game.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MountDiagnostics {
    pub mounts: Vec<MountView>,
    pub orphans: Vec<OrphanLayerView>,
    /// Writes that landed in the game folder during an overlay session, from the last run's report.
    pub warnings: Vec<Message>,
}

/// Lists the mounts of every prepared deployment and the orphaned layers of one game. A read: it creates no folders.
pub fn diagnose(root: &Data, game_id: &str) -> Result<MountDiagnostics, AppError> {
    let game = crate::games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let profiles = load_profiles(&data)?;
    let game_root = PathBuf::from(settings.game_root.trim());
    let active = overlay::in_use(&game_root);

    let mut mounts = Vec::new();

    for (profile_id, state) in data.deployment_states().prepared {
        let Some(profile) = profiles
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
        else {
            continue;
        };
        mounts.push(MountView {
            id: MountSpec::GAME_ID.to_string(),
            guest: settings.game_root.clone(),
            upper: state.join("overwrite").display().to_string(),
            active,
        });

        for set in user_data::sets(&game, &settings, profile, &state)?
            .into_iter()
            .filter(|set| set.isolated)
        {
            mounts.push(MountView {
                id: set.id,
                guest: set.real.display().to_string(),
                upper: set.upper.display().to_string(),
                active,
            });
        }
    }

    mounts.sort_by(|left, right| left.id.cmp(&right.id));

    let mut warnings = Vec::new();

    if let Some(report) = crate::launch::report::last_run(root, game_id)
        && report.game_changes_total > 0
    {
        warnings.push(
            Message::new("diagnostics.writes_outside")
                .with("count", report.game_changes_total)
                .with("path", settings.game_root),
        );
    }

    Ok(MountDiagnostics {
        mounts,
        orphans: orphans(&data, &game, &profiles),
        warnings,
    })
}

/// Removes only the folders the caller confirmed and that are still orphans under this game's own state folder.
pub fn clean(root: &Data, game_id: &str, paths: &[String]) -> Result<(), AppError> {
    let _state = lock_state(&root.dir)?;
    let game = crate::games::load(game_id)?;
    let data = root.game(game_id);

    if crate::context::running::game_running(root, game_id)? {
        return Err(crate::error::game_running());
    }

    let settings = load_settings(&data);
    let game_root = PathBuf::from(settings.game_root.trim());

    if overlay::in_use(&game_root) {
        return Err(AppError::user("deploy.overlay_in_use").with("path", game_root.display()));
    }

    let profiles = load_profiles(&data)?;
    let allowed: BTreeSet<String> = orphans(&data, &game, &profiles)
        .into_iter()
        .map(|orphan| orphan.path)
        .collect();
    for path in paths {
        if !allowed.contains(path) {
            return Err(AppError::user("diagnostics.orphan_invalid").with("path", path));
        }
    }

    for path in paths {
        let path = Path::new(path);
        files::remove_path(path).map_err(io_at(path))?;
    }

    Ok(())
}

/// Orphaned layers below this game's `state/` folder. A removed game keeps its layers in `games/<id>.removed` and is
/// handled by the game settings, not here.
fn orphans(
    data: &Data,
    game: &GameDef,
    profiles: &crate::domain::profiles::Profiles,
) -> Vec<OrphanLayerView> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(data.dir.join("state")) else {
        return found;
    };
    let defined: BTreeSet<&str> = game.user_data.iter().map(|set| set.id.as_str()).collect();

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let profile_id = entry.file_name().to_string_lossy().into_owned();

        if !profiles
            .profiles
            .iter()
            .any(|profile| profile.id == profile_id)
        {
            push(&mut found, &path, "profile");
            continue;
        }

        let Ok(sets) = fs::read_dir(path.join("userdata")) else {
            continue;
        };

        for set in sets.flatten() {
            let set_path = set.path();

            if !set_path.is_dir() {
                continue;
            }

            let id = set.file_name().to_string_lossy().into_owned();

            if !defined.contains(id.as_str()) {
                push(&mut found, &set_path, "set");
                continue;
            }
            // A completed move-back keeps its old layer and its removed files as recovery copies. They are not
            // automatic orphans, but the user can clear them from here once they are no longer needed.
            for child in fs::read_dir(&set_path).into_iter().flatten().flatten() {
                let name = child.file_name().to_string_lossy().into_owned();

                if name.starts_with("upper.moved-") || name == "removed" {
                    push(&mut found, &child.path(), "recovery");
                }
            }
        }
    }

    found.sort_by(|left, right| left.path.cmp(&right.path));
    found
}

fn push(found: &mut Vec<OrphanLayerView>, path: &Path, reason: &str) {
    found.push(OrphanLayerView {
        path: path.display().to_string(),
        reason: reason.to_string(),
        size_bytes: u32::try_from(files::folder_size(path)).unwrap_or(u32::MAX),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Settings;
    use crate::test_support::{games as test_games, scratch, write};

    #[test]
    fn diagnostics_lists_orphans_and_clean_removes_only_confirmed_folders() {
        test_games::install();
        let base = scratch!("mount-diagnostics");
        let root = Data::new(base.join("data"));
        let data = root.game(test_games::USER_DATA_ID);
        crate::settings::config::save_settings(
            &data,
            test_games::USER_DATA_ID,
            Settings::default(),
        )
        .unwrap();
        let live_state = data.profile_state("p1");
        write(
            &live_state.join("userdata/documents/upper/file.txt"),
            "kept",
        );
        write(&live_state.join("userdata/old/upper/file.txt"), "stale");
        let gone = data.profile_state("gone");
        write(&gone.join("userdata/documents/upper/file.txt"), "orphan");
        write(
            &live_state.join("userdata/documents/upper.moved-123/file.txt"),
            "recovery",
        );

        let view = diagnose(&root, test_games::USER_DATA_ID).unwrap();
        let reasons: Vec<_> = view
            .orphans
            .iter()
            .map(|orphan| (orphan.path.clone(), orphan.reason.clone()))
            .collect();
        assert!(
            reasons.iter().any(|(path, reason)| {
                // The path comes from `display()`, so Windows uses backslashes here.
                path.replace('\\', "/").contains("userdata/old") && reason == "set"
            }),
            "{reasons:?}"
        );
        assert!(
            reasons
                .iter()
                .any(|(path, reason)| path.ends_with("gone") && reason == "profile"),
            "{reasons:?}"
        );
        assert!(
            reasons
                .iter()
                .any(|(path, reason)| path.contains("upper.moved-123") && reason == "recovery"),
            "{reasons:?}"
        );

        let old = view
            .orphans
            .iter()
            .find(|orphan| orphan.reason == "set")
            .unwrap()
            .path
            .clone();
        clean(
            &root,
            test_games::USER_DATA_ID,
            &[old.clone(), "/not/an/orphan".into()],
        )
        .unwrap_err();
        assert!(Path::new(&old).exists(), "a rejected list removes nothing");
        clean(&root, test_games::USER_DATA_ID, std::slice::from_ref(&old)).unwrap();
        assert!(!Path::new(&old).exists());
        assert!(
            live_state
                .join("userdata/documents/upper/file.txt")
                .exists()
        );
    }

    #[test]
    fn cleanup_refuses_while_the_game_runs() {
        test_games::install();
        let base = scratch!("mount-cleanup-running");
        let root = Data::new(base.join("data"));
        let data = root.game(test_games::USER_DATA_COPY_ID);
        crate::settings::config::save_settings(
            &data,
            test_games::USER_DATA_COPY_ID,
            Settings::default(),
        )
        .unwrap();
        let gone = data.profile_state("gone");
        write(&gone.join("userdata/documents/upper/file.txt"), "x");
        let orphan = diagnose(&root, test_games::USER_DATA_COPY_ID)
            .unwrap()
            .orphans[0]
            .path
            .clone();

        let running = crate::context::running::mark_started(test_games::USER_DATA_COPY_ID);
        let error = clean(
            &root,
            test_games::USER_DATA_COPY_ID,
            std::slice::from_ref(&orphan),
        )
        .unwrap_err();
        running.store(false, std::sync::atomic::Ordering::Release);

        assert_eq!(error.code, "deploy.game_running");
        assert!(Path::new(&orphan).exists());
    }
}
