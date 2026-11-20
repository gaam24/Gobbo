//! The tray's view model. `snapshot` is the only part that reads the app; `render` turns plain data into one menu
//! description, so the state, labels and enabled rules can be tested without a window or an operating system.

use tauri::{AppHandle, Manager};

use crate::context::Data;
use crate::context::managed::managed_games;
use crate::error::AppError;
use crate::infra::tasks::{TaskFlags, TaskView};
use crate::status::stage::{GameStage, NextAction};

use super::labels::{TrayLabels, fill};
use super::state::TrayState;

/// Which of the small icons the tray shows. The shape changes with the state; colour is never the only signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Idle,
    Changes,
    Working,
    Attention,
}

/// The stage of the active game, reduced to what fits in a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Setup,
    Blocked,
    Empty,
    NoEnabled,
    Ready,
    Draft,
    Applied,
}

/// One line of the menu spec. Ids are the stable strings the menu handler dispatches on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecItem {
    Action {
        id: String,
        label: String,
        enabled: bool,
    },
    Check {
        id: String,
        label: String,
        checked: bool,
    },
    Separator,
    Submenu {
        id: String,
        label: String,
        enabled: bool,
        items: Vec<SpecItem>,
    },
}

/// Everything the menu, tooltip and icon need for one render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub icon: IconState,
    pub tooltip: String,
    pub title: String,
    pub subtitle: String,
    pub items: Vec<SpecItem>,
}

/// The active game as the tray sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub profiles: Vec<(String, String)>,
    pub next: NextAction,
    pub stage: Stage,
    pub running: bool,
    pub deployed: bool,
    pub draft: u32,
}

/// One game in the Games submenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameChoice {
    pub id: String,
    pub name: String,
}

/// A snapshot of everything the tray shows, gathered in one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub game: Option<Game>,
    pub games: Vec<GameChoice>,
    pub tasks: Vec<TaskView>,
    pub busy: Option<String>,
}

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
    let running = crate::launch::start::game_running(data, id)?;
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

/// The stable code of a next action, the same `snake_case` code the locale files use.
pub const fn action_code(action: NextAction) -> &'static str {
    match action {
        NextAction::SetGameFolder => "set_game_folder",
        NextAction::SetSettingsFolder => "set_settings_folder",
        NextAction::SetModsFolder => "set_mods_folder",
        NextAction::FixProblems => "fix_problems",
        NextAction::ImportMods => "import_mods",
        NextAction::EnableMods => "enable_mods",
        NextAction::Apply => "apply",
        NextAction::Play => "play",
    }
}

fn label_of(map: &std::collections::BTreeMap<String, String>, code: &str) -> String {
    map.get(code).cloned().unwrap_or_else(|| code.to_owned())
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
    task.state == "waiting" || task.state == "failed"
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
        Some(percent) if task.state == "running" => format!(
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
    let any_running = snapshot.tasks.iter().any(|task| task.state == "running");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::tasks::TaskDedupe;

    fn game() -> Game {
        Game {
            id: "generic-game".to_owned(),
            name: "Generic Game".to_owned(),
            profile: "Main".to_owned(),
            profiles: vec![
                ("p1".to_owned(), "Main".to_owned()),
                ("p2".to_owned(), "Vanilla".to_owned()),
            ],
            next: NextAction::Play,
            stage: Stage::Applied,
            running: false,
            deployed: true,
            draft: 0,
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            game: Some(game()),
            games: vec![
                GameChoice {
                    id: "generic-game".to_owned(),
                    name: "Generic Game".to_owned(),
                },
                GameChoice {
                    id: "other-game".to_owned(),
                    name: "Other Game".to_owned(),
                },
            ],
            tasks: Vec::new(),
            busy: None,
        }
    }

    fn in_items<'a>(items: &'a [SpecItem], id: &str) -> Option<&'a SpecItem> {
        for item in items {
            match item {
                SpecItem::Action { id: found, .. } | SpecItem::Check { id: found, .. }
                    if found == id =>
                {
                    return Some(item);
                }

                SpecItem::Submenu {
                    id: found, items, ..
                } => {
                    if found == id {
                        return Some(item);
                    }

                    if let Some(found) = in_items(items, id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn action<'a>(rendered: &'a Rendered, id: &str) -> Option<&'a SpecItem> {
        in_items(&rendered.items, id)
    }

    fn label(item: &SpecItem) -> &str {
        match item {
            SpecItem::Action { label, .. }
            | SpecItem::Check { label, .. }
            | SpecItem::Submenu { label, .. } => label,
            SpecItem::Separator => "",
        }
    }

    fn enabled(item: &SpecItem) -> bool {
        match item {
            SpecItem::Action { enabled, .. } | SpecItem::Submenu { enabled, .. } => *enabled,
            SpecItem::Check { .. } => true,
            SpecItem::Separator => false,
        }
    }

    #[test]
    fn applied_game_offers_play_and_undo() {
        let rendered = render(&snapshot(), &TrayLabels::default());
        assert_eq!(rendered.icon, IconState::Idle);
        assert_eq!(label(action(&rendered, "tray.primary").unwrap()), "Play");
        assert!(enabled(action(&rendered, "tray.undo").unwrap()));
        assert!(rendered.tooltip.contains("Applied"));
        assert!(rendered.subtitle.contains("Applied"));
    }

    #[test]
    fn draft_changes_take_over_the_status_and_icon() {
        let mut current = snapshot();
        current.game.as_mut().unwrap().draft = 3;
        current.game.as_mut().unwrap().stage = Stage::Draft;
        current.game.as_mut().unwrap().next = NextAction::Apply;
        let rendered = render(&current, &TrayLabels::default());
        assert_eq!(rendered.icon, IconState::Changes);
        assert!(rendered.subtitle.contains('3'));
        assert_eq!(
            label(action(&rendered, "tray.primary").unwrap()),
            "Apply and play"
        );
    }

    #[test]
    fn a_running_game_offers_stop_and_blocks_undo() {
        let mut current = snapshot();
        current.game.as_mut().unwrap().running = true;
        let rendered = render(&current, &TrayLabels::default());
        assert_eq!(rendered.icon, IconState::Idle);
        assert!(action(&rendered, "tray.primary").is_none());
        assert_eq!(label(action(&rendered, "tray.stop").unwrap()), "Stop game");
        assert!(!enabled(action(&rendered, "tray.undo").unwrap()));
        assert!(rendered.tooltip.contains("Game running"));
    }

    #[test]
    fn busy_disables_only_the_work_in_progress() {
        let mut current = snapshot();
        current.busy = Some("applying".to_owned());
        let rendered = render(&current, &TrayLabels::default());
        assert_eq!(rendered.icon, IconState::Working);
        assert!(!enabled(action(&rendered, "tray.primary").unwrap()));
        assert!(enabled(action(&rendered, "tray.undo").unwrap()));
        assert!(rendered.subtitle.contains("Applying"));
    }

    #[test]
    fn a_waiting_task_marks_attention_and_counts_it() {
        let mut current = snapshot();
        current.tasks.push(TaskView {
            id: "collection:generic-game:main".to_owned(),
            game: Some("generic-game".to_owned()),
            kind: "collection".to_owned(),
            state: "waiting".to_owned(),
            percent: None,
            dedupe: TaskDedupe::Other,
        });
        let rendered = render(&current, &TrayLabels::default());
        assert_eq!(rendered.icon, IconState::Attention);
        let working = rendered
            .items
            .iter()
            .find(|item| matches!(item, SpecItem::Submenu { id, .. } if id == "tray.working"))
            .unwrap();
        assert_eq!(label(working), "Working (1)");
        let SpecItem::Submenu { items, .. } = working else {
            panic!("the working entry is a submenu");
        };
        assert!(
            items
                .iter()
                .any(|item| label(item).contains("Waiting for you"))
        );
    }

    #[test]
    fn profiles_and_games_mark_the_current_choice() {
        let rendered = render(&snapshot(), &TrayLabels::default());
        assert!(matches!(
            action(&rendered, "tray.profile:p1"),
            Some(SpecItem::Check { checked: true, .. })
        ));
        assert!(matches!(
            action(&rendered, "tray.profile:p2"),
            Some(SpecItem::Check { checked: false, .. })
        ));
        assert!(matches!(
            action(&rendered, "tray.game:generic-game"),
            Some(SpecItem::Check { checked: true, .. })
        ));
        assert!(matches!(
            action(&rendered, "tray.game:other-game"),
            Some(SpecItem::Check { checked: false, .. })
        ));
    }

    #[test]
    fn a_single_game_has_no_games_submenu() {
        let mut current = snapshot();
        current.games.pop();
        let rendered = render(&current, &TrayLabels::default());
        assert!(action(&rendered, "tray.games").is_none());
    }

    #[test]
    fn without_a_game_the_menu_only_offers_a_way_in() {
        let mut current = snapshot();
        current.game = None;
        let rendered = render(&current, &TrayLabels::default());
        assert_eq!(rendered.icon, IconState::Idle);
        assert_eq!(rendered.subtitle, "Add a game to start");
        assert!(action(&rendered, "tray.primary").is_none());
        assert!(action(&rendered, "tray.open").is_some());
    }
}
