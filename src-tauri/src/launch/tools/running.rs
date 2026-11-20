//! Running the game and its tools in the overlay layers, and finding a tool by its id.

use crate::context::Data;
use crate::context::load_settings;
use crate::context::load_stopped;
use crate::deploy::DeployError;
use crate::deploy::ops::ensure_active_deployed;
use crate::deploy::ops::record_launch;
use crate::deploy::overlay::Session;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::games;
use crate::games::Executable;
use crate::games::GameDef;
use crate::games::Tool;
use crate::launch::executable::check_start;
use crate::launch::start::game_env;
use crate::launch::start::native_dlls_for_launch;
use crate::launch::start::overlay_session_state;
use crate::launch::start::program_launch;
use crate::launch::start::runs_through_proton;
use crate::launch::tools::prerequisites::missing_prerequisites;
use crate::platform::Process;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::infra::tasks::TaskHandle;

/// Runs a program in the overlay layer (Steam launch option: `gobbo run <game> -- %command%`).
/// Without an overlay deployment it is an error, because a game started that way would see only the game folder.
/// The steps of the enabled mods run before the program starts and after it exits.
pub fn run_game(
    root: &Data,
    game_id: &str,
    program: &str,
    args: &[String],
) -> Result<i32, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    check_start(&settings)?;
    ensure_active_deployed(root, game_id, false)?;
    record_launch(root, game_id)?;
    let Some(state) = overlay_session_state(&data)? else {
        return Err(DeployError::NoOverlaySession.into());
    };
    let game_root = Path::new(settings.game_root.trim());
    let session = crate::launch::overlay_session(&data, &game, &settings, &state, game_root)?;
    // The variables the app's own launch gives the game. Steam's launch option calls this command, so the variables
    // Steam passed in are inherited; the game's program is its default executable.
    let program_file = game.executables.first().map_or("", |exe| exe.path.as_str());
    let native = native_dlls_for_launch(&game, game_root, program_file, Some(&session));
    let inherited = std::env::var("WINEDLLOVERRIDES").ok();
    let env = game_env(
        &settings,
        runs_through_proton(&game),
        &native,
        inherited.as_deref(),
    )?;
    let launch =
        super::super::start::Launch::new(program.to_string(), args.to_vec(), env, Some(session));
    let home = crate::games::paths::home_dir()?;
    let outcome = super::super::pipeline::run(
        super::super::pipeline::RunOptions {
            data: &data,
            state_root: root.dir.clone(),
            game: &game,
            settings: &settings,
            home: &home,
            game_id: Some(game_id),
            steps: true,
            live: false,
            background: false,
            watch: false,
            report: true,
            begin_report: true,
            with_output: true,
            finish: true,
            on_exit: None,
        },
        launch,
        true,
        "launch.native_failed",
    )?;
    Ok(outcome.exit_code.unwrap_or(1))
}

/// Starts a tool of the game (`[[tools]]`) in the overlay layers, as the game would run it: it sees the mods, and what
/// it writes lands in the overwrite layer, where it can become a mod of its own. The program is started and the wait is
/// detached, so this returns as soon as it runs; `on_exit` receives the exit code later. The cancel flag is checked at
/// the safe boundaries before the program starts, and a watcher stops the process while it runs. The progress handle
/// is part of the task pattern; a tool reports no percent of its own.
pub fn run_tool(
    root: &Data,
    game_id: &str,
    tool_id: &str,
    home: &Path,
    cancel: Arc<AtomicBool>,
    _handle: TaskHandle,
    on_exit: Option<super::super::pipeline::ExitNotice>,
) -> Result<(), AppError> {
    crate::infra::tasks::stop_if(Some(cancel.as_ref()), "tools.cancelled")?;
    let (game, data, settings) = load_stopped(root, game_id)?;
    check_start(&settings)?;
    let tool = tool_of(&game, tool_id)?;
    let missing = missing_prerequisites(&game, tool, &settings.game_root);

    if !missing.is_empty() {
        return Err(AppError::user("tools.prerequisites_missing").with("names", missing.join(", ")));
    }

    let Some(state) = overlay_session_state(&data)? else {
        return Err(AppError::user("tools.needs_overlay"));
    };
    let layers = Session::at(&state, Path::new(settings.game_root.trim())).layers();
    let program = tool
        .detect
        .iter()
        .find(|path| layers.resolve(path, Path::is_file).is_some())
        .ok_or_else(|| tool.missing_error())?;
    let exe = Executable {
        name: tool.name.clone(),
        path: program.clone(),
        path_linux: None,
        args: Vec::new(),
    };
    // The program is about to be spawned, the last point where stopping leaves no half-written tool behind.
    crate::infra::tasks::stop_if(Some(cancel.as_ref()), "tools.cancelled")?;
    let (launch, with_mods, spawn_error) = program_launch(&game, &data, &settings, &exe, home)?;
    // The program's path inside the overlay, so the cancel watcher can find its process. The layers are resolved with
    // the same rule as the launch itself: a mod may provide the tool, otherwise it lies in the game folder.
    let launched = layers.resolve(program, Path::is_file).map_or_else(
        || Path::new(settings.game_root.trim()).join(program),
        |(_, path)| path,
    );
    let _outcome = super::super::pipeline::run(
        super::super::pipeline::RunOptions {
            data: &data,
            state_root: root.dir.clone(),
            game: &game,
            settings: &settings,
            home,
            game_id: None,
            steps: false,
            live: false,
            background: true,
            watch: false,
            report: false,
            begin_report: false,
            with_output: false,
            finish: false,
            on_exit,
        },
        launch,
        with_mods,
        spawn_error,
    )?;
    // The wait is detached now. A watcher makes Stop take effect while the tool runs; without it the cancel flag would
    // only be seen at the next safe boundary, after the program had already finished. The pipeline still reports the
    // exit, and the notice turns a cancelled flag into a null code and the `cancelled` task state.
    std::thread::spawn(move || watch_tool_cancel(cancel, launched));
    Ok(())
}

/// Polls a tool's cancel flag while its process runs and stops the process as soon as the user asks. The watcher gives
/// up when the process is gone, or shortly after a Stop when the program could not be found. It never reports an exit:
/// the pipeline's wait thread owns that, so a tool that ended just before Stop is reported once, with its real code.
fn watch_tool_cancel(cancel: Arc<AtomicBool>, program: std::path::PathBuf) {
    // A program may need a moment to appear in the process list after it was spawned.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut process = None;

    while process.is_none() && Instant::now() < deadline {
        process = Process::by_program(&program);

        if process.is_none() {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    let Some(process) = process else {
        return;
    };

    loop {
        if !process.exists() {
            return;
        }

        if crate::infra::tasks::cancelled(Some(cancel.as_ref())) {
            process.stop();
            return;
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The tool of the game with this id.
pub(super) fn tool_of<'a>(game: &'a GameDef, tool_id: &str) -> Result<&'a Tool, AppError> {
    game.tools
        .iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| {
            AppError::new("tools.unknown")
                .with("tool", tool_id)
                .kind(ErrorKind::Permanent)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_is_found_by_its_id_and_an_unknown_one_is_an_error() {
        let game = crate::test_support::games::slot();
        let tool = &game.tools[0];

        assert_eq!(tool_of(&game, &tool.id).unwrap().id, tool.id);
        assert!(tool_of(&game, "no-such-tool").is_err());
    }
}
