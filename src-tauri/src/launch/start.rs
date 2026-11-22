use crate::context::Data;
use crate::context::Settings;
use crate::context::data::JOURNAL_FILE;
use crate::context::load_settings;
use crate::context::running::game_running_at;
use crate::deploy::journal::Journal;
use crate::deploy::overlay;
use crate::deploy::overlay::Session;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::error::game_running as game_is_running_error;
use crate::games::Executable;
use crate::games::GameDef;
#[cfg(not(windows))]
use crate::games::runtime::Runtime;
use crate::infra::clock::now_seconds;
use crate::launch::executable::launch_args;
use crate::launch::executable::pick_executable;
use crate::launch::executable::{check_start, executable_missing};
#[cfg(not(windows))]
use crate::launch::proton::proton_command;
use crate::launch::report;
use std::io::PipeWriter;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

/// How a game starts: as a plain command, or as the program in the overlay layers. On Windows the overlay starts the
/// game in this process with the hook loaded; on Linux it starts it under `unshare`.
#[derive(Debug)]
pub enum Launch {
    Plain(Command),
    Layered {
        session: Session,
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

/// A started game. `wait` waits for it to exit.
pub enum Running {
    Plain(Child),
    /// Boxed: the overlay handle carries the hook and its launch timings, many times the size of a plain child.
    Layered(Box<overlay::Running>),
}

impl Launch {
    /// A plain command, or a command in the overlay layers when `session` is set.
    pub(super) fn new(
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        session: Option<Session>,
    ) -> Self {
        if let Some(session) = session {
            Self::Layered {
                session,
                program,
                args,
                env,
            }
        } else {
            let mut command = Command::new(&program);
            command.args(&args).envs(env);
            Self::Plain(command)
        }
    }

    /// Starts the game. A failure names `spawn_error`, which says whether Proton or the native file failed.
    pub(super) fn start(
        self,
        spawn_error: &'static str,
        output: Option<PipeWriter>,
    ) -> Result<Running, AppError> {
        let failed = |detail: String| {
            AppError::new(spawn_error)
                .with("detail", detail)
                .kind(ErrorKind::Permanent)
        };

        match self {
            Self::Plain(mut command) => {
                let program = command.get_program().to_string_lossy().into_owned();
                let args: Vec<String> = command
                    .get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect();
                crate::infra::log::info(format!("launch: program={program} args={args:?}"));

                if let Some(writer) = output {
                    let errors = writer.try_clone().map_err(|e| failed(e.to_string()))?;
                    command
                        .stdout(Stdio::from(writer))
                        .stderr(Stdio::from(errors));
                }

                command
                    .spawn()
                    .map(Running::Plain)
                    .map_err(|e| failed(e.to_string()))
            }

            Self::Layered {
                session,
                program,
                args,
                env,
            } => {
                crate::infra::log::info(format!("launch: program={program} args={args:?} overlay"));
                overlay::start(&session, &program, &args, &env, output)
                    .map(|running| Running::Layered(Box::new(running)))
                    .map_err(|e| failed(e.to_string()))
            }
        }
    }
}

impl Running {
    /// The launcher's steps when the game went through the overlay; `None` for a plain start.
    pub(super) const fn timings(&self) -> Option<overlay::LaunchTimings> {
        match self {
            Self::Plain(_) => None,
            Self::Layered(running) => running.timings(),
        }
    }

    /// Waits for the game to exit, and returns its exit code when it has one.
    pub(super) fn wait(&mut self) -> Option<i32> {
        match self {
            Self::Plain(child) => child.wait().ok().and_then(|status| status.code()),
            Self::Layered(running) => running.wait().ok(),
        }
    }
}

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
    on_exit: Option<super::pipeline::ExitNotice>,
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
    on_exit: Option<super::pipeline::ExitNotice>,
    settings: &Settings,
) -> Result<bool, AppError> {
    let data = root.game(game_id);
    let (launch, with_mods, spawn_error) =
        launch_command_with_settings(game, root, game_id, executable, home, settings)?;
    let outcome = super::pipeline::run(
        super::pipeline::RunOptions::for_game(
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
fn launch_command_with_settings(
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

/// How a program of the game starts: the game itself or one of its tools. Through Proton for a Steam game on Linux, in
/// the overlay layers when an overlay is deployed. A program a mod brings is found in the layers.
#[cfg_attr(
    windows,
    allow(
        unused_variables,
        reason = "only Proton reads the Steam folder, and Windows has none"
    )
)]
pub(super) fn program_launch(
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
        Some(state) if has_overlay_session(&state)? => Some(super::overlay_session(
            data, game, settings, &state, &game_root,
        )?),
        _ => None,
    };
    let exe_path = exe.path_for_platform();
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

    let program = path.display().to_string();
    // Games often load their own files by relative path, so they start in the folder of their executable.
    let folder = path.parent().unwrap_or(game_root.as_path()).to_path_buf();
    let args = launch_args(exe, settings);
    // A program that runs without Proton gets only the settings' own variables.
    let env = game_env(settings, false, &[], None)?;
    let mut launch = Launch::new(program, args, env, session);

    if let Launch::Plain(command) = &mut launch {
        command.current_dir(&folder);
    }

    Ok((launch, with_mods, "launch.native_failed"))
}

/// The variables a game starts with. Under Proton come the chosen Proton options, then the settings' own variables (a
/// variable of both takes the settings' value), then the DLL overrides that make the proxies in `native` load native.
/// `inherited` is the `WINEDLLOVERRIDES` the process would pass on; the override written here keeps its names.
pub fn game_env(
    settings: &Settings,
    through_proton: bool,
    native: &[String],
    inherited: Option<&str>,
) -> Result<Vec<(String, String)>, AppError> {
    let mut env = if through_proton {
        let mut options = crate::domain::proton_options::variables(&settings.proton_options);
        options.extend(crate::domain::proton_options::fps_limit_variables(
            settings.proton_fps_limit,
        ));
        options
    } else {
        Vec::new()
    };

    for (name, value) in crate::domain::arguments::parse_env(&settings.launch_env)? {
        env.retain(|(known, _)| *known != name);
        env.push((name, value));
    }

    if through_proton && !native.is_empty() {
        let existing = env
            .iter()
            .rev()
            .find(|(name, _)| name == "WINEDLLOVERRIDES")
            .map(|(_, value)| value.as_str())
            .or(inherited);
        let dlls: Vec<&str> = native.iter().map(String::as_str).collect();

        if let Some(value) = crate::domain::arguments::native_first_overrides(existing, &dlls) {
            env.retain(|(name, _)| name != "WINEDLLOVERRIDES");
            env.push(("WINEDLLOVERRIDES".to_string(), value));
        }
    }

    Ok(env)
}

/// The DLLs of a launch that load native: the loader DLLs and the proxies of the definition that the launched view of
/// the game has next to the program. `program` is the game's program relative to the game folder. The view is the overlay
/// layer seen from the game folder when the deployment root is the game folder or a folder inside it, so a proxy that a
/// mod places next to the program is found there; a game whose mods live outside the game folder runs its own program
/// folder, which the overlay does not change.
pub fn native_dlls_for_launch(
    game: &GameDef,
    game_root: &Path,
    program: &str,
    session: Option<&Session>,
) -> Vec<String> {
    let dir = crate::games::proxy::program_dir(program);
    session.and_then(Session::game_layers).map_or_else(
        || crate::games::proxy::native_dlls(game, dir, |rel| game_root.join(rel).is_file()),
        |layers| {
            crate::games::proxy::native_dlls(game, dir, |rel| {
                layers.resolve(rel, Path::is_file).is_some()
            })
        },
    )
}

pub fn has_overlay_session(state: &Path) -> Result<bool, AppError> {
    Journal::new(state.join(JOURNAL_FILE))
        .has_layer_prepared()
        .map_err(AppError::from)
}

/// The state folder when an overlay session is deployed (the mod layer). Otherwise `None`.
pub fn overlay_session_state(data: &Data) -> Result<Option<PathBuf>, AppError> {
    let Some(state) = data.deployed_state() else {
        return Ok(None);
    };
    Ok(has_overlay_session(&state)?.then_some(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn a_proxy_from_a_mod_under_a_deployment_root_inside_the_game_loads_native() {
        let base = scratch!("launch-native-inner-root");
        let game_root = base.join("game");
        let program = "inner/bin/game.exe";
        fs::create_dir_all(game_root.join("inner/bin")).unwrap();
        fs::write(game_root.join(program), "").unwrap();
        let session = Session::with_roots(
            &base.join("state"),
            &game_root.join("inner"),
            &game_root,
            Vec::new(),
        );
        let proxy = session.mods.join("inner/bin/version.dll");
        fs::create_dir_all(proxy.parent().unwrap()).unwrap();
        fs::write(&proxy, "proxy").unwrap();
        let game = test_games::mirror();

        assert_eq!(
            native_dlls_for_launch(&game, &game_root, program, Some(&session)),
            ["version.dll"],
            "the loader mod's proxy is in the layer, not in the game folder"
        );
        assert!(native_dlls_for_launch(&game, &game_root, program, None).is_empty());
    }

    #[test]
    fn a_launch_that_carries_the_settings_does_not_read_config_again() {
        crate::test_support::games::install();
        let base = scratch!("launch-settings-snapshot");
        let root = Data::new(base.join("data"));
        let game_data = root.game(test_games::MIRROR_ID);
        let mut game = test_games::mirror();
        game.detect.steam_app_ids.clear();
        let program = game.executables[0].path.clone();
        game.executables[0].path_linux = Some(program);

        // Folder A holds the program, and the saved config points at it.
        let folder_a = base.join("a");
        fs::create_dir_all(folder_a.join("bin/x64")).unwrap();
        fs::write(folder_a.join("bin/x64/game.exe"), "").unwrap();
        crate::context::save_settings(
            &root,
            test_games::MIRROR_ID,
            Settings {
                game_root: folder_a.display().to_string(),
                ..Settings::default()
            },
        )
        .unwrap();
        let snapshot = load_settings(&game_data);

        let (launch, _, _) = launch_command_with_settings(
            &game,
            &root,
            test_games::MIRROR_ID,
            None,
            &base,
            &snapshot,
        )
        .unwrap();

        // A new config points at folder B; the command built from the snapshot still carries folder A.
        let folder_b = base.join("b");
        fs::create_dir_all(folder_b.join("bin/x64")).unwrap();
        fs::write(folder_b.join("bin/x64/game.exe"), "").unwrap();
        crate::context::save_settings(
            &root,
            test_games::MIRROR_ID,
            Settings {
                game_root: folder_b.display().to_string(),
                ..Settings::default()
            },
        )
        .unwrap();

        let Launch::Plain(command) = launch else {
            panic!("with nothing deployed the native game starts as a plain command");
        };
        assert_eq!(
            command.get_program(),
            folder_a.join("bin/x64/game.exe").as_os_str()
        );

        let (launch, _, _) =
            launch_command(&game, &root, test_games::MIRROR_ID, None, &base).unwrap();
        let Launch::Plain(command) = launch else {
            panic!("with nothing deployed the native game starts as a plain command");
        };
        assert_eq!(
            command.get_program(),
            folder_b.join("bin/x64/game.exe").as_os_str()
        );
    }
}
