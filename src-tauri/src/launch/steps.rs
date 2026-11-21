use crate::context::Data;
use crate::context::Settings;
use crate::context::load_profiles;
use crate::deploy::overlay;
use crate::deploy::overlay::Session;
use crate::domain::mod_steps;
use crate::domain::mod_steps::ModSteps;
use crate::error::AppError;
use crate::games;
use crate::games::GameDef;
use crate::games::Phase;
use crate::infra::clock::now_seconds;
use crate::infra::text;
use crate::launch::report;
use crate::launch::report::OutputInfo;
use crate::launch::report::StepReport;
use crate::launch::report::StepStatus;
use crate::launch::start::overlay_session_state;
use std::collections::BTreeMap;
use std::path::Path;

/// The file in the game's data folder that names the steps each mod needs. Its keys are the mods' library folders.
const MOD_STEPS_FILE: &str = "mod_steps.toml";

/// The steps the enabled mods ask for in `phase`, run in the overlay in order. Without steps nothing happens.
/// Without an overlay deployment the steps cannot run, so the game starts with the mods as they are and the
/// report marks each planned step as skipped. A step that does not run still clears its outputs from the overlay,
/// so the game never loads one that the disabled mods left behind. A Steam game on Linux runs its steps through Proton,
/// so `home` is where Steam is looked for.
pub fn run_steps(
    data: &Data,
    game: &GameDef,
    phase: Phase,
    home: &Path,
    settings: &Settings,
) -> Result<(), AppError> {
    let steps = load_mod_steps(data)?;
    let enabled = enabled_mods(data)?;
    let planned = mod_steps::plan(&steps, &enabled, phase);
    let game_root = Path::new(settings.game_root.trim());
    let session = overlay_session_state(data)?.map(|state| {
        let deployment = crate::deploy::target::deployment_root(data, game, settings)
            .unwrap_or_else(|_| game_root.to_path_buf());
        (
            Session::with_roots(&state, &deployment, game_root, Vec::new()),
            state,
        )
    });

    if let Some((session, _)) = &session {
        for step in game
            .steps
            .iter()
            .filter(|step| step.phase == phase && !planned.contains(&step.id))
        {
            overlay::clear_outputs(session, step)?;
        }
    }

    if planned.is_empty() {
        return Ok(());
    }

    let Some((session, state)) = session else {
        for id in &planned {
            record_step_skipped(data, id, phase);
        }

        return Ok(());
    };
    let launcher = launcher_of(game, data, settings, home)?;

    for id in planned {
        record_step_running(data, &id, phase);
        let Some(step) = game.step(&id, phase) else {
            let error = fail_step(
                data,
                AppError::user("steps.unknown").with("step", &id),
                Vec::new(),
            );
            return Err(error);
        };

        match overlay::run_step(&state, &session, step, launcher.as_ref()) {
            Ok(ran) => {
                let (status, code) = if ran {
                    (StepStatus::Done, "steps.done")
                } else {
                    (StepStatus::Skipped, "steps.skipped")
                };
                let outputs = output_infos(&session, step);
                let log_tail = log_tail(&session, step);
                finish_step(data, status, code, BTreeMap::new(), outputs, log_tail);
            }
            Err(error) => {
                let error = AppError::from(error);
                let log_tail = log_tail(&session, step);
                return Err(fail_step(data, error, log_tail));
            }
        }
    }

    Ok(())
}

/// The launcher the steps of `game` start through. On Linux a Windows program of a step runs in the game's prefix: the
/// Steam prefix for a Steam install, one Gobbo owns for an install from another store. Windows starts it directly.
#[cfg(not(windows))]
fn launcher_of(
    game: &GameDef,
    data: &crate::context::Data,
    settings: &Settings,
    home: &Path,
) -> Result<Option<overlay::Launcher>, AppError> {
    let runtime = crate::context::target::runtime(game, settings);
    crate::launch::proton::step_launcher(
        game,
        data,
        settings,
        Path::new(settings.game_root.trim()),
        home,
        runtime,
    )
}

/// The launcher of the steps on Windows: none, the program starts directly.
#[cfg(windows)]
#[expect(
    clippy::missing_const_for_fn,
    clippy::unnecessary_wraps,
    reason = "the Linux version of this function can fail and uses all four arguments"
)]
fn launcher_of(
    _game: &GameDef,
    _data: &crate::context::Data,
    _settings: &Settings,
    _home: &Path,
) -> Result<Option<overlay::Launcher>, AppError> {
    Ok(None)
}

/// How many lines of a step's log the report keeps.
const LOG_TAIL_LINES: usize = 12;

fn record_step_running(data: &Data, id: &str, phase: Phase) {
    report::update(data, |r| {
        r.steps.push(StepReport {
            id: id.to_string(),
            phase,
            status: StepStatus::Running,
            code: "steps.running".to_string(),
            params: BTreeMap::new(),
            outputs: Vec::new(),
            log_tail: Vec::new(),
            started_at: now_seconds(),
            finished_at: None,
        });
    });
}

/// Records a step that did not run because the launch has no overlay session.
fn record_step_skipped(data: &Data, id: &str, phase: Phase) {
    record_step_running(data, id, phase);
    finish_step(
        data,
        StepStatus::Skipped,
        "steps.needs_overlay",
        BTreeMap::new(),
        Vec::new(),
        Vec::new(),
    );
}

/// Records a failed step and returns the error for the caller to propagate.
fn fail_step(data: &Data, error: AppError, log_tail: Vec<String>) -> AppError {
    finish_step(
        data,
        StepStatus::Failed,
        &error.code,
        error.params.clone(),
        Vec::new(),
        log_tail,
    );
    error
}

fn finish_step(
    data: &Data,
    status: StepStatus,
    code: &str,
    params: BTreeMap<String, String>,
    outputs: Vec<OutputInfo>,
    log_tail: Vec<String>,
) {
    report::update(data, |r| {
        if let Some(step) = r.steps.last_mut() {
            step.status = status;
            step.code = code.to_string();
            step.params = params;
            step.outputs = outputs;
            step.log_tail = log_tail;
            step.finished_at = Some(now_seconds());
        }
    });
}

/// The files a step declares as outputs, with their sizes, for the ones that exist.
fn output_infos(session: &Session, step: &games::Step) -> Vec<OutputInfo> {
    let layers = session.layers();
    step.outputs
        .iter()
        .filter_map(|rel| {
            let path = gobbo_overlay_core::join(&layers.upper, rel);
            let bytes = u32::try_from(path.metadata().ok()?.len()).unwrap_or(u32::MAX);
            Some(OutputInfo {
                path: rel.clone(),
                bytes,
            })
        })
        .collect()
}

/// The last lines of the step's log, when the step names one and the file exists in the layers.
fn log_tail(session: &Session, step: &games::Step) -> Vec<String> {
    let Some(rel) = &step.log else {
        return Vec::new();
    };
    let layers = session.layers();
    let Some((_, path)) = layers.resolve(rel, Path::is_file) else {
        return Vec::new();
    };
    text::tail_lines(&path, LOG_TAIL_LINES).unwrap_or_default()
}

/// `mod_steps.toml` from the game's data folder. Without the file no mod asks for steps.
pub fn load_mod_steps(data: &Data) -> Result<ModSteps, AppError> {
    let path = data.dir.join(MOD_STEPS_FILE);

    match std::fs::read_to_string(&path) {
        Ok(text) => ModSteps::parse(&text).map_err(|e| {
            AppError::user("steps.file_invalid")
                .with("path", path.display())
                .with("detail", e)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ModSteps::default()),
        Err(e) => Err(crate::error::io_at(&path)(e)),
    }
}

/// Writes `mod_steps.toml` in the game's data folder.
pub fn save_mod_steps(data: &Data, steps: &ModSteps) -> Result<(), AppError> {
    let path = data.dir.join(MOD_STEPS_FILE);
    let text = steps.to_text().map_err(AppError::unexpected)?;
    std::fs::create_dir_all(&data.dir).map_err(crate::error::io_at(&data.dir))?;
    Ok(crate::infra::store::write_atomic(&path, text.as_bytes())?)
}

/// The library folders of the enabled mods of the active profile, in profile order.
fn enabled_mods(data: &Data) -> Result<Vec<String>, AppError> {
    let profiles = load_profiles(data)?;
    Ok(profiles.active().enabled_folders())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn mod_steps_are_saved_and_read_back_from_their_file() {
        let base = scratch!("mod-steps");
        let data = Data::new(base.clone());
        assert!(load_mod_steps(&data).unwrap().mods.is_empty());

        let steps = ModSteps::parse("[mods.\"Mod A\"]\nbefore_start = [\"patch\"]\n").unwrap();
        save_mod_steps(&data, &steps).unwrap();

        assert!(base.join(MOD_STEPS_FILE).is_file());
        let back = load_mod_steps(&data).unwrap();
        assert_eq!(back.mods["Mod A"].before_start, ["patch"]);
    }

    #[test]
    fn a_steps_file_that_cannot_be_read_is_an_error_not_an_empty_list() {
        let base = scratch!("mod-steps-unreadable");
        std::fs::create_dir_all(base.join(MOD_STEPS_FILE)).unwrap();

        assert!(load_mod_steps(&Data::new(base)).is_err());
    }
}
