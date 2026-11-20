//! The record of the last launch: the game steps that ran, what each one did, and how the game ended. The interface
//! reads it, so the user sees what happens instead of only the final result. Like every text from the backend, a
//! step's state is a code with params; the interface turns the code into words.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::watch::GameChange;
use super::{Data, output};
use crate::deploy::overlay;
use crate::games::Phase;
use crate::infra::clock::now_seconds;

/// The file in the game's data folder that holds the last launch.
const REPORT_FILE: &str = "last_run.json";

/// How many of the newest overlay decisions the report carries. The ring keeps more; the console shows the tail.
const DECISION_LIMIT: usize = 200;

/// How many lines of Gobbo's own log the console shows.
const APP_LOG_LINES: usize = 200;

/// A ceiling for the app log tail, so a long session does not make every console refresh read the whole file.
const APP_LOG_BYTES: u64 = 512 * 1024;

/// The launcher's steps before the game's first instruction, in microseconds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub struct LaunchSteps {
    pub setup_us: u32,
    /// The crash-dump monitors, part of `setup_us`.
    pub dump_us: u32,
    /// The DLL list and its static-import read, which the report needs before the program starts.
    pub dll_report_us: u32,
    pub create_process_us: u32,
    pub detect_us: u32,
    pub stage0_us: u32,
    pub ready_event_us: u32,
    pub inject_us: u32,
    pub imports_us: u32,
    pub resume_us: u32,
    pub ready_wait_us: u32,
}

/// A microsecond count as the report stores it; a step longer than 71 minutes reads as the maximum.
fn micros(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl From<overlay::LaunchTimings> for LaunchSteps {
    fn from(timings: overlay::LaunchTimings) -> Self {
        Self {
            setup_us: micros(timings.setup_us),
            dump_us: micros(timings.dump_us),
            dll_report_us: micros(timings.dll_report_us),
            create_process_us: micros(timings.create_process_us),
            detect_us: micros(timings.detect_us),
            stage0_us: micros(timings.stage0_us),
            ready_event_us: micros(timings.ready_event_us),
            inject_us: micros(timings.inject_us),
            imports_us: micros(timings.imports_us),
            resume_us: micros(timings.resume_us),
            ready_wait_us: micros(timings.ready_wait_us),
        }
    }
}

/// One launch: when it started, the steps of both phases in the order they ran, and how the game ended.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RunReport {
    pub started_at: u32,
    pub finished_at: Option<u32>,
    pub game_started: bool,
    pub game_exit_code: Option<i32>,
    /// The user pressed Stop. A stopped game often has a nonzero exit code and must not be offered as a crash.
    #[serde(default)]
    pub stopped_by_user: bool,
    /// Wall time from the start request to the resumption of the game, including PE parsing, stage 0, static imports
    /// and injection. Old reports do not have it.
    #[serde(default)]
    pub launch_ms: Option<u32>,
    /// The launcher's own steps. Windows fills them from `overlay-launch`; Linux has no hook installer, so it is
    /// `None`. Old reports do not have it.
    #[serde(default)]
    pub launch_steps: Option<LaunchSteps>,
    pub steps: Vec<StepReport>,
    /// The end of the game's standard output and error. The file holds it, so it is filled when the report is read,
    /// not stored with it.
    #[serde(default)]
    pub output: Vec<String>,
    /// The game logs named by the definition that exist for this installation, newest file of each source first.
    /// Like `output`, this is filled when the report is read, so the console follows a running game.
    #[serde(default)]
    pub game_logs: Vec<GameLog>,
    /// Every `.log` file the mod layer and the overwrite layer hold, newest first. A mod adds its log without a
    /// definition change; the game's own log paths stay in `game_logs`.
    #[serde(default)]
    pub mod_logs: Vec<GameLog>,
    /// The end of Gobbo's own log. Filled when the report is read; it is not stored with the report.
    #[serde(default)]
    pub app_log: Vec<String>,
    /// What changed in the game folder while an overlay session ran. The game folder should not change then, so these
    /// are writes that got past the layers. Only the first changes are kept, and `game_changes_total` counts them all.
    #[serde(default)]
    pub game_changes: Vec<GameChange>,
    #[serde(default)]
    pub game_changes_total: u32,
    /// The newest decisions of the overlay hook in this launch, oldest first. Like `output`, the file on disk holds the
    /// ring and this list is filled when the report is read, so the console follows a running game.
    #[serde(default)]
    pub decisions: Vec<OverlayDecision>,
    /// How many decisions the hook recorded in this launch, including the ones the ring no longer holds.
    #[serde(default)]
    pub decisions_total: u32,
    /// The crash dump of this launch, when the hook wrote one.
    #[serde(default)]
    pub crash_dump: Option<String>,
    /// The DLLs an overlay launch found in the mod layer and the static imports it will point there. Filled when the
    /// launch starts; a launch without the overlay leaves it unset.
    #[serde(default)]
    pub dlls: Option<DllReport>,
}

/// The DLLs of one launch: what the mod layer holds, and what the launcher injects through the program's import table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct DllReport {
    /// The game-relative paths of every `.dll` in the mod layer.
    pub layer: Vec<String>,
    /// The static imports of the program that the launcher rewrites to the mod layer.
    pub injected: Vec<InjectedDll>,
}

/// One static import the launcher points at the mod layer.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct InjectedDll {
    /// The name as the import table spells it, for example `WINMM.dll`.
    pub name: String,
    /// Its game-relative path, for example `bin/x64/winmm.dll`.
    pub path: String,
    /// The full path of the file in the mod layer.
    pub source: String,
}

impl From<overlay::DllReport> for DllReport {
    fn from(report: overlay::DllReport) -> Self {
        Self {
            layer: report.layer,
            injected: report
                .injected
                .into_iter()
                .map(|dll| InjectedDll {
                    name: dll.name,
                    path: dll.path,
                    source: dll.source,
                })
                .collect(),
        }
    }
}

/// One decision of the overlay hook: a write, a copy-up, a whiteout, a missing file, a refusal or a pass-through.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OverlayDecision {
    /// The sequence number in the process's ring, capped at `u32::MAX` for the interface.
    pub seq: u32,
    /// The wall-clock time in seconds since the Unix epoch.
    pub time: u32,
    pub pid: u32,
    pub thread: u32,
    /// The stable kind name: `write`, `copy_up`, `whiteout`, `missing`, `refuse` or `pass_through`.
    pub kind: String,
    /// The layer the decision used: `none`, `upper`, `staging` or `root`.
    pub layer: String,
    /// The Windows error or NT status of a refusal or a missing file, zero otherwise.
    pub error: u32,
    /// The game-relative path, or the full path of a pass-through.
    pub path: String,
}

impl From<gobbo_overlay_decisions::Decision> for OverlayDecision {
    fn from(decision: gobbo_overlay_decisions::Decision) -> Self {
        Self {
            seq: u32::try_from(decision.seq).unwrap_or(u32::MAX),
            time: u32::try_from(decision.time_ms / 1000).unwrap_or(u32::MAX),
            pid: decision.pid,
            thread: decision.thread,
            kind: decision.kind.name().to_string(),
            layer: decision.layer.name().to_string(),
            error: decision.error,
            path: decision.path,
        }
    }
}

/// One log file of the game or its mods, as the console shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct GameLog {
    /// The universal kind: `script`, `loader`, `console`, `crash`, `mod` or `game`.
    pub kind: String,
    /// The file relative to the game folder, with `/` separators.
    pub path: String,
    /// The last lines of the file, oldest first.
    pub lines: Vec<String>,
    /// When the file was last written, in Unix seconds.
    pub modified_at: Option<u32>,
}

/// One step of a launch. `code` says what happened (`steps.running`, `steps.done`, `steps.skipped`, or the error code
/// of a failure), and `params` gives its values.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct StepReport {
    pub id: String,
    pub phase: Phase,
    pub status: StepStatus,
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub outputs: Vec<OutputInfo>,
    pub log_tail: Vec<String>,
    pub started_at: u32,
    pub finished_at: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Done,
    Skipped,
    Failed,
}

/// A file a step wrote into the overwrite layer, with its size when it exists.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OutputInfo {
    pub path: String,
    pub bytes: u32,
}

impl RunReport {
    pub const fn new(started_at: u32) -> Self {
        Self {
            started_at,
            finished_at: None,
            game_started: false,
            game_exit_code: None,
            stopped_by_user: false,
            launch_ms: None,
            launch_steps: None,
            steps: Vec::new(),
            output: Vec::new(),
            game_logs: Vec::new(),
            mod_logs: Vec::new(),
            app_log: Vec::new(),
            game_changes: Vec::new(),
            game_changes_total: 0,
            decisions: Vec::new(),
            decisions_total: 0,
            crash_dump: None,
            dlls: None,
        }
    }
}

/// The last launch of the game, or `None` when the game has not been launched from Gobbo yet.
pub fn read(data: &Data) -> Option<RunReport> {
    crate::infra::json::read_json_or_none(&data.dir.join(REPORT_FILE))
}

/// Remembers that the user stopped the running game, so the report is not offered as a crash.
pub fn mark_stopped(root: &Data, game_id: &str) {
    let data = root.game(game_id);
    // `stop_game` only calls this after it actually killed a process, so the current report belongs to that process even
    // when the wait thread has already recorded the exit. The exit code and the user's stop can arrive in either order.
    with_report_lock(&data, || {
        if let Some(mut report) = read(&data) {
            report.stopped_by_user = true;
            write(&data, &report);
        }
    });
}

/// The last launch of a game, by its id. `None` before the first launch from Gobbo.
pub fn last_run(root: &Data, game_id: &str) -> Option<RunReport> {
    let data = root.game(game_id);
    read(&data).map(|mut report| {
        report.output = output::tail(&data);
        report.app_log = app_log_tail(root);

        if let Ok(game) = crate::games::load(game_id) {
            let defined = crate::launch::logs::collect(&data, &game);
            report.mod_logs = crate::launch::logs::collect_mod_logs(&data, &game, &defined);
            report.game_logs = defined;
        }

        if let Some(state) = data.deployed_state() {
            let (decisions, total) = overlay::decisions(&state, DECISION_LIMIT);
            report.decisions = decisions.into_iter().map(OverlayDecision::from).collect();
            report.decisions_total = u32::try_from(total).unwrap_or(u32::MAX);
            report.crash_dump = overlay::latest_dump(&state);
        }

        report
    })
}

/// The end of the app's own log, from the data root. A missing log file gives an empty list.
fn app_log_tail(root: &Data) -> Vec<String> {
    crate::infra::text::tail_lines_with_limit(
        &root.dir.join(crate::infra::log::FILE_NAME),
        APP_LOG_LINES,
        APP_LOG_BYTES,
    )
    .unwrap_or_default()
}

/// Starts a new report for a launch, replacing the one of the previous launch.
pub fn begin(data: &Data) {
    with_report_lock(data, || write(data, &RunReport::new(now_seconds())));
}

/// Applies a change to the report, creating it when there is none. The report is only a record: a failure to write it
/// is not a failure of the launch, so it is ignored.
pub fn update(data: &Data, change: impl FnOnce(&mut RunReport)) {
    with_report_lock(data, || {
        let mut report = read(data).unwrap_or_else(|| RunReport::new(now_seconds()));
        change(&mut report);
        write(data, &report);
    });
}

/// Runs one report read-modify-write under the game's report lock. A test handle without caches has no contenders.
fn with_report_lock<T>(data: &Data, work: impl FnOnce() -> T) -> T {
    let Some(lock) = data.report_lock() else {
        return work();
    };
    let _guard = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    work()
}

/// Records that the program started: how long the start took since `started`, and the launcher's own steps with the
/// time the DLL report took before them.
pub fn record_start(
    data: &Data,
    started: std::time::Instant,
    timings: Option<overlay::LaunchTimings>,
    dll_report_us: u64,
) {
    let launch_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    let launch_steps = timings.map(|mut steps| {
        steps.dll_report_us = dll_report_us;
        steps.into()
    });
    update(data, |r| {
        r.game_started = true;
        r.launch_ms = Some(launch_ms);
        r.launch_steps = launch_steps;
    });
}

fn write(data: &Data, report: &RunReport) {
    let _ = crate::infra::json::write_json(&data.dir.join(REPORT_FILE), report);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::watch::ChangeKind;

    #[test]
    fn a_report_round_trips_through_json() {
        let report = RunReport {
            started_at: 10,
            finished_at: Some(20),
            game_started: true,
            game_exit_code: Some(0),
            stopped_by_user: false,
            launch_ms: Some(12),
            launch_steps: Some(LaunchSteps {
                setup_us: 4,
                dump_us: 1,
                dll_report_us: 2,
                create_process_us: 3,
                detect_us: 1,
                stage0_us: 2,
                ready_event_us: 1,
                inject_us: 2,
                imports_us: 1,
                resume_us: 1,
                ready_wait_us: 1,
            }),
            output: Vec::new(),
            game_logs: Vec::new(),
            mod_logs: Vec::new(),
            app_log: Vec::new(),
            game_changes: vec![GameChange {
                path: r"bin\x64\version.dll".to_string(),
                kind: ChangeKind::Created,
            }],
            game_changes_total: 1,
            decisions: Vec::new(),
            decisions_total: 0,
            crash_dump: None,
            dlls: Some(DllReport {
                layer: vec!["bin/x64/winmm.dll".to_string()],
                injected: vec![InjectedDll {
                    name: "WINMM.dll".to_string(),
                    path: "bin/x64/winmm.dll".to_string(),
                    source: r"C:\State\mods\bin\x64\winmm.dll".to_string(),
                }],
            }),
            steps: vec![StepReport {
                id: "redscript".to_string(),
                phase: Phase::BeforeStart,
                status: StepStatus::Done,
                code: "steps.done".to_string(),
                params: BTreeMap::new(),
                outputs: vec![OutputInfo {
                    path: "r6/cache/final.redscripts".to_string(),
                    bytes: 16_772_105,
                }],
                log_tail: vec!["Compilation complete".to_string()],
                started_at: 11,
                finished_at: Some(15),
            }],
        };
        let text = serde_json::to_string(&report).unwrap();
        let back: RunReport = serde_json::from_str(&text).unwrap();
        assert_eq!(back.steps[0].status, StepStatus::Done);
        assert_eq!(back.steps[0].outputs[0].bytes, 16_772_105);
        assert_eq!(back.game_changes[0].kind, ChangeKind::Created);
        assert_eq!(back.game_changes_total, 1);
        assert_eq!(back.dlls.as_ref().unwrap().layer, ["bin/x64/winmm.dll"]);
        assert_eq!(back.dlls.as_ref().unwrap().injected[0].name, "WINMM.dll");
    }
}

#[cfg(test)]
mod stop_tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn stopping_a_running_game_marks_the_report() {
        let root = Data::new(scratch!("report-stop"));
        let data = root.game("slot");
        let report = RunReport::new(10);
        write(&data, &report);
        mark_stopped(&root, "slot");
        assert!(read(&data).unwrap().stopped_by_user);
    }

    #[test]
    fn a_stop_that_races_the_exit_still_marks_the_report() {
        let root = Data::new(scratch!("report-stop-race"));
        let data = root.game("slot");
        let mut report = RunReport::new(10);
        // The wait thread recorded the killed process before `stop_game` calls `mark_stopped`.
        report.finished_at = Some(20);
        report.game_exit_code = Some(143);
        write(&data, &report);
        mark_stopped(&root, "slot");
        assert!(read(&data).unwrap().stopped_by_user);
    }
}
