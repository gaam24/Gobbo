#[cfg(not(windows))]
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Command;

#[cfg(not(windows))]
use super::executable::launch_args;
#[cfg(not(windows))]
use super::start::{Launch, game_env, native_dlls_for_launch, overlay_session_state};
#[cfg(not(windows))]
use crate::context::Data;
#[cfg(not(windows))]
use crate::context::Settings;
#[cfg(not(windows))]
use crate::deploy::overlay::Launcher;
#[cfg(not(windows))]
use crate::error::AppError;
#[cfg(not(windows))]
use crate::games::{Executable, GameDef, steam};

/// The Steam prefix of a game, from its folder in the Steam library.
pub(super) fn steam_prefix(
    game: &GameDef,
    game_root: &Path,
) -> Result<steam::SteamPrefix, AppError> {
    steam::steam_prefix(game, game_root).ok_or_else(|| AppError::user("launch.prefix_unknown"))
}

/// A game through Proton from the Steam library: its prefix, the Steam variables, and the overlay when it is deployed.
pub(super) fn proton_command(
    game: &GameDef,
    data: &Data,
    settings: &Settings,
    game_root: &Path,
    exe: &Executable,
    home: &Path,
    with_mods: bool,
) -> Result<(Launch, bool, &'static str), AppError> {
    let (prefix, steam, proton) = proton_setup(game, settings, game_root, home)?;

    let program = proton.display().to_string();
    let mut args = vec![
        "waitforexitandrun".to_string(),
        game_root.join(&exe.path).display().to_string(),
    ];
    args.extend(launch_args(exe, settings));
    let overlay_state = overlay_session_state(data)?;
    let session = overlay_state
        .map(|state| super::overlay_session(data, game, settings, &state, game_root))
        .transpose()?;
    let native = native_dlls_for_launch(game, game_root, &exe.path, session.as_ref());
    crate::infra::log::info(format!(
        "launch: proton={} prefix={} native={native:?}",
        proton.display(),
        prefix.compatdata.display()
    ));
    // The settings' variables come first, and Gobbo's Steam variables override them.
    let inherited = std::env::var("WINEDLLOVERRIDES").ok();
    let mut env = game_env(settings, true, &native, inherited.as_deref())?;
    env.extend(steam_env(&prefix, &steam, game_root));
    let mut launch = Launch::new(program, args, env, session);

    if let Launch::Plain(command) = &mut launch {
        command.current_dir(game_root);
    }

    Ok((launch, with_mods, "launch.proton_failed"))
}

/// Runs an installer in the game's prefix through Proton and waits for it. It runs without the overlay, the mods and the
/// game's launch arguments. Returns its exit code.
pub(super) fn run_in_prefix(
    game: &GameDef,
    settings: &Settings,
    game_root: &Path,
    program: &Path,
    args: &[String],
    home: &Path,
) -> Result<i32, AppError> {
    let (prefix, steam, proton) = proton_setup(game, settings, game_root, home)?;
    let mut command = Command::new(&proton);
    command
        .arg("waitforexitandrun")
        .arg(program)
        .args(args)
        .current_dir(game_root)
        .envs(steam_env(&prefix, &steam, game_root));
    let mut running = Launch::Plain(command).start("launch.proton_failed", None)?;
    Ok(running.wait().unwrap_or(1))
}

/// The launcher of a game's steps: a Steam game starts its steps through Proton, as it starts itself, so a Windows program
/// of a step runs in the game's prefix. A game without Steam app ids starts its programs directly, and needs none.
pub(super) fn step_launcher(
    game: &GameDef,
    settings: &Settings,
    game_root: &Path,
    home: &Path,
) -> Result<Option<Launcher>, AppError> {
    if game.detect.steam_app_ids.is_empty() {
        return Ok(None);
    }

    let (prefix, steam, proton) = proton_setup(game, settings, game_root, home)?;
    Ok(Some(Launcher {
        program: proton.display().to_string(),
        args: vec!["waitforexitandrun".to_string()],
        env: steam_env(&prefix, &steam, game_root),
    }))
}

/// The game's Steam prefix, the Steam folder and the Proton that runs the game. A prefix that is not there is an error.
fn proton_setup(
    game: &GameDef,
    settings: &Settings,
    game_root: &Path,
    home: &Path,
) -> Result<(steam::SteamPrefix, PathBuf, PathBuf), AppError> {
    let prefix = steam_prefix(game, game_root)?;

    if !prefix.wine().is_dir() {
        return Err(crate::games::paths::prefix_missing());
    }

    let steam = crate::platform::steam_root(home, &prefix.library);
    let proton = proton_for(settings, &steam, &prefix.library, &prefix.app_id)?;
    Ok((prefix, steam, proton))
}

/// The variables Steam gives a game, which Proton needs to find the prefix.
fn steam_env(prefix: &steam::SteamPrefix, steam: &Path, game_root: &Path) -> Vec<(String, String)> {
    [
        (
            "STEAM_COMPAT_DATA_PATH",
            prefix.compatdata.display().to_string(),
        ),
        (
            "STEAM_COMPAT_CLIENT_INSTALL_PATH",
            steam.display().to_string(),
        ),
        ("STEAM_COMPAT_INSTALL_PATH", game_root.display().to_string()),
        ("SteamAppId", prefix.app_id.clone()),
        ("SteamGameId", prefix.app_id.clone()),
    ]
    .map(|(name, value)| (name.to_string(), value))
    .into()
}

/// Proton from the settings. When the field is empty, the Proton Steam assigns to the game, else the last Proton in the
/// Steam library. A chosen folder without `proton` is an error, not a silent switch to another Proton.
fn proton_for(
    settings: &Settings,
    steam: &Path,
    library: &Path,
    app_id: &str,
) -> Result<PathBuf, AppError> {
    let chosen = settings.proton_dir.trim();

    if chosen.is_empty() {
        return steam_proton(steam, library, app_id)
            .or_else(|| crate::platform::find_proton(steam, library))
            .ok_or_else(|| AppError::user("launch.proton_not_found"));
    }

    crate::platform::proton_at(Path::new(chosen))
        .ok_or_else(|| AppError::user("launch.proton_file_missing").with("folder", chosen))
}

/// The Proton that Steam assigns to the app, when Steam's `config/config.vdf` names an installed tool for it: the app's
/// own choice, else Steam's default. `None` when there is no such tool.
fn steam_proton(steam: &Path, library: &Path, app_id: &str) -> Option<PathBuf> {
    let config = std::fs::read_to_string(steam.join("config/config.vdf")).ok()?;
    let name = steam::compat_tool(&config, app_id)?;
    crate::platform::proton_named(steam, library, &name)
}
