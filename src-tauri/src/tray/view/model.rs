//! The small types the snapshot and the menu share: the icon, the reduced game stage, one menu item, and the two read
//! models handed to the renderer.

use crate::infra::tasks::TaskView;
use crate::status::stage::NextAction;

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
