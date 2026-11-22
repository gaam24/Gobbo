//! The one derived state of a game and the next action it asks for.
//!
//! The window never derives this state from booleans: the status carries it, and every screen reads it. The order of
//! the checks is the order a person would follow: first the folders the game needs, then problems that block an apply,
//! then the mod list, then the deployment.

use serde::Serialize;

use crate::error::Message;

/// The state of a game, from the folders it needs to the list that is applied to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameStage {
    /// A folder the game needs is missing. `step` names the one to set.
    Setup { step: SetupStep },
    /// The game and the list are ready, but the deployment plan has problems to fix first.
    Blocked { reasons: Vec<Message> },
    /// The library has no mods yet.
    Empty,
    /// The library has mods, but none is enabled.
    NoEnabled { total: u32 },
    /// A list with enabled mods that was never applied.
    Ready { enabled: u32, total: u32 },
    /// The active profile is applied, and the list changed since then.
    Draft {
        changes: u32,
        enabled: u32,
        total: u32,
    },
    /// The active profile is applied and the list matches the deployment.
    Applied {
        at: Option<u32>,
        enabled: u32,
        total: u32,
    },
}

/// The folder that keeps a game from being ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SetupStep {
    Game,
    Settings,
    Mods,
}

/// The status message of an enabled mod that needs the game's loader package while it is not installed. The mod list
/// shows it with an install button, and it makes [`NextAction::InstallLoader`] the next action.
pub const LOADER_MISSING: &str = "status.loader_missing";

/// The one thing the user should do next. The frontend maps it to a label and an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    SetGameFolder,
    SetSettingsFolder,
    SetModsFolder,
    /// An enabled mod needs the game's loader package; one click installs it.
    InstallLoader,
    FixProblems,
    ImportMods,
    EnableMods,
    Apply,
    Play,
}

/// Why an apply cannot run yet. The window only translates this value; it does not re-derive the reason from folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum BlockerKey {
    /// Both the game folder and the settings folder are missing.
    GameAndSettings,
    /// The game folder is missing.
    GameFolder,
    /// The external mods folder is missing.
    ModsFolder,
    /// The deployment plan has problems.
    Problems,
    /// The library is empty or nothing is enabled.
    NoMods,
}

/// The first reason an apply is impossible, in the same order as the stage rules. A game that is ready and playable has
/// no blocker, so the window does not have to guess from the stage or the next action.
pub const fn blocker_of(input: &StageInput<'_>) -> Option<BlockerKey> {
    if !input.game_ready {
        if input.needs_settings_dir {
            return Some(BlockerKey::GameAndSettings);
        }

        return Some(BlockerKey::GameFolder);
    }

    if input.needs_settings_dir && !input.settings_ready {
        return Some(BlockerKey::GameAndSettings);
    }

    if input.needs_mods_root && !input.mods_ready {
        return Some(BlockerKey::ModsFolder);
    }

    if !input.problems.is_empty() {
        return Some(BlockerKey::Problems);
    }

    if input.total == 0 || input.enabled == 0 {
        return Some(BlockerKey::NoMods);
    }

    None
}

/// The values a stage is derived from. `StageInput` is plain data, so the rules are tested without a disk or a window.
#[derive(Clone, Copy)]
pub struct StageInput<'a> {
    pub game_ready: bool,
    pub needs_settings_dir: bool,
    pub settings_ready: bool,
    pub needs_mods_root: bool,
    pub mods_ready: bool,
    pub problems: &'a [Message],
    pub total: u32,
    pub enabled: u32,
    pub deployed: bool,
    pub draft_changes: u32,
    pub deployed_at: Option<u32>,
}

/// The stage and the next action, by one ordered rule. A setup step wins over a problem, because the user cannot fix a
/// plan for a folder that is not there. A problem wins over the list, because an apply would fail.
pub fn game_stage(input: StageInput<'_>) -> (GameStage, NextAction) {
    if !input.game_ready {
        return (
            GameStage::Setup {
                step: SetupStep::Game,
            },
            NextAction::SetGameFolder,
        );
    }

    if input.needs_settings_dir && !input.settings_ready {
        return (
            GameStage::Setup {
                step: SetupStep::Settings,
            },
            NextAction::SetSettingsFolder,
        );
    }

    if input.needs_mods_root && !input.mods_ready {
        return (
            GameStage::Setup {
                step: SetupStep::Mods,
            },
            NextAction::SetModsFolder,
        );
    }

    if !input.problems.is_empty() {
        return (
            GameStage::Blocked {
                reasons: input.problems.to_vec(),
            },
            if input
                .problems
                .iter()
                .any(|problem| problem.code == LOADER_MISSING)
            {
                NextAction::InstallLoader
            } else {
                NextAction::FixProblems
            },
        );
    }

    if input.total == 0 {
        return (GameStage::Empty, NextAction::ImportMods);
    }

    if input.enabled == 0 {
        return (
            GameStage::NoEnabled { total: input.total },
            NextAction::EnableMods,
        );
    }

    if input.deployed && input.draft_changes > 0 {
        return (
            GameStage::Draft {
                changes: input.draft_changes,
                enabled: input.enabled,
                total: input.total,
            },
            NextAction::Apply,
        );
    }

    if input.deployed {
        return (
            GameStage::Applied {
                at: input.deployed_at,
                enabled: input.enabled,
                total: input.total,
            },
            NextAction::Play,
        );
    }

    (
        GameStage::Ready {
            enabled: input.enabled,
            total: input.total,
        },
        NextAction::Apply,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> StageInput<'static> {
        StageInput {
            game_ready: true,
            needs_settings_dir: false,
            settings_ready: true,
            needs_mods_root: false,
            mods_ready: true,
            problems: &[],
            total: 3,
            enabled: 2,
            deployed: false,
            draft_changes: 0,
            deployed_at: None,
        }
    }

    fn stage(input: StageInput<'_>) -> GameStage {
        game_stage(input).0
    }

    fn action(input: StageInput<'_>) -> NextAction {
        game_stage(input).1
    }

    #[test]
    fn a_missing_folder_is_the_first_thing_to_fix() {
        let missing = StageInput {
            game_ready: false,
            needs_settings_dir: true,
            settings_ready: false,
            problems: &[Message::new("plan.duplicate_mod")],
            ..input()
        };
        assert_eq!(
            stage(missing),
            GameStage::Setup {
                step: SetupStep::Game
            }
        );
        assert_eq!(action(missing), NextAction::SetGameFolder);

        let settings = StageInput {
            needs_settings_dir: true,
            settings_ready: false,
            ..input()
        };
        assert_eq!(
            stage(settings),
            GameStage::Setup {
                step: SetupStep::Settings
            }
        );
        assert_eq!(action(settings), NextAction::SetSettingsFolder);

        let mods = StageInput {
            needs_mods_root: true,
            mods_ready: false,
            ..input()
        };
        assert_eq!(
            stage(mods),
            GameStage::Setup {
                step: SetupStep::Mods
            }
        );
        assert_eq!(action(mods), NextAction::SetModsFolder);
    }

    #[test]
    fn a_plan_problem_blocks_the_list_until_it_is_fixed() {
        let problems = [Message::new("status.requirement_missing")];
        let blocked = StageInput {
            problems: &problems,
            ..input()
        };
        assert_eq!(
            stage(blocked),
            GameStage::Blocked {
                reasons: problems.to_vec()
            }
        );
        assert_eq!(action(blocked), NextAction::FixProblems);
    }

    #[test]
    fn the_list_moves_from_empty_to_enabled_to_ready() {
        let empty = StageInput {
            total: 0,
            enabled: 0,
            ..input()
        };
        assert_eq!(stage(empty), GameStage::Empty);
        assert_eq!(action(empty), NextAction::ImportMods);

        let all_off = StageInput {
            enabled: 0,
            ..input()
        };
        assert_eq!(stage(all_off), GameStage::NoEnabled { total: 3 });
        assert_eq!(action(all_off), NextAction::EnableMods);

        assert_eq!(
            stage(input()),
            GameStage::Ready {
                enabled: 2,
                total: 3
            }
        );
        assert_eq!(action(input()), NextAction::Apply);
    }

    #[test]
    fn a_deployment_with_new_changes_is_a_draft() {
        let draft = StageInput {
            deployed: true,
            draft_changes: 2,
            deployed_at: Some(10),
            ..input()
        };
        assert_eq!(
            stage(draft),
            GameStage::Draft {
                changes: 2,
                enabled: 2,
                total: 3
            }
        );
        assert_eq!(action(draft), NextAction::Apply);
    }

    #[test]
    fn a_matching_deployment_is_applied_and_asks_for_play() {
        let applied = StageInput {
            deployed: true,
            deployed_at: Some(10),
            ..input()
        };
        assert_eq!(
            stage(applied),
            GameStage::Applied {
                at: Some(10),
                enabled: 2,
                total: 3
            }
        );
        assert_eq!(action(applied), NextAction::Play);
    }

    #[test]
    fn the_blocker_follows_the_same_order_as_the_stage() {
        assert_eq!(blocker_of(&input()), None);

        let both_folders = StageInput {
            game_ready: false,
            needs_settings_dir: true,
            ..input()
        };
        assert_eq!(blocker_of(&both_folders), Some(BlockerKey::GameAndSettings));

        let game = StageInput {
            game_ready: false,
            ..input()
        };
        assert_eq!(blocker_of(&game), Some(BlockerKey::GameFolder));

        let settings = StageInput {
            needs_settings_dir: true,
            settings_ready: false,
            ..input()
        };
        assert_eq!(blocker_of(&settings), Some(BlockerKey::GameAndSettings));

        let mods = StageInput {
            needs_mods_root: true,
            mods_ready: false,
            ..input()
        };
        assert_eq!(blocker_of(&mods), Some(BlockerKey::ModsFolder));

        let problems = StageInput {
            problems: &[Message::new("plan.duplicate_mod")],
            ..input()
        };
        assert_eq!(blocker_of(&problems), Some(BlockerKey::Problems));

        let empty = StageInput {
            total: 0,
            ..input()
        };
        assert_eq!(blocker_of(&empty), Some(BlockerKey::NoMods));
        let none_enabled = StageInput {
            enabled: 0,
            ..input()
        };
        assert_eq!(blocker_of(&none_enabled), Some(BlockerKey::NoMods));
    }
}
