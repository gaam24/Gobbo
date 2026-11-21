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
use crate::games::prefix::other_prefix;
#[cfg(not(windows))]
use crate::games::runtime::Runtime;
#[cfg(not(windows))]
use crate::games::stores::steam;
#[cfg(not(windows))]
use crate::games::{Executable, GameDef};

/// The Steam prefix of a game, from its folder in the Steam library.
pub(super) fn steam_prefix(
    game: &GameDef,
    game_root: &Path,
) -> Result<steam::SteamPrefix, AppError> {
    steam::steam_prefix(game, game_root).ok_or_else(|| AppError::user("launch.prefix_unknown"))
}

/// The Proton setup shared by a launch, a step and a prerequisite: the data folder Proton writes into, the build that
/// runs it, and the variables that tell Proton which install it is serving.
#[cfg(not(windows))]
struct Setup {
    compatdata: PathBuf,
    proton: PathBuf,
    store_env: Vec<(String, String)>,
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
    proton_command_with(
        game,
        data,
        settings,
        game_root,
        exe,
        home,
        with_mods,
        Runtime::SteamProton,
    )
}

/// A Windows program from another store: it starts through the newest Proton on this computer, in a prefix Gobbo owns.
pub(super) fn owned_proton_command(
    game: &GameDef,
    data: &Data,
    settings: &Settings,
    game_root: &Path,
    exe: &Executable,
    home: &Path,
    with_mods: bool,
) -> Result<(Launch, bool, &'static str), AppError> {
    proton_command_with(
        game,
        data,
        settings,
        game_root,
        exe,
        home,
        with_mods,
        Runtime::OwnedProton,
    )
}

/// The Proton launch itself, shared by the two runtimes: only the setup, the variables and the prefix differ.
#[cfg(not(windows))]
#[expect(
    clippy::too_many_arguments,
    reason = "a launch carries the game, its data and settings, the program, home and the runtime"
)]
fn proton_command_with(
    game: &GameDef,
    data: &Data,
    settings: &Settings,
    game_root: &Path,
    exe: &Executable,
    home: &Path,
    with_mods: bool,
    runtime: Runtime,
) -> Result<(Launch, bool, &'static str), AppError> {
    let setup = setup(data, game, settings, game_root, home, runtime)?;

    let program = setup.proton.display().to_string();
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
        setup.proton.display(),
        setup.compatdata.display()
    ));
    // The settings' variables come first, and Gobbo's Steam or owned variables override them.
    let inherited = std::env::var("WINEDLLOVERRIDES").ok();
    let mut env = game_env(settings, true, &native, inherited.as_deref())?;
    env.extend(setup.store_env);
    let mut launch = Launch::new(program, args, env, session);

    if let Launch::Plain(command) = &mut launch {
        command.current_dir(game_root);
    }

    Ok((launch, with_mods, "launch.proton_failed"))
}

/// Runs an installer in the game's prefix through Proton and waits for it. It runs without the overlay, the mods and the
/// game's launch arguments. Returns its exit code.
#[expect(
    clippy::too_many_arguments,
    reason = "an installer carries the game, its data and settings, the program, its arguments, home and the runtime"
)]
pub(super) fn run_in_prefix(
    game: &GameDef,
    data: &Data,
    settings: &Settings,
    game_root: &Path,
    program: &Path,
    args: &[String],
    home: &Path,
    runtime: Runtime,
) -> Result<i32, AppError> {
    let setup = setup(data, game, settings, game_root, home, runtime)?;
    let mut command = Command::new(&setup.proton);
    command
        .arg("waitforexitandrun")
        .arg(program)
        .args(args)
        .current_dir(game_root)
        .envs(setup.store_env);
    let mut running = Launch::Plain(command).start("launch.proton_failed", None)?;
    Ok(running.wait().unwrap_or(1))
}

/// The launcher of a game's steps: a Windows program of a step runs in the game's prefix, the Steam prefix for a Steam
/// install and the owned prefix for one from another store. A program of this system needs no launcher.
pub(super) fn step_launcher(
    game: &GameDef,
    data: &Data,
    settings: &Settings,
    game_root: &Path,
    home: &Path,
    runtime: Runtime,
) -> Result<Option<Launcher>, AppError> {
    if runtime == Runtime::Native {
        return Ok(None);
    }

    let setup = setup(data, game, settings, game_root, home, runtime)?;
    Ok(Some(Launcher {
        program: setup.proton.display().to_string(),
        args: vec!["waitforexitandrun".to_string()],
        env: setup.store_env,
    }))
}

/// The Proton setup of one runtime. A native launch never asks for one.
#[cfg(not(windows))]
fn setup(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    game_root: &Path,
    home: &Path,
    runtime: Runtime,
) -> Result<Setup, AppError> {
    match runtime {
        Runtime::SteamProton => steam_setup(game, settings, game_root, home),
        Runtime::OwnedProton => owned_setup(data, game, settings, game_root, home),
        Runtime::Native => Err(AppError::unexpected("a native launch has no Proton setup")),
    }
}

/// The Steam prefix, Steam folder and Proton of a Steam install. A prefix that is not there is an error: Steam must have
/// run the game once before Gobbo can start it in the same prefix.
#[cfg(not(windows))]
fn steam_setup(
    game: &GameDef,
    settings: &Settings,
    game_root: &Path,
    home: &Path,
) -> Result<Setup, AppError> {
    let prefix = steam_prefix(game, game_root)?;

    if !prefix.wine().is_dir() {
        return Err(crate::games::paths::prefix_missing());
    }

    let steam = crate::platform::steam_root(home, &prefix.library);
    let proton = proton_for(settings, &steam, &prefix.library, &prefix.app_id)?;
    Ok(Setup {
        compatdata: prefix.compatdata.clone(),
        proton,
        store_env: steam_env(&prefix, &steam, game_root),
    })
}

/// Proton's data folder for a non-Steam install and the Proton that runs it. There is no prefix requirement: the first
/// launch creates it.
#[cfg(not(windows))]
fn owned_setup(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    game_root: &Path,
    home: &Path,
) -> Result<Setup, AppError> {
    let compatdata = compatdata_of(&other_prefix(data, &game.id, &settings.prefix_dir))?;
    let installs = crate::games::stores::steam::steam_installs(home);
    let libraries = crate::games::stores::steam::libraries_in(&installs);
    let builds = crate::platform::installed_protons(&installs, &libraries);
    let proton = owned_proton_for(settings, &builds)?;
    let steam = crate::platform::steam_installs(home)
        .into_iter()
        .find(|dir| dir.is_dir());
    Ok(Setup {
        store_env: owned_env(&compatdata, game_root, steam.as_deref()),
        compatdata,
        proton,
    })
}

/// Proton's data folder for a Wine prefix: Proton keeps its prefix in `<data>/pfx`, so the prefix must be named `pfx`.
/// A prefix made by plain Wine (Lutris, or Heroic with Wine) is refused instead of letting Proton build a second prefix
/// inside it; its paths still resolve for mods and user data.
#[cfg(not(windows))]
fn compatdata_of(prefix: &Path) -> Result<PathBuf, AppError> {
    prefix
        .file_name()
        .is_some_and(|name| name == "pfx")
        .then(|| prefix.parent().map(Path::to_path_buf))
        .flatten()
        .ok_or_else(|| AppError::user("launch.prefix_not_proton").with("folder", prefix.display()))
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

/// The variables Proton needs for a prefix Gobbo owns. It has no Steam app id; the Steam client folder is passed on only
/// when Steam is installed, so Proton can find its runtime there.
#[cfg(not(windows))]
fn owned_env(compatdata: &Path, game_root: &Path, steam: Option<&Path>) -> Vec<(String, String)> {
    let mut env = vec![
        (
            "STEAM_COMPAT_DATA_PATH".to_string(),
            compatdata.display().to_string(),
        ),
        (
            "STEAM_COMPAT_INSTALL_PATH".to_string(),
            game_root.display().to_string(),
        ),
    ];

    if let Some(steam) = steam {
        env.push((
            "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_string(),
            steam.display().to_string(),
        ));
    }

    env
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

/// The build to use for an owned launch. A chosen folder wins and must hold a `proton` file; otherwise the newest build
/// from the same list the settings tab shows is used. An empty list is an error the interface can explain.
#[cfg(not(windows))]
fn owned_proton_for(
    settings: &Settings,
    builds: &[crate::platform::ProtonBuild],
) -> Result<PathBuf, AppError> {
    let chosen = settings.proton_dir.trim();

    if !chosen.is_empty() {
        return crate::platform::proton_at(Path::new(chosen))
            .ok_or_else(|| AppError::user("launch.proton_file_missing").with("folder", chosen));
    }

    builds
        .last()
        .map(|build| PathBuf::from(&build.folder).join("proton"))
        .ok_or_else(|| AppError::user("launch.proton_missing"))
}

/// The Proton that Steam assigns to the app, when Steam's `config/config.vdf` names an installed tool for it: the app's
/// own choice, else Steam's default. `None` when there is no such tool. Only the Linux
/// Proton path reads it, so Windows does not build it.
#[cfg(not(windows))]
fn steam_proton(steam: &Path, library: &Path, app_id: &str) -> Option<PathBuf> {
    let config = std::fs::read_to_string(steam.join("config/config.vdf")).ok()?;
    let name = steam::compat_tool(&config, app_id)?;
    crate::platform::proton_named(steam, library, &name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::ProtonBuild;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn a_proton_prefix_gives_its_data_folder_and_a_wine_prefix_is_refused() {
        assert_eq!(
            compatdata_of(Path::new("/prefixes/Slot/pfx")).unwrap(),
            Path::new("/prefixes/Slot")
        );
        let refused = compatdata_of(Path::new("/prefixes/Slot")).unwrap_err();
        assert_eq!(refused.code, "launch.prefix_not_proton");
    }

    #[test]
    fn an_owned_launch_without_an_installed_build_is_an_error() {
        let error = owned_proton_for(&Settings::default(), &[]).unwrap_err();
        assert_eq!(error.code, "launch.proton_missing");
    }

    #[test]
    fn an_owned_launch_takes_the_newest_build_from_the_settings_list() {
        let builds = [
            ProtonBuild {
                name: "Proton 8.0".to_string(),
                folder: "/tools/eight".to_string(),
            },
            ProtonBuild {
                name: "Proton 9.0".to_string(),
                folder: "/tools/nine".to_string(),
            },
        ];
        let chosen = owned_proton_for(&Settings::default(), &builds).unwrap();
        assert_eq!(chosen, PathBuf::from("/tools/nine/proton"));
    }

    #[test]
    fn a_chosen_proton_folder_wins_but_must_hold_the_proton_file() {
        let base = scratch!("owned-proton-choice");
        let chosen = base.join("Proton Custom");
        fs::create_dir_all(&chosen).unwrap();
        let settings = Settings {
            proton_dir: chosen.display().to_string(),
            ..Settings::default()
        };
        let builds = [ProtonBuild {
            name: "Proton 9.0".to_string(),
            folder: "/tools/nine".to_string(),
        }];
        assert_eq!(
            owned_proton_for(&settings, &builds).unwrap_err().code,
            "launch.proton_file_missing"
        );
        fs::write(chosen.join("proton"), "").unwrap();
        assert_eq!(
            owned_proton_for(&settings, &builds).unwrap(),
            chosen.join("proton")
        );
    }
}
