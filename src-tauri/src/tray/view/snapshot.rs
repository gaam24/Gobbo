//! The one part of the view that reads the app: the active game, the other games and the task registry, gathered in a
//! single snapshot.

use tauri::{AppHandle, Manager};

use crate::context::Data;
use crate::context::managed::managed_games;
use crate::error::AppError;
use crate::infra::tasks::TaskFlags;
use crate::status::stage::GameStage;
use crate::tray::state::TrayState;

use super::model::{Game, GameChoice, Snapshot, Stage};

/// Reads the active game, the other games and the task registry. The game id comes from the window's last report; before
/// the first report the first added game is used, so a tray created at startup is already useful.
pub fn snapshot(app: &AppHandle) -> Result<Snapshot, AppError> {
    let data = app.state::<Data>();
    let tasks = app.state::<TaskFlags>();
    let tray = app.state::<TrayState>();
    let (reported, busy) = tray.context();
    let managed = managed_games(&data)?;
    let games: Vec<GameChoice> = managed
        .iter()
        .map(|game| GameChoice {
            name: data
                .definition_shared(&game.id)
                .map_or_else(|_| game.id.clone(), |definition| definition.name.clone()),
            id: game.id.clone(),
        })
        .collect();
    let chosen = reported
        .filter(|id| games.iter().any(|game| &game.id == id))
        .or_else(|| games.first().map(|game| game.id.clone()));
    let game = chosen.and_then(|id| game_of(&data, &id).ok());
    Ok(Snapshot {
        game,
        games,
        tasks: tasks.list(),
        busy,
    })
}

fn game_of(data: &Data, id: &str) -> Result<Game, AppError> {
    let status = crate::status::status(data, id)?;
    let profile = status
        .profiles
        .iter()
        .find(|profile| profile.active)
        .map(|profile| profile.name.clone())
        .unwrap_or_default();
    let running = crate::context::running::game_running(data, id)?;
    Ok(Game {
        id: id.to_owned(),
        name: status.game_name,
        profile,
        profiles: status
            .profiles
            .into_iter()
            .map(|profile| (profile.id, profile.name))
            .collect(),
        next: status.next_action,
        stage: match status.stage {
            GameStage::Setup { .. } => Stage::Setup,
            GameStage::Blocked { .. } => Stage::Blocked,
            GameStage::Empty => Stage::Empty,
            GameStage::NoEnabled { .. } => Stage::NoEnabled,
            GameStage::Ready { .. } => Stage::Ready,
            GameStage::Draft { .. } => Stage::Draft,
            GameStage::Applied { .. } => Stage::Applied,
        },
        running,
        deployed: status.deployed,
        draft: status.draft_changes,
    })
}

// Shared by the status line and the task/menu labels: an unknown code falls back to the code itself.
pub(super) fn label_of(map: &std::collections::BTreeMap<String, String>, code: &str) -> String {
    map.get(code).cloned().unwrap_or_else(|| code.to_owned())
}
