//! What a game needs before the first apply, as one report: folders, loader, overlay support and free space. The report
//! reads the same `Status` the list shows, so a check cannot disagree with the header or the alerts.

use std::path::Path;

use serde::Serialize;

use crate::context::Data;
use crate::context::load_settings;
use crate::deploy::overlay;
use crate::deploy::target::deployment_root;
use crate::domain::pathkey::path_key;
use crate::error::{AppError, Message};
use crate::infra::disk;
use crate::setup::LoaderSetupState;

/// How a preflight check ended. A warning does not block an apply; a problem does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Ok,
    Warning,
    Problem,
}

/// One line of the setup checklist.
#[derive(Debug, Serialize, specta::Type)]
pub struct PreflightCheck {
    /// A stable id the interface uses to pick an action: `game_folder`, `settings_folder`, `mods_folder`, `loader`,
    /// `problems`, `overlay`, `space`, `running`.
    pub id: String,
    pub state: CheckState,
    pub message: Message,
    /// The file the game or its loader needs, when the check names one. The window can open its page with this path.
    pub requirement_path: Option<String>,
    /// The name the definition gives that file, for the action label.
    pub requirement_name: Option<String>,
}

/// The setup checklist of one game. `problems` and `warnings` are the full lists the mod list already shows, so the
/// dialog can name every item and not only the first one.
#[derive(Debug, Serialize, specta::Type)]
pub struct PreflightReport {
    pub ready: bool,
    pub mods_root: String,
    pub free_space_kb: u32,
    pub total_space_kb: u32,
    pub checks: Vec<PreflightCheck>,
    /// The loader package the game offers, with its source, version and installed state.
    pub loader: Option<crate::setup::LoaderSetupView>,
    pub problems: Vec<Message>,
    pub warnings: Vec<Message>,
}

fn check(id: &str, state: CheckState, message: Message) -> PreflightCheck {
    PreflightCheck {
        id: id.to_string(),
        state,
        message,
        requirement_path: None,
        requirement_name: None,
    }
}

/// The loader row for a required file that is missing. A file the definition links gets the Get action.
fn requirement_check(game: &crate::games::GameDef, problem: &Message) -> PreflightCheck {
    let requirement = problem.params.get("path").and_then(|path| {
        game.requirements
            .iter()
            .find(|requirement| path_key(&requirement.path) == path_key(path))
    });
    let Some(requirement) = requirement else {
        return check("loader", CheckState::Problem, problem.clone());
    };
    let linked = requirement.url.as_ref().map(|_| requirement);

    PreflightCheck {
        id: "loader".to_string(),
        state: CheckState::Problem,
        message: Message::new("preflight.loader.missing").with("name", &requirement.name),
        requirement_path: linked.map(|requirement| requirement.path.clone()),
        requirement_name: linked.map(|requirement| requirement.name.clone()),
    }
}

/// Builds the report from the game status. It does not write; `status` itself only makes the library and archive folders,
/// as the first look at a game already does.
pub fn preflight(root: &Data, game_id: &str) -> Result<PreflightReport, AppError> {
    let status = crate::status::status(root, game_id)?;
    let game = root.definition(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let loader_setup = crate::setup::view(root, &game, &settings).ok().flatten();
    let mut checks = Vec::new();

    let (state, message) = if status.game_ready {
        (CheckState::Ok, Message::new("preflight.gameFolder.ok"))
    } else {
        (
            CheckState::Problem,
            Message::new("preflight.gameFolder.missing"),
        )
    };
    checks.push(check("game_folder", state, message));

    let (state, message) = if !status.needs_settings_dir {
        (
            CheckState::Ok,
            Message::new("preflight.settingsFolder.notNeeded"),
        )
    } else if status.settings_ready {
        (CheckState::Ok, Message::new("preflight.settingsFolder.ok"))
    } else {
        (
            CheckState::Problem,
            Message::new("preflight.settingsFolder.missing"),
        )
    };
    checks.push(check("settings_folder", state, message));

    let (state, message) = if status.needs_mods_root && !status.mods_ready {
        (
            CheckState::Problem,
            Message::new("preflight.modsFolder.missing"),
        )
    } else {
        (
            CheckState::Ok,
            Message::new("preflight.modsFolder.ok").with("path", &status.mods_root),
        )
    };
    checks.push(check("mods_folder", state, message));

    let (state, message) = if status.mods.is_empty() {
        (CheckState::Problem, Message::new("preflight.mods.empty"))
    } else if !status.mods.iter().any(|mod_view| mod_view.enabled) {
        (
            CheckState::Problem,
            Message::new("preflight.mods.noneEnabled"),
        )
    } else {
        (CheckState::Ok, Message::new("preflight.mods.ok"))
    };
    checks.push(check("mods", state, message));

    let (state, message) = if status.user_data.is_empty() {
        (CheckState::Ok, Message::new("preflight.userData.none"))
    } else if let Some(set) = status.user_data.iter().find(|set| !set.available) {
        // A manual Windows install owns a prefix that does not exist until the first launch. A shared set does not
        // block mods; an isolated one cannot be prepared until the prefix exists.
        let message = if set.isolated {
            Message::new("preflight.userData.missing")
        } else {
            Message::new("preflight.userData.notYet")
        };
        (
            if set.isolated {
                CheckState::Problem
            } else {
                CheckState::Warning
            },
            message.with("id", &set.id).with("path", &set.path),
        )
    } else {
        (CheckState::Ok, Message::new("preflight.userData.ok"))
    };
    checks.push(check("user_data", state, message));

    let missing_requirement = status
        .problems
        .iter()
        .find(|problem| problem.code == "status.requirement_missing");
    let setup_state = loader_setup.as_ref().map(|setup| setup.state);
    let loader_message = |code: &str| {
        let setup = loader_setup.as_ref();
        Message::new(code)
            .with("name", setup.map_or("", |setup| setup.name.as_str()))
            .with("version", setup.map_or("", |setup| setup.version.as_str()))
    };
    let loader_needed = status
        .problems
        .iter()
        .any(|problem| problem.code == crate::status::stage::LOADER_MISSING);
    checks.push(match (setup_state, missing_requirement) {
        _ if loader_needed => check(
            "loader",
            CheckState::Problem,
            loader_message("preflight.loader.install"),
        ),
        (Some(LoaderSetupState::Update), _) => check(
            "loader",
            CheckState::Warning,
            loader_message("preflight.loader.update"),
        ),
        (_, Some(problem)) => requirement_check(&game, problem),
        // No enabled mod needs the loader yet, so a missing one is not a problem; a mod that needs it will ask.
        (Some(LoaderSetupState::Missing), None) => check(
            "loader",
            CheckState::Ok,
            loader_message("preflight.loader.optional"),
        ),
        (Some(LoaderSetupState::Installed), None) => check(
            "loader",
            CheckState::Ok,
            loader_message("preflight.loader.installed"),
        ),
        (None, None) => check(
            "loader",
            CheckState::Ok,
            Message::new("preflight.loader.ok"),
        ),
    });

    let other_problems = status
        .problems
        .iter()
        .filter(|problem| {
            problem.code != "status.requirement_missing"
                && problem.code != crate::status::stage::LOADER_MISSING
        })
        .count();
    if other_problems > 0 {
        checks.push(check(
            "problems",
            CheckState::Problem,
            Message::new("preflight.problems").with("count", other_problems),
        ));
    }

    let active = status
        .profiles
        .iter()
        .find(|profile| profile.active)
        .map(|profile| profile.id.clone())
        .unwrap_or_default();
    let state = data.profile_state(&active);
    let root = deployment_root(&data, &game, &settings)
        .unwrap_or_else(|_| std::path::PathBuf::from(settings.game_root.trim()));
    match overlay::check_support(&state, &root) {
        Ok(()) => checks.push(check(
            "overlay",
            CheckState::Ok,
            Message::new("preflight.overlay"),
        )),
        Err(error) => {
            let error: AppError = error.into();
            checks.push(check("overlay", CheckState::Problem, Message::from(error)));
        }
    }

    let target = if status.mods_root.trim().is_empty() {
        Path::new(&settings.game_root)
    } else {
        Path::new(&status.mods_root)
    };
    let space = disk::space_for(target);
    let (free_space_kb, total_space_kb) = space.map_or((0, 0), |space| {
        (
            u32::try_from(space.free_kb).unwrap_or(u32::MAX),
            u32::try_from(space.total_kb).unwrap_or(u32::MAX),
        )
    });

    match space {
        Some(space) if space.free_kb < 512 * 1024 => checks.push(check(
            "space",
            CheckState::Problem,
            Message::new("preflight.space.critical"),
        )),
        Some(space) if space.free_kb < 2 * 1024 * 1024 => checks.push(check(
            "space",
            CheckState::Warning,
            Message::new("preflight.space.low"),
        )),
        Some(_) => checks.push(check(
            "space",
            CheckState::Ok,
            Message::new("preflight.space.ok"),
        )),
        None => checks.push(check(
            "space",
            CheckState::Warning,
            Message::new("preflight.space.unknown"),
        )),
    }

    if crate::context::running::game_running_at(game_id, &settings.game_root) {
        checks.push(check(
            "running",
            CheckState::Warning,
            Message::new("preflight.running"),
        ));
    }

    let ready = checks
        .iter()
        .all(|check| check.state != CheckState::Problem);
    Ok(PreflightReport {
        ready,
        mods_root: status.mods_root,
        free_space_kb,
        total_space_kb,
        checks,
        loader: loader_setup,
        problems: status.problems,
        warnings: status.warnings,
    })
}
