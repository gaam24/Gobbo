//! Deployment and launch preparation: apply, undo, ensure and the running-game guards.

use std::sync::atomic::AtomicBool;

use crate::context::Data;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::load_settings;
use crate::context::load_stopped;
use crate::context::profile::lock_profiles;
use crate::context::profile::state as profile_state;
use crate::context::running::game_running_at;
use crate::deploy::ApplyView;
use crate::deploy::DeployError;
use crate::deploy::DeployProgress;
use crate::deploy::DeployStage;
use crate::deploy::apply_with_progress;
use crate::deploy::overlay;
use crate::deploy::plan_with_mode;
use crate::deploy::state_is_overlay;
use crate::deploy::target::journal_roots;
use crate::deploy::target::target_for;
use crate::deploy::undo;
use crate::deploy::undo_blocked_while_running;
use crate::domain::profiles::Entry as ProfileEntry;
use crate::domain::resolver::PlanMode;
use crate::error::AppError;
use crate::error::Message;
use crate::error::deployment_active;
use crate::error::game_running as game_is_running_error;
use crate::games;
use crate::import::IMPORT_CANCELLED;
use crate::infra::clock::now_seconds;
use crate::infra::tasks::stop_if;
use crate::library::active_mods;

pub fn apply_deployment(root: &Data, game_id: &str) -> Result<ApplyView, AppError> {
    apply_deployment_with_progress(root, game_id, &mut |_| {})
}

/// `apply_deployment` with a progress callback. The callback is called before the plan, once per phase and after each
/// placed file, so the window can show a real percent instead of a spinner.
pub fn apply_deployment_with_progress(
    root: &Data,
    game_id: &str,
    progress: &mut dyn FnMut(DeployProgress),
) -> Result<ApplyView, AppError> {
    apply_deployment_cancellable(root, game_id, progress, None)
}

/// `apply_deployment_with_progress`, with a cooperative cancellation flag.
///
/// The flag is checked after `gobbo.lock` is taken, before any state changes, and again just before the file-writing
/// pass. It is deliberately not checked between file writes: the journal is appended incrementally and the profile's
/// on-disk state is switched under the same lock, so a mid-pass stop would leave a torn journal. A cancel at the second
/// boundary stops before the first file is written; the next apply or undo repairs the prepared state. The global lock
/// order (`gobbo.lock` → index → profiles → reserved) is unchanged, and the task record reports the cancellation.
pub fn apply_deployment_cancellable(
    root: &Data,
    game_id: &str,
    progress: &mut dyn FnMut(DeployProgress),
    cancel: Option<&AtomicBool>,
) -> Result<ApplyView, AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    stop_if(cancel, IMPORT_CANCELLED)?;
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    progress(DeployProgress::at(DeployStage::Preparing, 0, 0));
    let profiles = load_profiles(&data)?;
    let running = game_running_at(game_id, &settings.game_root);
    // The settings file may be written by the deployment, so make the active profile the data owner first.
    // While the game runs we only prepare overlay layers and leave the real data alone.
    if !running {
        crate::snapshots::before_switch(root, &game, &settings, profiles.active())?;
        profile_state::switch(&data, &game, &settings, profiles.active())?;
    }

    let active = profiles.active.clone();
    let state = data.profile_state(&active);
    let roots = journal_roots(&data, &game, &settings, state.clone())?;
    let prepared = data.prepared_states();
    let active_prepared = prepared.contains_key(&active);
    let active_overlay = state_is_overlay(&state);

    if running {
        // A layer of this profile that the running game already uses cannot be rebuilt under it. The game's order
        // file is shared by every profile, so an apply that would write it also waits.
        let safe = state_is_overlay(&state)
            && !game.has_load_order()
            && !(active_prepared && profiles.last_launched.as_deref() == Some(active.as_str()));
        if !safe {
            return Err(game_is_running_error());
        }
    }

    overlay::make_room(&prepared, &active, game.has_load_order(), running, &roots)?;

    if active_prepared {
        // A layer that already matches the list and is what the game runs is the deployment the user asked for.
        if active_overlay && profiles.active().draft_changes() == 0 {
            return Err(deployment_active());
        }

        undo(&roots).map_err(AppError::from)?;
    }

    let target = target_for(&data, &game, &settings, state, profiles.active())?;
    // The index is read, refreshed and saved under its lock. The deployment below only reads it.
    let (library, mods) = {
        let _index = crate::library::lock_index();
        let mut library = load_library(&data, &game)?;
        let mods = active_mods(&profiles, &library, &game);
        library.refresh(&data.library(), &game, &mods)?;
        let library = data.save_library(library)?;
        (library, mods)
    };
    let deploy_plan = plan_with_mode(
        &game.deploy,
        &library.inputs(&mods),
        &profiles.active().winners,
        PlanMode::Full,
    );

    if !game.requires.is_empty() {
        let placed = deploy_plan.placed_keys();

        if let Some(file) = game
            .unmet_requirement(std::path::Path::new(&settings.game_root), |key| {
                placed.contains(key)
            })
        {
            return Err(AppError::user("deploy.requirement_missing").with("path", file));
        }
    }

    stop_if(cancel, IMPORT_CANCELLED)?;
    let total = deploy_plan.files.len();
    progress(DeployProgress::at(DeployStage::Writing, 0, total));
    let report = apply_with_progress(&deploy_plan, &data.library(), &target, progress)
        .map_err(AppError::from)?;
    // The deployment was built from this snapshot. A list edit made while it ran must survive, so re-read the profiles
    // under the lock and write only the deployment metadata; every other field comes from the fresh read.
    let deployed_id = profiles.active.clone();
    let deployed_profile = profiles.active();
    let deployed_mods: Vec<ProfileEntry> = profiles.active().enabled_mods();
    let winners = profiles.active().winners.clone();
    let deployed_at = now_seconds();
    let still_exists = {
        let _profiles = lock_profiles();
        let mut current = load_profiles(&data)?;
        let profile = current
            .profiles
            .iter_mut()
            .find(|profile| profile.id == deployed_id);
        let exists = profile.is_some();

        if let Some(profile) = profile {
            profile.mark_deployed(deployed_mods, winners, deployed_at);
        }

        data.save_profiles(&current)?;
        exists
    };

    if still_exists {
        profile_state::capture_settings(&data, &game, &settings, deployed_profile)?;
    }

    progress(DeployProgress::at(DeployStage::Done, total, total));
    Ok(ApplyView {
        files: report.files as u32,
    })
}

/// Before a launch: the viewed profile becomes the deployment the game starts with. A clean deployment is reused;
/// a draft is applied when `apply_draft` is true. Other overlay layers stay for fast switching.
pub fn ensure_active_deployed(
    root: &Data,
    game_id: &str,
    apply_draft: bool,
) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    let (game, data, settings) = load_stopped(root, game_id)?;
    let profiles = load_profiles(&data)?;
    crate::snapshots::before_switch(root, &game, &settings, profiles.active())?;
    profile_state::switch(&data, &game, &settings, profiles.active())?;
    let active = profiles.active.clone();
    let state = data.profile_state(&active);
    let prepared = data.prepared_states();

    if prepared.contains_key(&active)
        && state_is_overlay(&state)
        && (profiles.active().draft_changes() == 0 || !apply_draft)
    {
        return Ok(());
    }

    apply_deployment(root, game_id).map(|_| ())
}

/// Remembers which profile a launch uses. Undo protects this layer while the game runs.
pub fn record_launch(root: &Data, game_id: &str) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    games::load(game_id)?;
    let data = root.game(game_id);
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    profiles.last_launched = Some(profiles.active.clone());
    data.save_profiles(&profiles)
}

/// Undoes the active profile's deployment and returns the warnings from `deploy`. An overlay layer that the
/// running game uses must stay; another profile's overlay layer may be removed while the game runs.
pub fn undo_deployment(root: &Data, game_id: &str) -> Result<Vec<Message>, AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let profiles = load_profiles(&data)?;
    // An interrupted apply has a journal but no completion marker. It must still be undoable, and the undo is what
    // repairs it; only a missing journal means there is nothing to undo.
    let state = data
        .deployed_state()
        .or_else(|| data.incomplete_state())
        .ok_or(DeployError::NothingToUndo)?;
    let running = game_running_at(game_id, &settings.game_root);

    if running
        && undo_blocked_while_running(&state, &profiles.active, profiles.last_launched.as_deref())
    {
        return Err(game_is_running_error());
    }

    if !running {
        profile_state::capture_current(&data, &game, &settings)?;
    }

    let roots = journal_roots(&data, &game, &settings, state)?;
    let warnings = undo(&roots).map_err(AppError::from)?;

    if !running {
        profile_state::restore_profile(&data, &game, &settings, &profiles.active, false)?;
    }
    // Undo removes the deployment, so the ledger must stop saying it is applied. The list itself is unchanged and
    // becomes a draft the next apply can write.
    {
        let _profiles = lock_profiles();
        let mut current = load_profiles(&data)?;
        current.active_mut().mark_undeployed();
        current.last_launched = None;
        data.save_profiles(&current)?;
    }

    Ok(warnings)
}
