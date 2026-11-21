//! What closing the main window does. Rust blocks every close and decides here: it performs the saved choice, or asks
//! the window to show the closing dialog. The tray owns this because it owns the app's lifetime while work continues.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::context::Data;
use crate::settings::config::{CloseAction, load_app_settings};

use super::state::TrayState;
use super::view::snapshot;

/// The event the window listens for to show the closing dialog.
const CLOSE_REQUESTED_EVENT: &str = "close-requested";

/// What the closing dialog needs to know to offer only the choices that work.
#[derive(Debug, Clone, Copy, Serialize)]
struct CloseRequest {
    /// The system shows Gobbo's tray icon, so hiding the window there keeps a way back.
    tray: bool,
    /// A game is running, so exiting stops it first.
    game_running: bool,
}

/// Whether the tray icon exists. A desktop without a tray (or one that refused the icon) has none.
pub fn available(app: &AppHandle) -> bool {
    app.try_state::<TrayState>()
        .is_some_and(|state| state.tray().is_some())
}

/// The action that can really be done. Hiding in the tray minimizes instead when there is no tray icon, so a saved choice
/// can never leave a window that cannot be found again.
pub const fn effective(action: CloseAction, tray: bool) -> CloseAction {
    match action {
        CloseAction::HideToTray if !tray => CloseAction::Minimize,
        other => other,
    }
}

/// Handles a close the user started on the window. The saved choice runs at once; `Ask` opens the dialog. A second close
/// while the dialog has not answered ends the app the safe way, so a window whose interface cannot answer is never stuck
/// open.
pub fn request(app: &AppHandle, data: &Data) {
    let tray = available(app);
    let action = effective(load_app_settings(data).close_action, tray);
    if action != CloseAction::Ask {
        apply(app, action);
        return;
    }

    let state = app.try_state::<TrayState>();
    if state.as_ref().is_some_and(|state| state.begin_close()) {
        apply(app, CloseAction::Exit);
        return;
    }

    let game_running = state.is_some()
        && snapshot(app)
            .ok()
            .and_then(|current| current.game)
            .is_some_and(|game| game.running);
    let _ = app.emit(CLOSE_REQUESTED_EVENT, CloseRequest { tray, game_running });
}

/// Performs one choice from the closing dialog. `Ask` is the dialog's Cancel: it only ends the pending close.
pub fn apply(app: &AppHandle, action: CloseAction) {
    if let Some(state) = app.try_state::<TrayState>() {
        state.end_close();
    }

    let window = app.get_webview_window("main");
    match effective(action, available(app)) {
        CloseAction::Ask => {}
        CloseAction::Exit => super::quit_app(app),
        CloseAction::Minimize => {
            if let Some(window) = window {
                let _ = window.minimize();
            }
        }
        CloseAction::HideToTray => {
            if let Some(window) = window {
                let _ = window.hide();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CloseAction, effective};

    #[test]
    fn hiding_without_a_tray_minimizes_instead() {
        assert_eq!(
            effective(CloseAction::HideToTray, false),
            CloseAction::Minimize
        );
        assert_eq!(
            effective(CloseAction::HideToTray, true),
            CloseAction::HideToTray
        );
    }

    #[test]
    fn the_other_choices_do_not_depend_on_the_tray() {
        for action in [CloseAction::Ask, CloseAction::Exit, CloseAction::Minimize] {
            assert_eq!(effective(action, false), action);
            assert_eq!(effective(action, true), action);
        }
    }
}
