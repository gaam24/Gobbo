//! The native notifications for work that continues while the window is hidden. The tray already owns the translated
//! words and the task lifecycle, so the decision to speak and the message stay here instead of becoming a second source
//! of truth. A notification never fails the work that announced it.
//!
//! Every task kind has its own switch per outcome (`import.failed`, `import.done`, ...), plus one switch per app event
//! in [`APP_NOTIFICATIONS`]. The main `system_notifications` switch silences all of them.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::context::Data;
use crate::infra::tasks::{TaskView, state};
use crate::settings::config::{
    APP_NOTIFICATIONS, AppSettings, load_app_settings, task_notification,
};

use super::labels::TrayLabels;
use super::state::TrayState;

/// The code of the notification sent when the window is hidden in the tray.
const HIDDEN_TO_TRAY: &str = APP_NOTIFICATIONS[0];

/// What one task event announces.
#[derive(Debug, PartialEq, Eq)]
enum Notice {
    /// The work paused on a user action. It has no switch of its own: the work cannot go on until the user answers, so
    /// only the main switch silences it.
    Waiting,
    /// The work ended; the `<task kind>.<outcome>` code the user can mute.
    Outcome(String),
}

/// The notice of one task event, or `None` for an event that never notifies: running, a progress tick, or a
/// cancellation the user asked for.
fn notice_of(task: &TaskView) -> Option<Notice> {
    match task.state.as_str() {
        state::WAITING => Some(Notice::Waiting),
        state::FAILED | state::DONE => {
            Some(Notice::Outcome(task_notification(&task.kind, &task.state)))
        }
        _ => None,
    }
}

/// Whether the user lets this notice through. Pure, so the rule is tested without a window or a settings file.
fn allowed(notice: &Notice, settings: &AppSettings) -> bool {
    match notice {
        Notice::Waiting => settings.system_notifications,
        Notice::Outcome(code) => settings.notifies(code),
    }
}

/// Whether the main window cannot show the result itself. A missing window counts as hidden: there is no place to read
/// the result on screen.
fn window_hidden(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_none_or(|window| {
        !window.is_visible().unwrap_or(true) || window.is_minimized().unwrap_or(false)
    })
}

fn labels(app: &AppHandle) -> TrayLabels {
    app.try_state::<TrayState>()
        .map_or_else(TrayLabels::default, |tray| tray.labels())
}

fn show(app: &AppHandle, title: String, body: String) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Registers the click handler once at startup. Without a handler the notification server never reports a click, so a
/// user who taps the message would be left with a hidden window. A tap is not tied to a particular notification on
/// desktop, and every message here concerns background work, so they all open Working.
pub fn init_action_handler(app: &AppHandle) {
    let handle = app.clone();
    if let Err(error) = app.notification().on_action(move |action| {
        if action.action_id() == "tap" {
            super::actions::show_window(&handle);
            let _ = handle.emit("tray-open-view", "working");
        }
    }) {
        crate::infra::log::warn(format!("could not listen for notification clicks: {error}"));
    }
}

/// The notification hook for one task lifecycle event. The title names the game, so each body is one short sentence
/// that every language can translate without a placeholder.
pub fn task_event(app: &AppHandle, task: &TaskView) {
    let Some(notice) = notice_of(task) else {
        return;
    };
    let Some(data) = app.try_state::<Data>() else {
        return;
    };
    if !allowed(&notice, &load_app_settings(&data)) || !window_hidden(app) {
        return;
    }

    let labels = labels(app);
    let body = match &notice {
        Notice::Waiting => labels.notice_waiting.clone(),
        Notice::Outcome(code) => labels
            .notices
            .get(code)
            .cloned()
            .unwrap_or_else(|| code.clone()),
    };
    let title = task
        .game
        .as_deref()
        .and_then(|id| data.definition_shared(id).ok())
        .map_or(labels.notice_title, |game| game.name.clone());
    show(app, title, body);
}

/// Announces that the window was hidden in the tray, so a user who closed Gobbo knows it is still working and can click
/// the message to come back. Called from both places that hide the window.
pub fn hidden_to_tray(app: &AppHandle) {
    let Some(data) = app.try_state::<Data>() else {
        return;
    };
    if !load_app_settings(&data).notifies(HIDDEN_TO_TRAY) || !window_hidden(app) {
        return;
    }

    let labels = labels(app);
    show(app, labels.notice_title, labels.notice_hidden);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::tasks::TaskDedupe;

    fn task(kind: &str, state: &str) -> TaskView {
        TaskView {
            id: "task-1".to_owned(),
            game: Some("game-1".to_owned()),
            kind: kind.to_owned(),
            state: state.to_owned(),
            percent: None,
            dedupe: TaskDedupe::Other,
            error: None,
        }
    }

    #[test]
    fn only_a_waiting_or_finished_task_has_a_notice() {
        assert_eq!(notice_of(&task("import", "running")), None);
        assert_eq!(notice_of(&task("import", "cancelled")), None);
        assert_eq!(notice_of(&task("import", "waiting")), Some(Notice::Waiting));
        assert_eq!(
            notice_of(&task("import", "failed")),
            Some(Notice::Outcome("import.failed".to_owned()))
        );
        assert_eq!(
            notice_of(&task("update", "done")),
            Some(Notice::Outcome("update.done".to_owned()))
        );
    }

    #[test]
    fn a_muted_outcome_is_silent_and_the_others_still_speak() {
        let settings = AppSettings {
            muted_notifications: vec!["import.done".to_owned()],
            ..AppSettings::default()
        };
        let done = notice_of(&task("import", "done")).expect("a finished task has a notice");
        let failed = notice_of(&task("import", "failed")).expect("a failed task has a notice");
        let other = notice_of(&task("update", "done")).expect("a finished task has a notice");
        assert!(!allowed(&done, &settings));
        assert!(allowed(&failed, &settings));
        assert!(allowed(&other, &settings));
    }

    #[test]
    fn the_main_switch_silences_waiting_work_too() {
        let off = AppSettings {
            system_notifications: false,
            ..AppSettings::default()
        };
        assert!(!allowed(&Notice::Waiting, &off));
        assert!(allowed(&Notice::Waiting, &AppSettings::default()));
    }
}
