//! Builds the status the window shows: the folders, the profiles, the mods with their conflicts, and the stage.

use crate::context::Data;
use crate::context::Settings;
use crate::context::data::load_profiles_shared;
use crate::context::load_library_shared;
use crate::context::load_settings;
use crate::context::profile::data as profile_data;
use crate::context::profile::history::history_state;
use crate::context::profile::lock_profiles;
use crate::context::profile::state as profile_state;
use crate::context::profile::user_data;
use crate::deploy::overwrite::has_overwrite_for;
use crate::deploy::target::build_plan_with_mode;
use crate::deploy::target::is_ready;
use crate::domain::library::{Library, LibraryView};
#[cfg(test)]
use crate::domain::profiles::entry::ProfileMod;
use crate::domain::profiles::{Profiles, ProfilesView};
use crate::domain::resolver::{ModInput, PlanMode, is_orderable};
use crate::error::AppError;
use crate::error::Message;
use crate::error::io_at;
use crate::games::GameDef;
use crate::games::prefix::effective_source;
use crate::library::active_mods;
use crate::library::relations::{SourceLookup, UnmetRequirements, relation_warnings_with};
use crate::library::sync::sync_profiles as sync_profile_lists;
use crate::status::bisect::bisect_view;
use crate::status::stage::{StageInput, game_stage};
use crate::status::views::{
    DeployedRow, ExecutableView, LoadOrderRange, ProfileView, Status, UserDataView,
};
use rustc_hash::FxHashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

mod mods;
mod problems;
mod readiness;
mod tools;

use mods::{build_mod_views, row_revision, subtitle_for};
use problems::index_problems;
use readiness::readiness;
#[cfg(test)]
pub use tools::tool_folder;
use tools::tool_views;

/// A reply counter, not game state: it orders status answers that a slow command and a quick edit may cross.
static NEXT_BUILD: AtomicU32 = AtomicU32::new(1);

pub(super) fn status_inner(
    root: &Data,
    game_id: &str,
    save_sync: bool,
) -> Result<Status, AppError> {
    let build = NEXT_BUILD.fetch_add(1, Ordering::Relaxed);
    let game = root.definition_shared(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let tools = tool_views(&game, &data, &settings);
    let library = load_library_shared(&data, &game)?;
    let view = LibraryView::new(&library);
    // The lock covers the read, the sync and the save of `synced_profiles`. The rest of the status is read-only: it
    // works on the snapshot above and must not hold the profiles lock through the folder checks, the plan and the views.
    let profiles = {
        let _profiles = save_sync.then(lock_profiles);
        synced_profiles(&data, &library, &view, save_sync)?
    };
    let profiles_view = ProfilesView::new(&profiles);
    let listed = active_mods(&profiles, &library, &game);
    let library_dir = data.library();
    // The library and archive folders belong to Gobbo, but creating them is a write: `status` is a pure read and must
    // leave the data folder untouched. The change path (`sync_profiles`, called when a game opens) creates them once,
    // and the readers below already handle a missing folder as an empty one.
    if save_sync {
        for dir in [data.library(), data.archives()] {
            if !dir.is_dir() {
                fs::create_dir_all(&dir).map_err(io_at(&dir))?;
            }
        }
    }

    // A version added or removed by hand shows at once, without scanning the contents (that is only a folder listing).
    let disk_folders = data.library_folders()?;
    let folder_check = data.folder_check(&library, &disk_folders);
    let mut problems = index_problems(&library, &listed, &folder_check);
    // The planner's inputs of the list, built once: the plan and the load order flags both read them.
    let inputs = library.inputs(&listed);
    let orderable: Vec<bool> = inputs
        .iter()
        .map(|input| is_orderable(&game.deploy, input))
        .collect();
    let mut plan = plan_outcome(&data, &game, &settings, &profiles, &inputs);
    problems.append(&mut plan.problems);

    let lookup = SourceLookup::new(&view, &listed);
    let requirements = UnmetRequirements::of(&library, &lookup, &listed);
    let library_context = super::mod_builders::LibraryContext {
        library: &library,
        view: &view,
        lookup: &lookup,
        orderable: &orderable,
    };
    let mods = build_mod_views(
        &listed,
        &game,
        &library_context,
        &profiles_view,
        &plan,
        &requirements,
    )?;
    let revisions = mods.iter().map(row_revision).collect();

    let prepared = data.prepared_states();
    let profile_views = profiles
        .profiles
        .iter()
        .map(|profile| ProfileView {
            id: profile.id.clone(),
            name: profile.name.clone(),
            mod_count: profile.mods.len() as u32,
            enabled_count: u32::try_from(profile.enabled_count()).unwrap_or(u32::MAX),
            active: profile.id == profiles.active,
            deployed: prepared.contains_key(&profile.id),
            has_user_data: user_data::state_dir(&data.profile_state(&profile.id)).exists(),
            collections: profile
                .collections
                .iter()
                .map(crate::domain::collection::CollectionBadge::from)
                .collect(),
        })
        .collect();

    let game_subtitle = subtitle_for(&settings.source, &game.detect.steam_app_ids);
    let runtime = crate::context::target::runtime(&game, &settings);
    let source = effective_source(&settings.source, &game.detect.steam_app_ids);
    let launch_hint = super::launch_hint(game_id, source);
    let game_profile_data = profile_data::game_paths(&data, &game, &settings);
    // One path for the view and for the user-data sets: the active profile's state folder is the same throughout.
    let active_state = data.profile_state(&profiles.active);
    let user_sets = user_data::sets(&data, &game, &settings, profiles.active(), &active_state)
        .unwrap_or_default();
    let settings_redirected =
        crate::deploy::target::settings_redirected(&game, &settings, &user_sets);
    let ready = readiness(&data, &game, &settings, settings_redirected);
    let user_data_views = user_sets
        .into_iter()
        .map(|set| UserDataView {
            available: set.real.exists() || set.real.parent().is_some_and(std::path::Path::exists),
            path: set.real.display().to_string(),
            upper: set.upper.display().to_string(),
            upper_exists: set.upper.exists(),
            id: set.id,
            kind: set.kind,
            mode: set.mode,
            isolated: set.isolated,
            inherit: set.inherit,
        })
        .collect();

    // The deployed flag is about the viewed profile: other profiles may keep their prepared layers.
    let active = profiles.active();
    let deployed = prepared.contains_key(&profiles.active);
    let draft_changes = if deployed { active.draft_changes() } else { 0 };
    let deployed_at = if deployed { active.deployed_at } else { None };
    let total = u32::try_from(mods.len()).unwrap_or(u32::MAX);
    let enabled =
        u32::try_from(mods.iter().filter(|view| view.enabled).count()).unwrap_or(u32::MAX);
    let input = StageInput {
        game_ready: ready.game_ready,
        needs_settings_dir: game.has_settings_file(),
        settings_ready: ready.settings_ready,
        needs_mods_root: game.has_external_root(),
        mods_ready: ready.mods_ready,
        problems: &problems,
        total,
        enabled,
        deployed,
        draft_changes,
        deployed_at,
    };
    let (stage, next_action) = game_stage(input);
    let blocker = crate::status::stage::blocker_of(&input);

    Ok(Status {
        game_name: game.name.clone(),
        game_subtitle,
        runtime,
        stage,
        next_action,
        blocker,
        settings,
        game_ready: ready.game_ready,
        settings_ready: ready.settings_ready,
        needs_mods_root: game.has_external_root(),
        mods_ready: ready.mods_ready,
        mods_root: ready.mods_root,
        needs_settings_dir: game.has_settings_file(),
        load_order: game.load_order.as_ref().map(|order| LoadOrderRange {
            priority_min: order.priority_min,
            priority_max: order.priority_max,
        }),
        deployed,
        draft_changes,
        deployed_at,
        deployed_list: if deployed {
            active
                .deployed_mods
                .iter()
                .map(|entry| DeployedRow {
                    id: entry.id.clone(),
                    folder: entry.folder.clone(),
                })
                .collect()
        } else {
            Vec::new()
        },
        profiles: profile_views,
        mods,
        revisions,
        build,
        patch: false,
        removed: Vec::new(),
        order: None,
        separators: active.separators.clone(),
        groups: active.groups.clone(),
        history: history_state(&data, &profiles.active),
        bisect: bisect_view(&data, &library, &profiles.active),
        has_overwrite: has_overwrite_for(&data, &profiles.active),
        conflict_count: plan.conflict_count,
        problems,
        warnings: relation_warnings_with(&library, &lookup, &listed, &requirements),
        library_dir: library_dir.display().to_string(),
        archives_dir: data.archives().display().to_string(),
        profile_dir: active_state
            .is_dir()
            .then(|| active_state.display().to_string()),
        overwrite_dir: profile_state::overwrite(&active_state)
            .display()
            .to_string(),
        game_profile_data,
        user_data: user_data_views,
        launch_hint,
        executables: game
            .executables
            .iter()
            .enumerate()
            .map(|(index, exe)| ExecutableView {
                name: exe.name.clone(),
                default: index == 0,
                args: exe.args.clone(),
            })
            .collect(),
        tools,
    })
}

/// The profiles the status shows: the cached list when it is already in step with `library`, otherwise a copy with the
/// in-memory sync applied. Only a sync with `save_sync` writes the list, so a read never does.
fn synced_profiles(
    data: &Data,
    library: &Arc<Library>,
    view: &LibraryView<'_>,
    save_sync: bool,
) -> Result<Arc<Profiles>, AppError> {
    let cached = load_profiles_shared(data)?;

    if data.profiles_in_step(&cached, library) {
        return Ok(cached);
    }

    let mut profiles = (*cached).clone();

    if !sync_profile_lists(&mut profiles, library, view) {
        data.remember_profiles_in_step(&cached, library);
        return Ok(cached);
    }

    let profiles = Arc::new(profiles);

    if !save_sync {
        return Ok(profiles);
    }

    data.save_profiles(&profiles)?;
    // The save put a new value in the cache; keep that pair so the next status reads it without copying and syncing.
    let saved = load_profiles_shared(data)?;
    data.remember_profiles_in_step(&saved, library);
    Ok(saved)
}

/// What the deployment plan of the shown profile says about the list: the conflicts per mod and the plan's problems.
/// Without a ready game and settings there is no plan, so nothing is counted.
#[derive(Default)]
pub(super) struct PlanOutcome {
    conflict_count: u32,
    pub(super) wins: FxHashMap<String, u32>,
    pub(super) loses: FxHashMap<String, u32>,
    problems: Vec<Message>,
}

fn plan_outcome(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profiles: &Profiles,
    inputs: &[ModInput<'_>],
) -> PlanOutcome {
    let mut outcome = PlanOutcome::default();

    if !is_ready(
        data,
        game,
        settings,
        profiles.active(),
        &data.profile_state(&profiles.active),
    ) {
        return outcome;
    }

    match build_plan_with_mode(
        data,
        game,
        settings,
        profiles.active(),
        &profiles.active().winners,
        inputs,
        PlanMode::Summary,
    ) {
        Ok(mut deploy_plan) => {
            let conflicts = std::mem::take(&mut deploy_plan.conflicts);
            let plan_problems = std::mem::take(&mut deploy_plan.problems);
            // Most definitions require no game file. Building a key for every planned file is only useful when one
            // does, and a large plan can hold thousands of paths.
            if !game.requires.is_empty() || game.install.is_some() {
                let placed = deploy_plan.placed_keys();

                if let Some(file) = game.unmet_requirement(Path::new(&settings.game_root), &placed)
                {
                    outcome
                        .problems
                        .push(Message::new("status.requirement_missing").with("path", file));
                }

                let deploy_root = crate::context::target::deployment_root(data, game, settings);

                if let Some(install) = deploy_root
                    .ok()
                    .and_then(|root| game.missing_loader(&root, &deploy_plan, &placed))
                {
                    outcome.problems.push(
                        Message::new(crate::status::stage::LOADER_MISSING)
                            .with("name", &install.name),
                    );
                }
            }

            outcome.conflict_count = u32::try_from(conflicts.len()).unwrap_or(u32::MAX);

            for conflict in conflicts {
                *outcome.wins.entry(conflict.winner).or_insert(0) += 1;
                for loser in conflict.losers {
                    *outcome.loses.entry(loser).or_insert(0) += 1;
                }
            }

            outcome.problems.extend(
                plan_problems
                    .iter()
                    .map(crate::domain::resolver::Problem::message),
            );
        }
        Err(error) => outcome.problems.push(error.into()),
    }

    outcome
}

#[cfg(test)]
mod tests;
