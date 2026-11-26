//! The one recorded run: prepare the report, run the steps before, start the program, wait, record the exit, run the
//! steps after and save the profile data. A game, a tool and `gobbo run` all use this pipeline; they differ only in the
//! options and in the `Launch` they hand in.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::changes::{record_game_changes, watch_game_folder};
use super::model::Launch;
use super::output;
use super::report;
use super::steps::run_steps;
use super::watch::Snapshot;
use crate::context::running::mark_started_with_finished;
use crate::context::{Data, Settings, load_settings};
use crate::deploy::overlay;
use crate::error::AppError;
use crate::games::{GameDef, Phase};

/// Called once when the program ends, with its exit code. The command turns it into a window event.
pub(super) type ExitNotice = Box<dyn FnOnce(Option<i32>) + Send>;

/// Called once the program has started, with the handle of the process it runs as. A tool uses it to watch its cancel
/// flag instead of searching the process list again.
pub(super) type SpawnNotice = Box<dyn FnOnce(crate::platform::Process) + Send>;

/// What a run does around the program. A game records and runs steps in the background; a tool only waits for the
/// program.
pub(super) struct RunOptions<'a> {
    data: &'a Data,
    /// The app data root, which owns `gobbo.lock`. `data` may be a game-level handle, whose own folder is not the root.
    state_root: PathBuf,
    game: &'a GameDef,
    settings: &'a Settings,
    home: &'a Path,
    /// The id for the live marker. `None` for a tool.
    game_id: Option<&'a str>,
    steps: bool,
    live: bool,
    background: bool,
    watch: bool,
    report: bool,
    /// Start a fresh report before the run.
    begin_report: bool,
    with_output: bool,
    finish: bool,
    /// Called when the program ends, so the window can stop polling. Tools leave it unset.
    on_exit: Option<ExitNotice>,
    /// The program path and the watcher to start with its process handle. Only a tool sets it.
    spawn_notice: Option<(PathBuf, SpawnNotice)>,
}

impl<'a> RunOptions<'a> {
    /// A game launched from the window: steps and a live marker, a background wait, output and a report, and the
    /// post-exit save of the profile data.
    pub(super) fn for_game(
        data: &'a Data,
        state_root: PathBuf,
        game: &'a GameDef,
        settings: &'a Settings,
        home: &'a Path,
        game_id: &'a str,
        on_exit: Option<ExitNotice>,
    ) -> Self {
        Self {
            data,
            state_root,
            game,
            settings,
            home,
            game_id: Some(game_id),
            steps: true,
            live: true,
            background: true,
            watch: true,
            report: true,
            begin_report: false,
            with_output: true,
            finish: true,
            on_exit,
            spawn_notice: None,
        }
    }

    /// A game started from the Steam launch option: a fresh report and a foreground wait; the caller is the process, so
    /// nothing is marked live in the app.
    pub(super) fn for_run_command(
        data: &'a Data,
        state_root: PathBuf,
        game: &'a GameDef,
        settings: &'a Settings,
        home: &'a Path,
        game_id: &'a str,
    ) -> Self {
        Self {
            data,
            state_root,
            game,
            settings,
            home,
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
            spawn_notice: None,
        }
    }

    /// A tool: only a detached wait, with no steps, report, live marker or post-exit save. The notice tells its caller
    /// how it ended.
    pub(super) fn for_tool(
        data: &'a Data,
        state_root: PathBuf,
        game: &'a GameDef,
        settings: &'a Settings,
        home: &'a Path,
        on_exit: Option<ExitNotice>,
        spawn_notice: Option<(PathBuf, SpawnNotice)>,
    ) -> Self {
        Self {
            data,
            state_root,
            game,
            settings,
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
            spawn_notice,
        }
    }
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
        spawn_notice,
    } = options;

    crate::infra::log::info(format!(
        "launch: game={} profile={} with_mods={with_mods} steps={steps} overlay={} background={background}",
        game.id,
        data.active_profile_id(),
        matches!(&launch, Launch::Layered { .. })
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

    // The process is started now, so the watcher gets its handle instead of searching for it and waiting for it to
    // appear. A program that is already gone needs no watcher: the wait thread still reports its exit.
    if let Some((program, notice)) = spawn_notice
        && let Some(process) = crate::platform::Process::by_program(&program)
    {
        notice(process);
    }

    if record {
        report::record_start(data, started, running.timings(), dll_report_us);
    }

    let started = if live {
        game_id.map(mark_started_with_finished)
    } else {
        None
    };
    // Both flags come from one launch, so an old wait thread can never mark a relaunched game finished.
    let live_flag = started.as_ref().map(|started| Arc::clone(&started.running));
    let live_finished = started.map(|started| started.finished);
    let after = AfterExit {
        data: data.clone(),
        game: game.clone(),
        home: home.to_path_buf(),
        state_root,
        session_root: PathBuf::from(settings.game_root.trim()),
        live_flag,
        live_finished,
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
    /// Cleared when the game's last process ends, while post-exit work is still running.
    live_flag: Option<Arc<AtomicBool>>,
    /// Set after the post-exit work and the save, belonging to the same launch as `live_flag`.
    live_finished: Option<Arc<AtomicBool>>,
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
        // The save is complete now, even when a step failed. A caller waiting to quit can exit without losing what
        // the game wrote; `steps` is returned below so the error still reaches the caller. The flag belongs to this
        // launch, so a game started while this wait ran keeps its own unfinished flag.
        if let Some(finished) = &self.live_finished {
            finished.store(true, Ordering::Release);
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
