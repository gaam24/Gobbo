use std::path::Path;
use std::path::PathBuf;

use super::env::game_env;
use super::has_overlay_session;
use crate::context::Data;
use crate::context::Settings;
use crate::context::load_settings;
use crate::context::running::game_running_at;
use crate::error::AppError;
use crate::error::game_running as game_is_running_error;
use crate::games::Executable;
use crate::games::GameDef;
#[cfg(not(windows))]
use crate::games::runtime::Runtime;
use crate::infra::clock::now_seconds;
use crate::launch::executable::launch_args;
use crate::launch::executable::pick_executable;
use crate::launch::executable::{check_start, executable_missing};
use crate::launch::model::Launch;
use crate::launch::pipeline;
#[cfg(not(windows))]
use crate::launch::proton::proton_command;
use crate::launch::report;

/// Stops every process of the game, also one started outside Gobbo, and returns whether a process was stopped. The
/// process list is searched by the same rule as `game_running`, so a game that is stuck before its window appears can
/// be stopped from Gobbo.
pub fn stop_game(root: &Data, game_id: &str) -> Result<bool, AppError> {
    root.definition_shared(game_id)?;
    let settings = load_settings(&root.game(game_id));
    let game_root = settings.game_root.trim();

    if game_root.is_empty() {
        return Ok(false);
    }

    Ok(crate::platform::stop_game(Path::new(game_root)))
}

/// Starts the game's launch file (`executable`, or the default when none). A game with Steam app ids goes through
/// Proton from the Steam library on Linux; a game without them starts its own file directly (GOG, a game from disk).
/// When the deployment is prepared, the process starts in the mod layer. Returns `true` when the mods are in the game.
/// It does not wait for the game to exit. Only a background thread waits, so the process does not become a zombie.
/// Starts the game and, when it ends, calls `on_exit` so the window can stop its running check at once.
pub fn launch_game(
    root: &Data,
    game_id: &str,
    executable: Option<&str>,
    apply_draft: bool,
    home: &Path,
    on_exit: Option<pipeline::ExitNotice>,
) -> Result<bool, AppError> {
    let game = root.definition_shared(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);

    if game_running_at(game_id, &settings.game_root) {
        return Err(game_is_running_error());
    }
    // The game starts with the viewed profile: another profile's deployment makes way for it first.
    crate::deploy::ops::ensure_active_deployed(root, game_id, apply_draft)?;
    crate::deploy::ops::record_launch(root, game_id)?;
    report::begin(&data);
    let outcome = start_recorded(root, &game, game_id, executable, home, on_exit, &settings);

    if outcome.is_err() {
        report::update(&data, |r| r.finished_at = Some(now_seconds()));
    }

    outcome
}

/// The start of a launch, recorded in the last-launch report. A background thread waits for the game, then records
/// its exit code and runs the steps that come after it.
fn start_recorded(
    root: &Data,
    game: &GameDef,
    game_id: &str,
    executable: Option<&str>,
    home: &Path,
    on_exit: Option<pipeline::ExitNotice>,
    settings: &Settings,
) -> Result<bool, AppError> {
    let data = root.game(game_id);
    let (launch, with_mods, spawn_error) =
        launch_command_with_settings(game, root, game_id, executable, home, settings)?;
    let outcome = pipeline::run(
        pipeline::RunOptions::for_game(
            &data,
            root.dir.clone(),
            game,
            settings,
            home,
            game_id,
            on_exit,
        ),
        launch,
        with_mods,
        spawn_error,
    )?;
    Ok(outcome.with_mods)
}

/// The launch of the game, without starting it. In an overlay session the program runs in the layers, as `gobbo run`
/// does. Returns also the error code for a failed start, which names Proton or the native file.
#[cfg_attr(
    windows,
    allow(
        unused_variables,
        reason = "only Proton reads the Steam folder, and Windows has none"
    )
)]
#[cfg(test)]
pub fn launch_command(
    game: &GameDef,
    root: &Data,
    game_id: &str,
    executable: Option<&str>,
    home: &Path,
) -> Result<(Launch, bool, &'static str), AppError> {
    let data = root.game(game_id);
    let settings = load_settings(&data);
    launch_command_with_settings(game, root, game_id, executable, home, &settings)
}

/// `launch_command` with the settings the caller already read. A launch reads `config.json` once and passes the same
/// snapshot through the command, the steps and the pipeline; only the end-of-game work reads it again, because a long
/// session can change which data belongs to the profile.
pub(super) fn launch_command_with_settings(
    game: &GameDef,
    root: &Data,
    game_id: &str,
    executable: Option<&str>,
    home: &Path,
    settings: &Settings,
) -> Result<(Launch, bool, &'static str), AppError> {
    let data = root.game(game_id);
    check_start(settings)?;
    let exe = pick_executable(game, executable)?;
    program_launch(game, &data, settings, exe, home)
}

/// Whether a path is at or past the legacy `MAX_PATH` of Windows, 260 UTF-16 units. Windows starts a program at such a
/// path only in its verbatim form, and it refuses such a path as a working folder. Every other system takes any length.
#[cfg(windows)]
pub(super) fn legacy_long(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().count() >= 260
}

/// See the Windows version: only Windows has the legacy limit.
#[cfg(not(windows))]
pub(super) const fn legacy_long(_path: &Path) -> bool {
    false
}

/// The text that starts a program at `path`. On Windows a path past the legacy limit gets the verbatim prefix, which the
/// system reads without the limit: `\\?\C:\...`, or `\\?\UNC\server\share\...` for a network path. Any other path keeps
/// its text.
fn program_text(path: &Path) -> String {
    let text = path.display().to_string();

    if !legacy_long(path) || !path.is_absolute() || text.starts_with(r"\\?\") {
        return text;
    }

    text.strip_prefix(r"\\").map_or_else(
        || format!(r"\\?\{text}"),
        |network| format!(r"\\?\UNC\{network}"),
    )
}

/// How a program of the game starts: the game itself or one of its tools. Through Proton for a Steam game on Linux, in
/// the overlay layers when an overlay is deployed. A program a mod brings is found in the layers.
#[cfg_attr(
    windows,
    allow(
        unused_variables,
        reason = "only Proton reads the Steam folder, and Windows has none"
    )
)]
pub(in crate::launch) fn program_launch(
    game: &GameDef,
    data: &Data,
    settings: &Settings,
    exe: &Executable,
    home: &Path,
) -> Result<(Launch, bool, &'static str), AppError> {
    let game_root = PathBuf::from(settings.game_root.trim());
    // The mods are in the game when something is deployed: the layer holds them and the game folder stays untouched.
    // The state is read once and reused for the overlay check; each read parses `profiles.json`.
    let deployed_state = data.deployed_state();
    let with_mods = deployed_state.is_some();

    // The install decides the runtime: a Windows program of a Steam install goes through Steam's Proton, and one from
    // another store through Proton in its own prefix. A native build, and anything on Windows, starts directly.
    #[cfg(not(windows))]
    let runtime = crate::context::target::runtime(game, settings);
    #[cfg(not(windows))]
    {
        match runtime {
            Runtime::SteamProton | Runtime::OwnedProton => {
                return proton_command(
                    game, data, settings, &game_root, exe, home, with_mods, runtime,
                );
            }
            Runtime::Native => {}
        }
    }

    // A program of this system runs its own file: no Proton, no prefix, only the user's environment and arguments.
    // In an overlay the file may come from a mod, so the layers say where it really is.
    let session = match deployed_state {
        Some(state) if has_overlay_session(&state)? => Some(crate::launch::overlay_session(
            data, game, settings, &state, &game_root,
        )?),
        _ => None,
    };
    let exe_path = exe.path_for_platform(crate::platform::is_windows());
    // A mod may provide the program itself. That only works when the deployment root is the game folder; a game with
    // its mods outside the game folder runs the program from the game folder.
    let path = session
        .as_ref()
        .filter(|session| session.root == session.game_root)
        .and_then(|session| session.layers().resolve(exe_path, Path::is_file))
        .map_or_else(|| game_root.join(exe_path), |(_, path)| path);
    if !path.is_file() {
        return Err(executable_missing(path.display()));
    }

    // A program started without the overlay gets the verbatim form of a long path; the overlay keeps its own text.
    let program = if session.is_some() {
        path.display().to_string()
    } else {
        program_text(&path)
    };
    // Games often load their own files by relative path, so they start in the folder of their executable.
    let folder = path.parent().unwrap_or(game_root.as_path()).to_path_buf();
    let args = launch_args(exe, settings);
    // A program that runs without Proton gets only the settings' own variables.
    let env = game_env(settings, false, &[], None)?;
    let mut launch = Launch::new(program, args, env, session);

    // Windows refuses a working folder of the legacy limit or longer, so such a game starts in the folder Gobbo runs in.
    if let Launch::Plain(command) = &mut launch
        && !legacy_long(&folder)
    {
        command.current_dir(&folder);
    }

    Ok((launch, with_mods, "launch.native_failed"))
}
