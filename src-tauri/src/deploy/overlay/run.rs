//! Running a program in the overlay: the steps of a game, their launchers and their output.

use std::fs;
use std::io::PipeWriter;
use std::path::Path;

use super::session::Session;
use super::{Running, start};
use crate::context::data::JOURNAL_FILE;
use crate::deploy::error::DeployError;
use crate::deploy::journal::{Journal, JournalOp};
use crate::error::FileError;
use crate::games::Step;
use gobbo_overlay_core::join;

/// Runs `program` in the layers with `env` added, and returns its exit code, waiting for it to exit.
pub fn execute(
    session: &Session,
    program: &str,
    args: &[String],
    env: &[(String, String)],
) -> Result<i32, DeployError> {
    start(session, program, args, env, None)?.wait()
}

/// Runs `program` in an overlay session and returns the game's exit code. Without an overlay deployment it is an error.
/// Tests use it; the app itself needs the output variant.
#[cfg(test)]
pub fn run(session: &Session, program: &str, args: &[String]) -> Result<i32, DeployError> {
    run_with_output(session, program, args, None)
}

/// The same as `run`, with the program's standard output and error going to `output` when it is given.
pub fn run_with_output(
    session: &Session,
    program: &str,
    args: &[String],
    output: Option<PipeWriter>,
) -> Result<i32, DeployError> {
    run_with_env(session, program, args, &[], output)
}

/// Runs a program in the overlay and waits for it, like `run_with_output`, with these variables set for it.
pub fn run_with_env(
    session: &Session,
    program: &str,
    args: &[String],
    env: &[(String, String)],
    output: Option<PipeWriter>,
) -> Result<i32, DeployError> {
    start_with_env(session, program, args, env, output)?.wait()
}

/// The same checks as `run_with_env`, but hands back the running game once it is resumed, so a caller can record how
/// long the start took before it waits for the exit.
pub fn start_with_env(
    session: &Session,
    program: &str,
    args: &[String],
    env: &[(String, String)],
    output: Option<PipeWriter>,
) -> Result<Running, DeployError> {
    if !crate::platform::supports_overlay() {
        return Err(DeployError::OverlayUnsupported);
    }

    if !session.mods.is_dir() {
        return Err(DeployError::NoOverlaySession);
    }

    start(session, program, args, env, output)
}

/// Removes the `clean` files of a step from the overwrite layer. It runs for a step that does not run as well: otherwise
/// the output of an earlier run outlives the mods that made it, and the game loads it. A redscript cache of mods that
/// are disabled now is such an output.
pub fn clear_outputs(session: &Session, step: &Step) -> Result<(), DeployError> {
    let layers = session.layers();

    for rel in &step.clean {
        let path = join(&layers.upper, rel);

        if path.is_file() {
            fs::remove_file(&path).map_err(|e| FileError::new(&path, e))?;
        }
    }

    Ok(())
}

/// A program that starts the program of a step, such as Proton for a Steam game on Linux. The step's program becomes its
/// argument, after `args`, and the launcher gets the variables it needs.
#[derive(Debug, Clone)]
pub struct Launcher {
    /// The launcher's program, for example the `proton` file.
    pub program: String,
    /// The arguments before the step's program, for example `waitforexitandrun`.
    pub args: Vec<String>,
    /// The variables the launcher needs.
    pub env: Vec<(String, String)>,
}

/// Runs one game step in the overlay, as `game.toml` describes it. Its `clean` files are removed from the overwrite layer
/// first, also when the step is skipped (see `clear_outputs`). The step is skipped when its `when_dir` is not in any
/// layer. Its `outputs` are journaled. The program is looked up in the layers, so a mod's copy of it is used. With a
/// `launcher` the program starts through it, read from the game folder where the overlay shows the mods, as the game
/// itself is started. A nonzero exit code fails the step.
pub fn run_step(
    state: &Path,
    session: &Session,
    step: &Step,
    launcher: Option<&Launcher>,
) -> Result<bool, DeployError> {
    let layers = session.layers();
    clear_outputs(session, step)?;

    if let Some(dir) = &step.when_dir
        && layers.resolve(dir, Path::is_dir).is_none()
    {
        return Ok(false);
    }

    let journal = Journal::new(state.join(JOURNAL_FILE));

    for rel in &step.outputs {
        journal.append(&JournalOp::FileDeployed {
            path: join(&layers.upper, rel),
        })?;
    }

    let Some((_, program)) = layers.resolve(&step.program, Path::is_file) else {
        return Err(DeployError::StepFailed {
            detail: format!("{} is missing for step {}", step.program, step.id),
        });
    };
    let root = layers.root.display().to_string();
    let args: Vec<String> = step
        .args
        .iter()
        .map(|arg| arg.replace("{game}", &root))
        .collect();
    let (program, args, env) = match launcher {
        Some(launcher) => {
            let mut through = launcher.args.clone();
            through.push(layers.root.join(&step.program).display().to_string());
            through.extend(args);
            (launcher.program.clone(), through, launcher.env.clone())
        }
        None => (program.display().to_string(), args, Vec::new()),
    };
    let code = execute(session, &program, &args, &env)?;

    if code != 0 {
        return Err(DeployError::StepFailed {
            detail: format!("step {} stopped with code {code}", step.id),
        });
    }

    Ok(true)
}
