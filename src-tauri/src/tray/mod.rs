//! The native system tray: a status surface for the work that continues when the window is hidden. The menu is built
//! from the same status, task registry and game runtime the window reads, so it never becomes a second source of truth.

mod actions;
pub mod close;
pub mod commands;
mod icons;
mod labels;
mod menu;
mod notifications;
mod state;
mod view;

pub use actions::quit_app;
pub use notifications::hidden_to_tray;
use state::TrayState;

use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager};

use crate::context::Data;
use crate::error::AppError;
use crate::infra::tasks::TaskView;
use crate::settings::config::load_app_settings;

const TRAY_ID: &str = "gobbo-main";

/// Creates the icon, installs its handlers and subscribes to the events that change what it shows. A failure is logged
/// by the caller: a tray is useful, but a missing one never stops the app.
pub fn init(app: &AppHandle) -> Result<(), AppError> {
    // The state is managed before the icon is built: closing and quitting read the game state through it, so a desktop
    // without a tray still asks before stopping a running game. Only the icon is missing then.
    app.manage(TrayState::new());
    notifications::init_action_handler(app);
    let state = app.state::<TrayState>();
    let labels = state.labels();
    let tooltip = icons::load(view::IconState::Idle)
        .map_err(AppError::unexpected)
        .and_then(|icon| {
            TrayIconBuilder::with_id(TRAY_ID)
                .icon(icon)
                .tooltip(labels.name.clone())
                // Left click opens the window; the right click keeps the menu, like Steam and other desktop tools.
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|icon, event| {
                    if let TrayIconEvent::Click { button, .. } = event {
                        let app = icon.app_handle();

                        match button {
                            MouseButton::Left => actions::show_window(app),
                            MouseButton::Right => {
                                let _ = refresh(app);
                            }
                            MouseButton::Middle => {}
                        }
                    }
                })
                .on_menu_event(|app, event| {
                    actions::handle(app, event.id().as_ref());
                    let _ = refresh(app);
                })
                .build(app)
                .map_err(AppError::unexpected)
        })?;
    state.set_tray(tooltip);
    refresh(app)?;
    watch(app, "task-started");
    watch(app, "task-finished");
    watch(app, "task-cancelled");
    let exit = app.clone();
    app.listen(crate::launch::commands::GAME_EXITED_EVENT, move |_event| {
        let _ = refresh(&exit);
    });
    Ok(())
}

/// Subscribes to one task lifecycle event: the icon and the Working submenu follow state changes, and a task that needs
/// the user announces itself when the window is hidden.
fn watch(app: &AppHandle, name: &str) {
    let handle = app.clone();
    app.listen(name, move |event| {
        let Ok(task) = serde_json::from_str::<TaskView>(event.payload()) else {
            return;
        };
        notifications::task_event(&handle, &task);
        let _ = refresh(&handle);
    });
}

/// Hides the main window after a game starts when the user asked for it. Both launch paths call this only after the game
/// started, so a failed launch leaves the window where it is; the tray owns the behaviour and announces the move.
pub fn after_game_launch(app: &AppHandle) {
    // Hiding without an icon would leave no way back. The settings page hides the switch on such a desktop too.
    if !close::available(app) {
        return;
    }
    let Some(data) = app.try_state::<Data>() else {
        return;
    };
    if load_app_settings(&data).hide_when_game_launches
        && let Some(window) = app.get_webview_window("main")
    {
        let _ = window.hide();
        hidden_to_tray(app);
    }
}

/// Rebuilds the icon, tooltip and menu from one fresh snapshot.
pub fn refresh(app: &AppHandle) -> Result<(), AppError> {
    let Some(state) = app.try_state::<TrayState>() else {
        return Ok(());
    };
    let Some(icon) = state.tray() else {
        return Ok(());
    };
    let snapshot = view::snapshot(app)?;
    let rendered = view::render(&snapshot, &state.labels());
    let image = icons::load(rendered.icon).map_err(AppError::unexpected)?;
    icon.set_icon(Some(image)).map_err(AppError::unexpected)?;
    icon.set_tooltip(Some(rendered.tooltip.clone()))
        .map_err(AppError::unexpected)?;
    let menu = menu::build(app, &rendered).map_err(AppError::unexpected)?;
    icon.set_menu(Some(menu)).map_err(AppError::unexpected)?;
    Ok(())
}
