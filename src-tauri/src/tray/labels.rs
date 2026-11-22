//! The words of the native menu. The webview owns the translations; the frontend sends them here whenever the language
//! changes. These English strings are only the bootstrap before the window has sent the real labels.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::infra::tasks::TASK_KINDS;
use crate::settings::config::{NOTIFICATION_OUTCOMES, task_notification};

/// Every string the tray menu and its notices show. The maps are keyed by the stable codes the backend already emits:
/// `NextAction`, task kinds and task states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TrayLabels {
    pub name: String,
    pub open: String,
    pub settings: String,
    pub quit: String,
    pub undo: String,
    pub stop: String,
    pub profiles: String,
    pub games: String,
    pub working: String,
    pub open_working: String,
    pub no_game: String,
    pub no_tasks: String,
    /// `{count}` changes waiting.
    pub changes: String,
    /// `{count}` tasks need attention.
    pub attention: String,
    pub applied: String,
    pub game_running: String,
    /// `{percent}` percent of a task.
    pub percent: String,
    /// `{game}` and `{profile}` in the tooltip.
    pub tooltip: String,
    pub next: BTreeMap<String, String>,
    pub busy: BTreeMap<String, String>,
    pub kinds: BTreeMap<String, String>,
    pub states: BTreeMap<String, String>,
    pub quit_title: String,
    pub quit_game: String,
    pub quit_tasks: String,
    pub quit_stop: String,
    pub quit_cancel: String,
    pub quit_leave: String,
    /// The title of a notification that is not about one game; a task notification takes the game's name instead.
    pub notice_title: String,
    /// The body for a task that waits for the user.
    pub notice_waiting: String,
    /// The body of each task notification, keyed by its `<task kind>.<outcome>` code. A full sentence per code, so every
    /// language can agree the verb with its own noun.
    pub notices: BTreeMap<String, String>,
    /// Shown when the main window was hidden in the tray.
    pub notice_hidden: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            name: "Gobbo".to_owned(),
            open: "Open Gobbo".to_owned(),
            settings: "Settings".to_owned(),
            quit: "Quit Gobbo".to_owned(),
            undo: "Undo apply".to_owned(),
            stop: "Stop game".to_owned(),
            profiles: "Profiles".to_owned(),
            games: "Games".to_owned(),
            working: "Working".to_owned(),
            open_working: "Open Working".to_owned(),
            no_game: "Add a game to start".to_owned(),
            no_tasks: "No tasks".to_owned(),
            changes: "Changes waiting: {{count}}".to_owned(),
            attention: "Needs attention: {{count}}".to_owned(),
            applied: "Applied".to_owned(),
            game_running: "Game running".to_owned(),
            percent: "{{percent}}%".to_owned(),
            tooltip: "{{game}} · {{profile}} — {{status}}".to_owned(),
            next: BTreeMap::from([
                ("set_game_folder", "Set the game folder"),
                ("set_settings_folder", "Set the settings folder"),
                ("set_mods_folder", "Set the mods folder"),
                ("install_loader", "Install the mod loader"),
                ("fix_problems", "Fix problems"),
                ("import_mods", "Import mods"),
                ("enable_mods", "Enable mods"),
                ("apply", "Apply and play"),
                ("play", "Play"),
            ])
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect(),
            busy: BTreeMap::from([
                ("applying", "Applying…"),
                ("launching", "Launching…"),
                ("stopping", "Stopping…"),
                ("updating", "Updating…"),
            ])
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect(),
            kinds: TASK_KINDS
                .iter()
                .map(|&code| (code.to_owned(), kind_label(code).to_owned()))
                .collect(),
            states: BTreeMap::from([
                ("running", "Running"),
                ("waiting", "Waiting for you"),
                ("failed", "Needs attention"),
            ])
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect(),
            quit_title: "Quit Gobbo?".to_owned(),
            quit_game: "A game is running. Stopping Gobbo leaves it running without the overlay manager.".to_owned(),
            quit_tasks: "Work is still in progress. It will be interrupted and can be retried after a restart."
                .to_owned(),
            quit_stop: "Stop game and quit".to_owned(),
            quit_cancel: "Cancel".to_owned(),
            quit_leave: "Quit anyway".to_owned(),
            notice_title: "Gobbo".to_owned(),
            notice_waiting: "A task is waiting for your input.".to_owned(),
            notices: TASK_KINDS
                .iter()
                .flat_map(|&code| {
                    NOTIFICATION_OUTCOMES.iter().map(move |&outcome| {
                        (
                            task_notification(code, outcome),
                            bootstrap_notice(code, outcome),
                        )
                    })
                })
                .collect(),
            notice_hidden: "Gobbo keeps working in the tray. Click to open it.".to_owned(),
        }
    }
}

/// Fills a `{{name}}` placeholder in a label. Missing values stay visible, so a bad key is easy to spot.
pub fn fill(template: &str, name: &str, value: &str) -> String {
    template.replace(&format!("{{{{{name}}}}}"), value)
}

/// The bootstrap English text of one stable task kind, used before the window sends its translations. Every code in
/// [`TASK_KINDS`] has one; a test fails when a new code is added without its label.
fn kind_label(code: &str) -> &'static str {
    match code {
        "apply" => "Applying mods…",
        "import" => "Importing files…",
        "update" | "update-all" => "Updating mods…",
        "requirement" => "Installing a requirement…",
        "pack" => "Importing a mod pack…",
        "pack-link" => "Downloading a mod pack…",
        "collection" => "Installing collection…",
        "launch" => "Running a tool…",
        "install" => "Installing tool prerequisites…",
        "loader" => "Installing the mod loader…",
        _ => "Working…",
    }
}

/// The bootstrap English body of one task notification. The window replaces it with its translation.
fn bootstrap_notice(code: &str, outcome: &str) -> String {
    let what = match code {
        "apply" => "Applying mods",
        "import" => "Import",
        "update" => "Mod update",
        "update-all" => "Updating mods",
        "requirement" => "Requirement install",
        "pack" => "Mod pack import",
        "pack-link" => "Mod pack download",
        "collection" => "Collection install",
        "launch" => "Tool run",
        "install" => "Tool prerequisites",
        "loader" => "Mod loader install",
        _ => "Task",
    };
    let verb = if outcome == crate::infra::tasks::state::FAILED {
        "failed"
    } else {
        "finished"
    };
    format!("{what} {verb}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fallback is for a code that is not a stable task kind at all; every kind the backend emits has real text.
    #[test]
    fn every_task_kind_has_a_bootstrap_label() {
        let labels = TrayLabels::default();

        for code in TASK_KINDS {
            assert_eq!(
                labels.kinds.get(*code).map(String::as_str),
                Some(kind_label(code))
            );
            assert_ne!(
                kind_label(code),
                "Working…",
                "{code} needs a bootstrap label"
            );
        }

        assert_eq!(labels.kinds.len(), TASK_KINDS.len());
    }

    #[test]
    fn every_task_kind_has_a_bootstrap_notice_for_each_outcome() {
        let labels = TrayLabels::default();

        for code in TASK_KINDS {
            for outcome in NOTIFICATION_OUTCOMES {
                let notice = labels.notices.get(&task_notification(code, outcome));
                assert!(
                    notice.is_some_and(|text| !text.starts_with("Task ")),
                    "{code}.{outcome} needs a bootstrap notice"
                );
            }
        }

        assert_eq!(
            labels.notices.len(),
            TASK_KINDS.len() * NOTIFICATION_OUTCOMES.len()
        );
    }

    #[test]
    fn a_placeholder_is_replaced_and_an_unknown_one_stays_visible() {
        assert_eq!(fill("{{count}} tasks", "count", "3"), "3 tasks");
        assert_eq!(fill("{{count}} tasks", "other", "3"), "{{count}} tasks");
    }
}
