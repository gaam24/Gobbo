//! What a deployment would do, before it does anything: the mods it adds, removes and changes compared with what is
//! deployed now, the files it places, the conflicts, and the problems that would stop it. Nothing is written.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::deploy::overlay;
use crate::deploy::target::build_plan_with_mode;
use crate::domain::library::LibraryView;
use crate::domain::profiles::ListedMod;
use crate::domain::resolver::PlanMode;
use crate::error::AppError;
use crate::error::Message;
use crate::library::active_mods;
use crate::library::relations::{SourceLookup, relation_warnings};
use serde::Serialize;

/// What applying the active list would do, compared with the deployment the game has now.
#[derive(Debug, Serialize, specta::Type)]
pub struct DeployPreview {
    /// The active profile is deployed now, so the deployment replaces it.
    pub replaces: bool,
    /// Files the deployment places.
    pub files: u32,
    /// Paths that several enabled mods write; one copy of each wins.
    pub conflicts: u32,
    /// Mod names, compared with the deployed list (or with nothing, when nothing is deployed).
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Mods deployed with another version now.
    pub updated: Vec<String>,
    /// Mods whose place in the list changed.
    pub moved: u32,
    /// File winners chosen or cleared since the last deployment.
    pub winners_changed: u32,
    /// What stops the deployment. Empty when it can run.
    pub problems: Vec<Message>,
    /// What does not stop it but is worth a look: missing requirements, mods that do not work together.
    pub warnings: Vec<Message>,
}

/// Builds the deployment preview for the window, without writing any file.
pub fn preview_deployment(root: &Data, game_id: &str) -> Result<DeployPreview, AppError> {
    let Loaded {
        game,
        data,
        settings,
        library,
        profiles,
        ..
    } = load_game(root, game_id)?;
    let profile = profiles.active();
    let replaces = data.prepared_states().contains_key(&profiles.active);
    let view = LibraryView::new(&library);
    let listed = active_mods(&profiles, &library, &game);
    let lookup = SourceLookup::new(&view, &listed);

    let mut preview = DeployPreview {
        replaces,
        files: 0,
        conflicts: 0,
        added: Vec::new(),
        removed: Vec::new(),
        updated: Vec::new(),
        moved: 0,
        winners_changed: if replaces {
            profile.winner_changes()
        } else {
            0
        },
        problems: Vec::new(),
        warnings: relation_warnings(&library, &view, &listed),
    };

    match build_plan_with_mode(
        &data,
        &game,
        &settings,
        profiles.active(),
        &profile.winners,
        &library.inputs(&listed),
        PlanMode::Summary,
    ) {
        Ok(plan) => {
            preview
                .warnings
                .extend(overlay::reserved_dll_warnings(&plan));
            preview.files = plan.file_count;
            preview.conflicts = u32::try_from(plan.conflicts.len()).unwrap_or(u32::MAX);
            preview.problems.extend(
                plan.problems
                    .iter()
                    .map(crate::domain::resolver::Problem::message),
            );
        }
        Err(error) => preview.problems.push(error.into()),
    }

    if !listed.iter().any(|entry| entry.enabled) {
        preview.problems.push(Message::new("preview.no_mods"));
    }

    let now = profile.enabled_mods();
    let before: &[ListedMod] = if replaces {
        &profile.deployed_mods
    } else {
        &[]
    };
    let difference = crate::domain::profiles::diff::diff(before, &now);
    preview.added = difference
        .added
        .iter()
        .map(|entry| lookup.family_name(&entry.id).to_string())
        .collect();
    preview.updated = difference
        .updated
        .iter()
        .map(|(_, entry)| lookup.family_name(&entry.id).to_string())
        .collect();
    preview.moved = difference.moved;
    preview.removed = difference
        .removed
        .iter()
        .map(|entry| lookup.family_name(&entry.id).to_string())
        .collect();
    Ok(preview)
}
