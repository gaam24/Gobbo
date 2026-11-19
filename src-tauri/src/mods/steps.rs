//! The steps each mod asks for, edited in the inspector. The choices are kept in `mod_steps.toml`. A mod can pick only
//! the steps the game definition has for the phase, so the file never names a program.

use crate::context::Data;
use crate::context::load_library_shared;
use crate::error::AppError;
use crate::games;
use crate::games::GameDef;

/// Saves the steps a mod asks for. Each id must be a step the game defines for that phase. A mod with no steps
/// leaves the file. The other mods keep their entries.
pub fn set_mod_steps(
    root: &Data,
    game_id: &str,
    folder: &str,
    before_start: &[String],
    after_end: &[String],
) -> Result<(), AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    load_library_shared(&data, &game)?.version(folder)?;
    let before = checked(&game, before_start, games::Phase::BeforeStart)?;
    let after = checked(&game, after_end, games::Phase::AfterEnd)?;

    let mut steps = crate::launch::steps::load_mod_steps(&data)?;
    if before.is_empty() && after.is_empty() {
        steps.mods.remove(folder);
    } else {
        steps.mods.insert(
            folder.to_string(),
            crate::domain::mod_steps::Phases {
                before_start: before,
                after_end: after,
            },
        );
    }
    crate::launch::steps::save_mod_steps(&data, &steps)
}

/// The ids that the game defines for `phase`, without repeats, in the order given.
fn checked(game: &GameDef, ids: &[String], phase: games::Phase) -> Result<Vec<String>, AppError> {
    let mut kept: Vec<String> = Vec::new();
    for id in ids {
        if game.step(id, phase).is_none() {
            return Err(AppError::user("steps.unknown").with("step", id));
        }
        if !kept.contains(id) {
            kept.push(id.clone());
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;

    fn game() -> GameDef {
        let mut game = test_games::mirror();
        game.steps.push(games::Step {
            id: "compile".to_string(),
            phase: games::Phase::BeforeStart,
            program: "tools/compiler.exe".to_string(),
            args: Vec::new(),
            when_dir: None,
            clean: Vec::new(),
            outputs: Vec::new(),
            log: None,
        });
        game
    }

    #[test]
    fn only_steps_the_game_defines_for_the_phase_are_accepted() {
        let game = game();
        let before = checked(&game, &["compile".to_string()], games::Phase::BeforeStart);
        assert_eq!(before.unwrap(), ["compile"]);

        let wrong_phase = checked(&game, &["compile".to_string()], games::Phase::AfterEnd);
        assert!(wrong_phase.is_err());
        let unknown = checked(&game, &["nope".to_string()], games::Phase::BeforeStart);
        assert!(unknown.is_err());
    }

    #[test]
    fn a_repeated_step_is_kept_once() {
        let game = game();
        let ids = ["compile".to_string(), "compile".to_string()];
        assert_eq!(
            checked(&game, &ids, games::Phase::BeforeStart).unwrap(),
            ["compile"]
        );
    }
}
