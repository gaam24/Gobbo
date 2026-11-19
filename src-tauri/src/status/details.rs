//! The per-mod queries: the profiles that use a mod, and what each of its files means in conflicts.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::deploy::target::build_plan_with_mode;
use crate::deploy::target::is_ready;
use crate::domain::pathkey::path_key;
use crate::domain::profiles::ProfilesView;
use crate::domain::resolver::{Conflict, PlanMode};
use crate::error::AppError;
use crate::error::Message;
use crate::launch::start::game_running;
use crate::library::LibraryView;
use crate::library::active_mods;
use crate::library::relations::SourceLookup;
use crate::status::steps::step_choices;
use crate::status::views::ConflictView;
use crate::status::views::FileState;
use crate::status::views::FileView;
use crate::status::views::ModDetails;
use crate::status::views::ModExtras;
use crate::status::views::ModUsage;
use crate::status::views::ModUsageProfile;
use std::collections::BTreeMap;

/// The profiles that list a mod and, among them, the ones whose prepared deployment contains it.
pub fn mod_usage(root: &Data, game_id: &str, id: &str) -> Result<ModUsage, AppError> {
    let Loaded {
        data,
        library,
        profiles,
        ..
    } = load_game(root, game_id)?;
    let prepared = data.prepared_states();
    let running = game_running(root, game_id)?;
    let last_launched = profiles.last_launched.as_deref();
    let mut usage = ModUsage {
        profiles: Vec::new(),
        prepared: false,
        running: false,
    };
    for profile in &profiles.profiles {
        let Some(entry) = profile.mods.iter().find(|entry| entry.id == id) else {
            continue;
        };
        let record = library.versions.get(&entry.folder);
        let version = record
            .and_then(|record| record.info.version.clone())
            .or_else(|| {
                record
                    .and_then(|record| library.archive_label_of(record))
                    .map(|label| label.version.clone())
            });
        let is_prepared = prepared.contains_key(&profile.id);
        let deployed = profile
            .deployed_mods
            .iter()
            .any(|deployed| deployed.id == id || deployed.folder == entry.folder);
        let is_running = running && is_prepared && last_launched == Some(profile.id.as_str());
        if deployed {
            usage.prepared = usage.prepared || is_prepared;
            usage.running = usage.running || is_running;
        }
        usage.profiles.push(ModUsageProfile {
            id: profile.id.clone(),
            name: profile.name.clone(),
            folder: entry.folder.clone(),
            version,
            enabled: entry.enabled,
            prepared: is_prepared && deployed,
            running: is_running && deployed,
        });
    }
    Ok(usage)
}

/// Every installed version of a mod family, newest first, for the table's version picker.
pub fn mod_versions(
    root: &Data,
    game_id: &str,
    folder: &str,
) -> Result<Vec<crate::status::views::VersionView>, AppError> {
    let Loaded {
        library, profiles, ..
    } = load_game(root, game_id)?;
    let record = library.version(folder)?;
    let view = LibraryView::new(&library);
    let profiles_view = ProfilesView::new(&profiles);
    Ok(super::mod_views::version_views(
        &library,
        &view,
        &profiles_view,
        &record.family,
    ))
}

/// What each mod file means in conflicts: it wins, it is overwritten, it has no conflict, or the mod is disabled.
pub fn mod_details(root: &Data, game_id: &str, folder: &str) -> Result<ModDetails, AppError> {
    let Loaded {
        game,
        data,
        settings,
        library,
        profiles,
        ..
    } = load_game(root, game_id)?;
    let listed = active_mods(&profiles, &library, &game);
    let record = library.version(folder)?;
    let family = record.family.clone();
    let enabled = listed.iter().any(|m| m.folder == folder && m.enabled);
    let view = LibraryView::new(&library);
    let lookup = SourceLookup::new(&view, &listed);
    let mod_steps = crate::launch::steps::load_mod_steps(&data)?;

    let deploy_plan = if is_ready(
        &game,
        &settings,
        profiles.active(),
        &data.profile_state(&profiles.active),
    ) {
        build_plan_with_mode(
            &data,
            &game,
            &settings,
            profiles.active(),
            &profiles.active().winners,
            &library.inputs(&listed),
            PlanMode::Summary,
        )
        .ok()
    } else {
        None
    };
    let by_path: BTreeMap<String, &Conflict> = deploy_plan
        .iter()
        .flat_map(|plan| plan.conflicts.iter())
        .map(|conflict| (path_key(&conflict.path), conflict))
        .collect();
    let name_of = |id: &str| crate::library::relations::family_name(library.as_ref(), id);
    let conflicts = deploy_plan
        .iter()
        .flat_map(|plan| plan.conflicts.iter())
        .filter(|conflict| conflict.winner == family || conflict.losers.contains(&family))
        .map(|conflict| ConflictView {
            path: conflict.path.clone(),
            winner: conflict.winner.clone(),
            losers: conflict.losers.clone(),
        })
        .collect();

    // A game whose deployment cannot share a file never has a conflict. The path key is then unnecessary, and a
    // mod with 100k files did not need 100k lowercased path allocations.
    let may_conflict = deploy_plan.is_some() && game.deploy.shares_files();
    let files = record
        .files
        .iter()
        .map(|path| {
            let (state, result) = if may_conflict {
                match by_path.get(&path_key(path)) {
                    Some(conflict) if conflict.winner == family => (
                        FileState::Win,
                        Message::new("file.wins_over").with(
                            "losers",
                            conflict
                                .losers
                                .iter()
                                .map(|loser| name_of(loser))
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ),
                    Some(conflict) => (
                        FileState::Lose,
                        Message::new("file.overridden_by")
                            .with("winner", name_of(&conflict.winner)),
                    ),
                    None if !enabled => (FileState::Disabled, Message::new("file.mod_disabled")),
                    None => (FileState::None, Message::new("file.no_conflict")),
                }
            } else if !enabled {
                (FileState::Disabled, Message::new("file.mod_disabled"))
            } else if deploy_plan.is_none() {
                (FileState::None, Message::new("file.needs_game_folder"))
            } else {
                (FileState::None, Message::new("file.no_conflict"))
            };
            FileView {
                path: path.clone(),
                state,
                result,
            }
        })
        .collect();

    let profiles_view = ProfilesView::new(&profiles);
    let versions = super::mod_views::version_views(&library, &view, &profiles_view, &family);
    let extras = super::mod_views::mod_extras(
        folder,
        library.families.get(&family),
        Some(record),
        &game,
        &library,
        &lookup,
        &mod_steps,
    );
    Ok(ModDetails {
        folder: folder.to_string(),
        info: record.info.clone(),
        sources: super::mod_views::source_details(
            library.families.get(&family),
            &game,
            &library,
            Some(record),
        ),
        versions,
        files,
        conflicts,
        steps: step_choices(&game, folder, &mod_steps),
        file_winners: game.deploy.shares_files(),
        extras,
    })
}

/// The page text, notes and relation details of several mods, for the rows the table shows. Rows that are no longer in
/// the library are skipped, so a stale viewport request is harmless.
pub fn mod_extras(
    root: &Data,
    game_id: &str,
    folders: &[String],
) -> Result<Vec<ModExtras>, AppError> {
    let Loaded {
        game,
        data,
        library,
        profiles,
        ..
    } = load_game(root, game_id)?;
    let listed = active_mods(&profiles, &library, &game);
    let view = LibraryView::new(&library);
    let lookup = SourceLookup::new(&view, &listed);
    let mod_steps = crate::launch::steps::load_mod_steps(&data)?;
    let mut extras = Vec::with_capacity(folders.len());
    for folder in folders {
        let Some(record) = library.versions.get(folder) else {
            continue;
        };
        extras.push(super::mod_views::mod_extras(
            folder,
            library.families.get(&record.family),
            Some(record),
            &game,
            &library,
            &lookup,
            &mod_steps,
        ));
    }
    Ok(extras)
}
