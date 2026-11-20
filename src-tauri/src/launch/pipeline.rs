//! The one recorded run: prepare the report, run the steps before, start the program, wait, record the exit, run the
//! steps after and save the profile data. A game, a tool and `gobbo run` all use this pipeline; they differ only in the
//! options and in the `Launch` they hand in.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::changes::{record_game_changes, watch_game_folder};
use super::output;
use super::report;
use super::start::Launch;
use super::steps::run_steps;
use super::watch::Snapshot;
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
    let after = AfterExit {
        data: data.clone(),
        game: game.clone(),
        home: home.to_path_buf(),
        state_root,
        session_root: PathBuf::from(settings.game_root.trim()),
        live_flag,
        record,
        notify: on_exit.take(),
        watched,
        steps: steps && with_mods,
        finish,
    };

    if background {
        std::thread::spawn(move || {
            let code = running.wait();

            if let Err(error) = after.run(code) {
                crate::infra::log::error(error);
            }
        });
        return Ok(Outcome {
            with_mods,
            exit_code: None,
        });
    }

    let code = running.wait();
    after.run(code)?;
    Ok(Outcome {
        with_mods,
        exit_code: code,
    })
}

/// What follows the exit of a launched program, the same for a background and a foreground run.
struct AfterExit {
    data: Data,
    game: GameDef,
    home: PathBuf,
    state_root: PathBuf,
    session_root: PathBuf,
    live_flag: Option<Arc<AtomicBool>>,
    record: bool,
    notify: Option<ExitNotice>,
    watched: Option<(PathBuf, Snapshot)>,
    steps: bool,
    finish: bool,
}

impl AfterExit {
    /// Records the exit, waits for the overlay to be released, runs the after-end steps and saves the profile data. A
    /// failed step is returned only after the profile data is saved, so it never loses what the game wrote.
    fn run(self, code: Option<i32>) -> Result<(), AppError> {
        let Self { data, game, .. } = &self;
        crate::infra::log::info(format!("launch: game={} exited with {code:?}", game.id));

        if self.record {
            report::update(data, |report| report.game_exit_code = code);
        }
        // The launcher can exit while the game's own processes still run in the overlay, such as Wine's services.
        // The game counts as running until the last of them has ended.
        overlay::wait_released(&self.session_root);

        if let Some(flag) = &self.live_flag {
            flag.store(false, Ordering::Release);
        }

        if let Some((root, before)) = &self.watched {
            record_game_changes(data, root, before);
        }
        // The settings are read again after the game exits: its steps may have changed its profile data.
        let steps = if self.steps {
            run_steps(
                data,
                game,
                Phase::AfterEnd,
                &self.home,
                &load_settings(data),
            )
        } else {
            Ok(())
        };

        if self.finish {
            super::finish_launch(data, game, &load_settings(data), &self.state_root);
        }

        if let Some(notify) = self.notify {
            notify(code);
        }

        steps
    }
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
