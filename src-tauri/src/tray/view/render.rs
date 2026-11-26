//! Turns a snapshot and the translated labels into one menu description: no Tauri state, no disk.

use crate::infra::tasks::TaskView;
use crate::infra::tasks::state::{FAILED, RUNNING, WAITING};
use crate::status::stage::NextAction;
use crate::tray::labels::{TrayLabels, fill};

use super::model::{IconState, Rendered, Snapshot, SpecItem, Stage};
use super::snapshot::label_of;

/// The stable code of a next action, the same `snake_case` code the locale files use.
pub const fn action_code(action: NextAction) -> &'static str {
    match action {
        NextAction::SetGameFolder => "set_game_folder",
        NextAction::SetSettingsFolder => "set_settings_folder",
        NextAction::SetModsFolder => "set_mods_folder",
        NextAction::InstallLoader => "install_loader",
        NextAction::FixProblems => "fix_problems",
        NextAction::ImportMods => "import_mods",
        NextAction::EnableMods => "enable_mods",
        NextAction::Apply => "apply",
        NextAction::Play => "play",
    }
}

/// The one status line shared by the tooltip and the menu header.
fn status_line(snapshot: &Snapshot, labels: &TrayLabels) -> String {
    let Some(game) = &snapshot.game else {
        return labels.no_game.clone();
    };

    if game.running {
        return labels.game_running.clone();
    }

    let attention = attention_count(snapshot);

    if attention > 0 {
        return fill(&labels.attention, "count", &attention.to_string());
    }

    if let Some(busy) = &snapshot.busy {
        return label_of(&labels.busy, busy);
    }

    if game.draft > 0 {
        return fill(&labels.changes, "count", &game.draft.to_string());
    }

    if game.stage == Stage::Applied {
        return labels.applied.clone();
    }

    label_of(&labels.next, action_code(game.next))
}

fn needs_attention(task: &TaskView) -> bool {
    task.state == WAITING || task.state == FAILED
}

fn attention_count(snapshot: &Snapshot) -> usize {
    snapshot
        .tasks
        .iter()
        .filter(|task| needs_attention(task))
        .count()
}

fn task_label(task: &TaskView, labels: &TrayLabels) -> String {
    let state = label_of(&labels.states, &task.state);
    let kind = label_of(&labels.kinds, &task.kind);
    let base = format!("{state} — {kind}");

    match task.percent {
        Some(percent) if task.state == RUNNING => format!(
            "{base}, {}",
            fill(&labels.percent, "percent", &percent.to_string())
        ),
        _ => base,
    }
}

/// Builds the menu description from a snapshot and the translated labels. Pure: no Tauri state, no disk.
pub fn render(snapshot: &Snapshot, labels: &TrayLabels) -> Rendered {
    let line = status_line(snapshot, labels);
    let attention = attention_count(snapshot);
    let busy = snapshot.busy.is_some();
    let any_running = snapshot.tasks.iter().any(|task| task.state == RUNNING);
    let icon = if attention > 0 {
        IconState::Attention
    } else if busy || any_running {
        IconState::Working
    } else if snapshot.game.as_ref().is_some_and(|game| game.draft > 0) {
        IconState::Changes
    } else {
        IconState::Idle
    };
    let (title, subtitle, tooltip) = snapshot.game.as_ref().map_or_else(
        || {
            (
                labels.name.clone(),
                labels.no_game.clone(),
                labels.no_game.clone(),
            )
        },
        |game| {
            (
                format!("{} · {}", labels.name, game.name),
                format!("{} — {}", game.profile, line),
                fill(
                    &fill(
                        &fill(&labels.tooltip, "game", &game.name),
                        "profile",
                        &game.profile,
                    ),
                    "status",
                    &line,
                ),
            )
        },
    );
    let mut items = vec![
        SpecItem::Action {
            id: "tray.title".to_owned(),
            label: title.clone(),
            enabled: false,
        },
        SpecItem::Action {
            id: "tray.subtitle".to_owned(),
            label: subtitle.clone(),
            enabled: false,
        },
        SpecItem::Separator,
    ];

    if let Some(game) = &snapshot.game {
        if game.running {
            items.push(SpecItem::Action {
                id: "tray.stop".to_owned(),
                label: labels.stop.clone(),
                enabled: snapshot.busy.is_none(),
            });
        } else {
            items.push(SpecItem::Action {
                id: "tray.primary".to_owned(),
                label: label_of(&labels.next, action_code(game.next)),
                enabled: !busy,
            });
        }

        items.push(SpecItem::Action {
            id: "tray.undo".to_owned(),
            label: labels.undo.clone(),
            enabled: game.deployed && !game.running,
        });
        items.push(SpecItem::Separator);
        items.push(SpecItem::Submenu {
            id: "tray.profiles".to_owned(),
            label: labels.profiles.clone(),
            enabled: true,
            items: game
                .profiles
                .iter()
                .map(|(id, name)| SpecItem::Check {
                    id: format!("tray.profile:{id}"),
                    label: name.clone(),
                    checked: game.profile == *name,
                })
                .collect(),
        });

        if snapshot.games.len() > 1 {
            items.push(SpecItem::Submenu {
                id: "tray.games".to_owned(),
                label: labels.games.clone(),
                enabled: true,
                items: snapshot
                    .games
                    .iter()
                    .map(|choice| SpecItem::Check {
                        id: format!("tray.game:{}", choice.id),
                        label: choice.name.clone(),
                        checked: choice.id == game.id,
                    })
                    .collect(),
            });
        }

        items.push(SpecItem::Separator);
    }

    let working_label = if attention > 0 {
        format!("{} ({attention})", labels.working)
    } else {
        labels.working.clone()
    };
    let mut working_items: Vec<SpecItem> = snapshot
        .tasks
        .iter()
        .take(8)
        .map(|task| SpecItem::Action {
            id: format!("tray.task:{}", task.id),
            label: task_label(task, labels),
            enabled: true,
        })
        .collect();
    if working_items.is_empty() {
        working_items.push(SpecItem::Action {
            id: "tray.no-tasks".to_owned(),
            label: labels.no_tasks.clone(),
            enabled: false,
        });
    } else {
        working_items.push(SpecItem::Separator);
        working_items.push(SpecItem::Action {
            id: "tray.open-working".to_owned(),
            label: labels.open_working.clone(),
            enabled: true,
        });
    }

    items.push(SpecItem::Submenu {
        id: "tray.working".to_owned(),
        label: working_label,
        enabled: true,
        items: working_items,
    });
    items.push(SpecItem::Separator);
    items.push(SpecItem::Action {
        id: "tray.open".to_owned(),
        label: labels.open.clone(),
        enabled: true,
    });
    items.push(SpecItem::Action {
        id: "tray.settings".to_owned(),
        label: labels.settings.clone(),
        enabled: true,
    });
    items.push(SpecItem::Action {
        id: "tray.quit".to_owned(),
        label: labels.quit.clone(),
        enabled: true,
    });
    Rendered {
        icon,
        tooltip,
        title,
        subtitle,
        items,
    }
}
