use super::*;
use crate::infra::tasks::{TaskDedupe, TaskView};
use crate::status::stage::NextAction;
use crate::tray::labels::TrayLabels;

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
        error: None,
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
