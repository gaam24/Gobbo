use tauri::{AppHandle, Emitter, State};

use crate::commands::helpers::blocking;
use crate::context::Data;
use crate::error::AppError;
use crate::sources::update::{
    MANUAL_PAGE_EVENT, ManualPageRequest, ManualStep, ManualTarget, ManualUpdateFinish,
    ManualUpdateSessions, ManualUpdateView, UpdateChange,
};

/// Starts the backend-owned browser step for `targets` and answers the first view. The command asks the window to open
/// the first provider page with [`MANUAL_PAGE_EVENT`]; the window only renders the view and reports finished downloads.
#[tauri::command]
#[specta::specta]
pub fn start_manual_updates(
    app: AppHandle,
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
    targets: Vec<ManualTarget>,
) -> ManualUpdateView {
    let view = sessions.begin(game, targets);

    if let Some(page) = &view.current {
        let _ = app.emit(MANUAL_PAGE_EVENT, page);
    }

    view
}

/// The current view of one browser run. A window that reloads or switches back reads the run without losing it.
#[tauri::command]
#[specta::specta]
pub fn manual_update_status(
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
) -> ManualUpdateView {
    sessions.view(&game)
}

/// Reports a transient download step for a target that has not settled yet.
#[tauri::command]
#[specta::specta]
pub fn manual_update_step(
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
    id: String,
    step: ManualStep,
) -> ManualUpdateView {
    sessions.step(&game, &id, step)
}

/// The window reports that one target's download finished and imported. The backend selects the new version, settles
/// the target and asks for the next page. A selection failure is kept on the target as a translatable error code.
#[tauri::command]
#[specta::specta]
pub async fn manual_update_download_done(
    app: AppHandle,
    data: State<'_, Data>,
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
    id: String,
) -> Result<ManualUpdateView, AppError> {
    let target = sessions
        .view(&game)
        .targets
        .into_iter()
        .find(|target| target.id == id);
    let Some(target) = target else {
        return Ok(sessions.view(&game));
    };
    let root = data.inner().clone();
    let work_game = game.clone();
    let provider = target.provider.clone();
    let work_id = id.clone();
    let selected = blocking(move || {
        super::update::select_installed_update(&root, &work_game, &work_id, &provider)
    })
    .await;
    let (view, page) = match selected {
        Ok(()) => sessions.selected(
            &game,
            &id,
            UpdateChange {
                id: target.id.clone(),
                name: target.name.clone(),
                previous: target.folder.clone(),
            },
        ),
        Err(error) => (sessions.failed(&game, &id, error.code), None),
    };

    if let Some(page) = page {
        let _ = app.emit(MANUAL_PAGE_EVENT, &page);
    }

    Ok(view)
}

/// Skips the target the run is waiting on and asks for the next provider page.
#[tauri::command]
#[specta::specta]
pub fn manual_update_next(
    app: AppHandle,
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
) -> ManualUpdateView {
    let (view, page) = sessions.skip(&game);

    if let Some(page) = page {
        let _ = app.emit(MANUAL_PAGE_EVENT, &page);
    }

    view
}

/// Ends a browser run and hands back one undo for every version it selected, plus the failures it kept.
#[tauri::command]
#[specta::specta]
pub fn manual_update_finish(
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
) -> ManualUpdateFinish {
    sessions
        .finish(&game)
        .unwrap_or_else(|| ManualUpdateFinish {
            game,
            changes: Vec::new(),
            failures: Vec::new(),
        })
}

/// Stops a run without a finish handoff. Downloads already started import themselves, as before.
#[tauri::command]
#[specta::specta]
pub fn manual_update_cancel(sessions: State<'_, ManualUpdateSessions>, game: String) {
    sessions.cancel(&game);
}

/// Asks the window to reopen the current page after a game switch.
#[tauri::command]
#[specta::specta]
pub fn manual_update_reopen(
    app: AppHandle,
    sessions: State<'_, ManualUpdateSessions>,
    game: String,
) -> Option<ManualPageRequest> {
    let page = sessions.reopen(&game);

    if let Some(page) = &page {
        let _ = app.emit(MANUAL_PAGE_EVENT, page);
    }

    page
}
