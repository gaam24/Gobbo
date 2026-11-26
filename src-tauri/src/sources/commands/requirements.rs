use tauri::State;
use tauri::ipc::Channel;

use crate::commands::helpers::{channel_sink_with_progress, for_game, run_task};
use crate::context::Data;
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::infra::tasks::kind;
use crate::infra::tasks::{TaskFlags, TaskKey};
use crate::sources::requirements::{RequirementInstallView, RequirementPlan};
use crate::sources::update::UpdateProgress;
use crate::status::views::Status;

/// Installs or enables a requirement of the selected mod, then answers with the fresh status.
#[tauri::command]
#[specta::specta]
pub async fn install_requirement(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    source: SourceRef,
    progress: Channel<UpdateProgress>,
) -> Result<Status, AppError> {
    let key = TaskKey::requirement(&game);
    run_task(
        data,
        &tasks,
        key,
        game,
        kind::REQUIREMENT,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &UpdateProgress| step.percent);
            crate::sources::requirements::install_one_cancellable(
                root,
                game,
                source,
                Some(flag),
                &mut send,
            )?;
            crate::status::status(root, game)
        },
    )
    .await
}

/// The requirement closure of the selected mod version: direct requirements, transitive ones the library knows,
/// alternatives, cycles and the reason a step is blocked. Nothing is downloaded.
#[tauri::command]
#[specta::specta]
pub async fn preview_requirements(
    data: State<'_, Data>,
    game: String,
    folder: String,
) -> Result<RequirementPlan, AppError> {
    for_game(data, game, move |root, game| {
        crate::sources::requirements::plan(root, game, &folder)
    })
    .await
}

/// Installs every step the plan can install and saves the profile list once, so the whole closure is one undo.
#[tauri::command]
#[specta::specta]
pub async fn install_requirements(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    folder: String,
    progress: Channel<UpdateProgress>,
) -> Result<RequirementInstallView, AppError> {
    let key = TaskKey::requirement(&game);
    run_task(
        data,
        &tasks,
        key,
        game,
        kind::REQUIREMENT,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &UpdateProgress| step.percent);
            crate::sources::requirements::install_cancellable(
                root,
                game,
                &folder,
                Some(flag),
                &mut send,
            )
        },
    )
    .await
}
