//! The steps of a mod as the inspector shows them: the steps the game defines, and the ones the mod asks for in each
//! phase. The choices are read from `mod_steps.toml`; saving them is in `mods::steps`.

use crate::domain::mod_steps::ModSteps;
use crate::games;
use crate::games::GameDef;
use serde::Serialize;
use std::path::Path;

/// A step the game defines, as the editor lists it.
#[derive(Debug, Serialize, specta::Type)]
pub struct GameStepView {
    pub id: String,
    pub phase: games::Phase,
    /// The file name of the program the step runs, so the user sees what it is.
    pub program: String,
}

/// The steps the game defines, and the ids this mod asks for in each phase.
#[derive(Debug, Serialize, specta::Type)]
pub struct StepChoices {
    pub defined: Vec<GameStepView>,
    pub before_start: Vec<String>,
    pub after_end: Vec<String>,
}

/// The steps of the game and the ones the mod asks for, from the `mod_steps.toml` the caller already read.
pub fn step_choices(game: &GameDef, folder: &str, steps: &ModSteps) -> StepChoices {
    let asked = steps.mods.get(folder);
    StepChoices {
        defined: game
            .steps
            .iter()
            .map(|step| GameStepView {
                id: step.id.clone(),
                phase: step.phase,
                program: Path::new(&step.program).file_name().map_or_else(
                    || step.program.clone(),
                    |name| name.to_string_lossy().into_owned(),
                ),
            })
            .collect(),
        before_start: asked
            .map(|phases| phases.before_start.clone())
            .unwrap_or_default(),
        after_end: asked
            .map(|phases| phases.after_end.clone())
            .unwrap_or_default(),
    }
}
