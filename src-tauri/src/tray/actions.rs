//! What a tray menu click does. Every action either calls an existing use case or tells the window which screen to open;
//! the tray never reimplements a game rule.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_notification::NotificationExt;

use crate::context::Data;
use crate::error::AppError;
use crate::games::paths::home_dir;
use crate::status::stage::NextAction;
use crate::tray::labels::TrayLabels;

use super::state::TrayState;
use super::view::{Snapshot, snapshot};

/// Brings the main window back. The tray owns this so the webview needs no extra window permissions.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows the window and asks it to open one screen. The Rust side handles the window itself; the frontend only navigates.
fn open_view(app: &AppHandle, view: &str) {
    show_window(app);
    let _ = app.emit("tray-open-view", view);
}

fn log(error: &AppError) {
    crate::infra::log::warn(format!("tray action failed: {}", error.code));
}

fn blocking(app: AppHandle, work: impl FnOnce(&Data) -> Result<(), AppError> + Send + 'static) {
    drop(tauri::async_runtime::spawn_blocking(move || {
        let data = app.state::<Data>();

        if let Err(error) = work(&data) {
            log(&error);
        }
    }));
}

/// Applies the draft (when needed) and starts the game through the existing launch use case. A running game cannot be
/// launched again; the menu already shows Stop in that state.
fn play(app: &AppHandle, game: String, apply_draft: bool) {
    let handle = app.clone();
    let exit_game = game.clone();
    blocking(app.clone(), move |data| {
        let home = home_dir()?;
        crate::launch::start::launch_game(
            data,
            &game,
            None,
            apply_draft,
            &home,
            Some(Box::new(move |_code| {
                let _ = handle.emit("game-exited", exit_game.clone());
            })),
        )?;
        Ok(())
    });
}

fn stop(app: &AppHandle, game: String) {
    let handle = app.clone();
    blocking(app.clone(), move |data| {
        if crate::launch::start::stop_game(data, &game)? {
            crate::launch::report::mark_stopped(data, &game);
            let _ = handle.emit("game-exited", game.clone());
        }

        Ok(())
    });
}

fn undo(app: &AppHandle, game: String) {
    blocking(app.clone(), move |data| {
        crate::deploy::ops::undo_deployment(data, &game)?;
        Ok(())
    });
}

/// A step the window owns (folders, problems, import, enable) opens the right screen instead of trying to guess a fix.
fn open_step(app: &AppHandle, action: NextAction) {
    match action {
        NextAction::SetGameFolder | NextAction::SetSettingsFolder | NextAction::SetModsFolder => {
            open_view(app, "game-settings");
        }

        NextAction::FixProblems
        | NextAction::ImportMods
        | NextAction::EnableMods
        | NextAction::Apply
        | NextAction::Play => open_view(app, "mods"),
    }
}

fn quit(app: &AppHandle, snapshot: &Snapshot, labels: &TrayLabels) {
    let game = snapshot.game.as_ref();
    let running = game.is_some_and(|game| game.running);
    let busy_tasks = !snapshot.tasks.is_empty();

    if !running && !busy_tasks {
        app.exit(0);
        return;
    }

    let body = if running {
        labels.quit_game.clone()
    } else {
        labels.quit_tasks.clone()
    };
    let confirm = if running {
        labels.quit_stop.clone()
    } else {
        labels.quit_leave.clone()
    };
    let handle = app.clone();
    let stop_game = game.map(|game| game.id.clone());
    app.dialog()
        .message(body)
        .title(labels.quit_title.clone())
        .buttons(MessageDialogButtons::OkCancelCustom(
            confirm,
            labels.quit_cancel.clone(),
        ))
        .show(move |yes| {
            if !yes {
                return;
            }

            match stop_game {
                Some(game) => {
                    // Stop before exiting: leaving a game process behind would also leave the overlay mounted.
                    // When the stop fails, Gobbo stays open and says why, so the user can stop the game another way.
                    drop(tauri::async_runtime::spawn_blocking(move || {
                        let data = handle.state::<Data>();

                        match crate::launch::start::stop_game(&data, &game) {
                            Ok(_) => handle.exit(0),
                            Err(error) => {
                                crate::infra::log::warn(format!(
                                    "quit: the game was not stopped: {error}"
                                ));
                                handle.dialog().message(error.to_string()).show(|_| {});
                            }
                        }
                    }));
                }
                None => handle.exit(0),
            }
        });
}

fn notify_if_hidden(app: &AppHandle, task_state: &str, kind: &str, game: Option<&str>) {
    if task_state != "waiting" && task_state != "failed" {
        return;
    }

    let hidden = app.get_webview_window("main").is_none_or(|window| {
        !window.is_visible().unwrap_or(true) || window.is_minimized().unwrap_or(false)
    });

    if !hidden {
        return;
    }

    let labels = app.state::<TrayState>().labels();
    let template = if task_state == "waiting" {
        &labels.notice_waiting
    } else {
        &labels.notice_failed
    };
    let body = crate::tray::labels::fill(
        &crate::tray::labels::fill(
            template,
            "kind",
            &labels
                .kinds
                .get(kind)
                .cloned()
                .unwrap_or_else(|| kind.to_owned()),
        ),
        "game",
        game.unwrap_or(&labels.name),
    );
    let _ = app
        .notification()
        .builder()
        .title(labels.notice_title.clone())
        .body(body)
        .show();
}

/// The notification hook for one task event. Only a state change that needs the user produces a notice, and only when
/// the window is hidden or minimized.
pub fn task_event(app: &AppHandle, task: &crate::infra::tasks::TaskView) {
    notify_if_hidden(app, &task.state, &task.kind, task.game.as_deref());
}

/// Dispatches one native menu id.
pub fn handle(app: &AppHandle, id: &str) {
    if id.starts_with("tray.task:") {
        open_view(app, "working");
        return;
    }

    if let Some(profile) = id.strip_prefix("tray.profile:") {
        if let Some(game) = snapshot(app).ok().and_then(|snapshot| snapshot.game) {
            let id = profile.to_owned();
            let handle = app.clone();
            let switched = game.id.clone();
            blocking(app.clone(), move |data| {
                crate::profiles::actions::switch_profile(data, &game.id, &id)?;
                let _ = handle.emit("tray-profile-switched", switched.clone());
                Ok(())
            });
        }

        return;
    }

    if let Some(game_id) = id.strip_prefix("tray.game:") {
        let tray = app.state::<TrayState>();
        let (_, busy) = tray.context();
        tray.set_context(Some(game_id.to_owned()), busy);
        let _ = app.emit("tray-game-switched", game_id);
        show_window(app);
        return;
    }

    match id {
        "tray.open" => show_window(app),
        "tray.settings" => open_view(app, "app-settings"),
        "tray.open-working" => open_view(app, "working"),
        "tray.stop" => {
            if let Some(game) = snapshot(app).ok().and_then(|snapshot| snapshot.game) {
                stop(app, game.id);
            }
        }
        "tray.undo" => {
            if let Some(game) = snapshot(app).ok().and_then(|snapshot| snapshot.game) {
                undo(app, game.id);
            }
        }
        "tray.primary" => {
            if let Some(game) = snapshot(app).ok().and_then(|snapshot| snapshot.game) {
                match game.next {
                    NextAction::Apply => play(app, game.id, true),
                    NextAction::Play => play(app, game.id, false),
                    step => open_step(app, step),
                }
            }
        }
        "tray.quit" => {
            if let Ok(current) = snapshot(app) {
                let labels = app.state::<TrayState>().labels();
                quit(app, &current, &labels);
            }
        }
        _ => {}
    }
}
