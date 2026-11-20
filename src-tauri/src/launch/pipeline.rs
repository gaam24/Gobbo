//! The one recorded run: prepare the report, run the steps before, start the program, wait, record the exit, run the
//! steps after and save the profile data. A game, a tool and `gobbo run` all use this pipeline; they differ only in the
//! options and in the `Launch` they hand in.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::changes::{record_game_changes, watch_game_folder};
use super::output;
use super::report;
use super::start::Launch;
use super::steps::run_steps;
use crate::context::{Data, Settings, load_settings};
use crate::deploy::overlay;
use crate::error::AppError;
use crate::games::{GameDef, Phase};

/// Called once when the program ends, with its exit code. The command turns it into a window event.
pub(super) type ExitNotice = Box<dyn FnOnce(Option<i32>) + Send>;

/// What a run does around the program. A game records and runs steps in the background; a tool only waits for the
/// program.
pub(super) struct RunOptions<'a> {
    pub data: &'a Data,
    /// The app data root, which owns `gobbo.lock`. `data` may be a game-level handle, whose own folder is not the root.
    pub state_root: PathBuf,
    pub game: &'a GameDef,
    pub settings: &'a Settings,
    pub home: &'a Path,
    /// The id for the live marker. `None` for a tool.
    pub game_id: Option<&'a str>,
    pub steps: bool,
    pub live: bool,
    pub background: bool,
    pub watch: bool,
    pub report: bool,
    /// Start a fresh report before the run.
    pub begin_report: bool,
    pub with_output: bool,
    pub finish: bool,
    /// Called when the program ends, so the window can stop polling. Tools leave it unset.
    pub on_exit: Option<ExitNotice>,
}

/// What a run left behind. `exit_code` is `None` for a background run, whose wait thread records it later.
pub(super) struct Outcome {
    pub with_mods: bool,
    pub exit_code: Option<i32>,
}

/// Runs `launch` with the phases in `options`. A background run returns once the program has started; the wait thread
/// records the exit and the steps after it.
pub(super) fn run(
    options: RunOptions<'_>,
    launch: Launch,
    with_mods: bool,
    spawn_error: &'static str,
) -> Result<Outcome, AppError> {
    let RunOptions {
        data,
        state_root,
        game,
        settings,
        home,
        game_id,
        steps,
        live,
        background,
        watch,
        report: record,
        begin_report,
        with_output,
        finish,
        on_exit,
    } = options;

    crate::infra::log::info(format!(
        "launch: game={} profile={} with_mods={with_mods} steps={steps} overlay={} background={background}",
        game.id,
        data.active_profile_id(),
        matches!(&launch, crate::launch::start::Launch::Layered { .. })
    ));

    if begin_report {
        report::begin(data);
    }

    let dll_started = Instant::now();
    let dlls = launch_dlls(&launch);
    let dll_report_us = u64::try_from(dll_started.elapsed().as_micros()).unwrap_or(u64::MAX);

    if record {
        report::update(data, |report| report.dlls = dlls);
    }

    if steps && with_mods {
        run_steps(data, game, Phase::BeforeStart, home, settings)?;
    }

    let console = if with_output {
        match output::begin(data) {
            Ok(writer) => Some(writer),
            Err(error) => {
                crate::infra::log::warn(format!("launch: could not open the game output: {error}"));
                None
            }
        }
    } else {
        None
    };
    let watched = if watch { watch_game_folder(data) } else { None };
    let started = Instant::now();
    let mut running = launch.start(spawn_error, console)?;
    let mut on_exit = on_exit;

    if record {
        report::record_start(data, started, running.timings(), dll_report_us);
    }

    let live_flag = if live {
        game_id.map(crate::context::running::mark_started)
    } else {
        None
    };
    let session_root = PathBuf::from(settings.game_root.trim());

    if background {
        let data = data.clone();
        let game = game.clone();
        let home = home.to_path_buf();
        let notify = on_exit.take();
        std::thread::spawn(move || {
            let code = running.wait();
            crate::infra::log::info(format!("launch: game={} exited with {code:?}", game.id));

            if record {
                report::update(&data, |report| report.game_exit_code = code);
            }
            // The launcher can exit while the game's own processes still run in the overlay, such as Wine's services.
            // The game counts as running until the last of them has ended.
            overlay::wait_released(&session_root);

            if let Some(flag) = live_flag {
                flag.store(false, Ordering::Release);
            }

            if let Some(notify) = notify {
                notify(code);
            }

            if let Some((root, before)) = watched {
                record_game_changes(&data, &root, &before);
            }
            // The settings are read again after the game exits: its steps may have changed its profile data.
            if steps && with_mods {
                let after = load_settings(&data);

                if let Err(error) = run_steps(&data, &game, Phase::AfterEnd, &home, &after) {
                    crate::infra::log::error(error);
                }
            }

            if finish {
                let settings = load_settings(&data);
                super::finish_launch(&data, &game, &settings, &state_root);
            }
        });
        return Ok(Outcome {
            with_mods,
            exit_code: None,
        });
    }

    let code = running.wait();
    crate::infra::log::info(format!("launch: game={} exited with {code:?}", game.id));

    if let Some(flag) = live_flag {
        flag.store(false, Ordering::Release);
    }

    if record {
        report::update(data, |report| report.game_exit_code = code);
    }

    if let Some(notify) = on_exit {
        notify(code);
    }

    if let Some((root, before)) = watched {
        record_game_changes(data, &root, &before);
    }

    if steps && with_mods {
        let after = load_settings(data);
        run_steps(data, game, Phase::AfterEnd, home, &after)?;
    }

    if finish {
        let settings = load_settings(data);
        super::finish_launch(data, game, &settings, &state_root);
    }

    Ok(Outcome {
        with_mods,
        exit_code: code,
    })
}

/// The DLLs an overlay launch found in the mod layer and the static imports it will patch. A plain launch has none.
fn launch_dlls(launch: &Launch) -> Option<report::DllReport> {
    if let Launch::Layered {
        session, program, ..
    } = launch
    {
        return Some(report::DllReport::from(overlay::dll_report(
            session,
            Path::new(program),
        )));
    }

    None
}
