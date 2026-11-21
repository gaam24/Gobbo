//! The prerequisites of a tool: what is missing from the Proton prefix, and the installers that put it there.

use crate::context::Data;
use crate::context::Settings;
#[cfg(not(windows))]
use crate::context::load_stopped;
use crate::error::AppError;
use crate::games::GameDef;
use crate::games::Tool;
use std::path::Path;

#[cfg(not(windows))]
use super::running::tool_of;
#[cfg(not(windows))]
use crate::domain::naming;
#[cfg(not(windows))]
use crate::error::{ErrorKind, io_at};
#[cfg(not(windows))]
use crate::games::Prerequisite;
#[cfg(not(windows))]
use crate::games::prefix::other_prefix;
#[cfg(not(windows))]
use crate::games::runtime::Runtime;
#[cfg(not(windows))]
use crate::launch::executable::check_start;
#[cfg(not(windows))]
use crate::launch::proton::{run_in_prefix, steam_prefix};
#[cfg(not(windows))]
use sha2::Sha512;
#[cfg(not(windows))]
use std::fs;
#[cfg(not(windows))]
use std::path::PathBuf;

/// The installers of prerequisites, kept in the game's data folder.
#[cfg(not(windows))]
const PREREQUISITES_DIR: &str = "prerequisites";

/// The prerequisites of a tool that are not in the Proton prefix, by name. A Windows program checks the Steam prefix of
/// a Steam install and the prefix Gobbo owns for one from another store. A game without a prefix yet, or one that starts
/// a Linux program, has nothing to check: both give an empty list.
#[cfg(not(windows))]
pub fn missing_prerequisites(
    game: &GameDef,
    data: &Data,
    tool: &Tool,
    settings: &Settings,
) -> Vec<String> {
    let Some(wine) = crate::context::target::prefix(data, game, settings) else {
        return Vec::new();
    };
    tool.prerequisites
        .iter()
        .filter(|prerequisite| !is_installed(&wine, prerequisite))
        .map(|prerequisite| prerequisite.name.clone())
        .collect()
}

/// Windows runs the tool with the runtime it has, so nothing is checked there.
#[cfg(windows)]
pub const fn missing_prerequisites(
    _game: &GameDef,
    _data: &Data,
    _tool: &Tool,
    _settings: &Settings,
) -> Vec<String> {
    Vec::new()
}

/// Whether a prerequisite is in the prefix: its `check` folder holds a folder that starts with the expected name.
#[cfg(not(windows))]
fn is_installed(wine: &Path, prerequisite: &Prerequisite) -> bool {
    fs::read_dir(wine.join(&prerequisite.check)).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&prerequisite.check_starts_with)
        })
    })
}

/// Installs the prerequisites of a tool that are missing in the game's Proton prefix. Each installer comes from its own
/// link, is checked against its SHA-512, and runs through Proton. An exit code of 3010 means installed, with a restart
/// pending, and counts as done.
#[cfg(not(windows))]
pub fn install_prerequisites(
    root: &Data,
    game_id: &str,
    tool_id: &str,
    home: &Path,
) -> Result<(), AppError> {
    let (game, data, settings) = load_stopped(root, game_id)?;
    check_start(&settings)?;
    let tool = tool_of(&game, tool_id)?;
    let game_root = PathBuf::from(settings.game_root.trim());
    let runtime = crate::context::target::runtime(&game, &settings);
    let wine = match runtime {
        Runtime::SteamProton => steam_prefix(&game, &game_root)?.wine(),
        Runtime::OwnedProton => other_prefix(&data, &game.id, &settings.prefix_dir),
        Runtime::Native => return Ok(()),
    };

    for prerequisite in &tool.prerequisites {
        if is_installed(&wine, prerequisite) {
            continue;
        }

        let installer = fetch_installer(&data, prerequisite)?;
        let code = run_in_prefix(
            &game,
            &data,
            &settings,
            &game_root,
            &installer,
            &prerequisite.args,
            home,
            runtime,
        )?;

        if !matches!(code, 0 | 3010) || !is_installed(&wine, prerequisite) {
            return Err(AppError::new("tools.prerequisite_failed")
                .with("name", &prerequisite.name)
                .with("code", code)
                .kind(ErrorKind::Permanent));
        }
    }

    Ok(())
}

/// Windows runs a tool with the runtime it has, so there is nothing to install there.
#[cfg(windows)]
#[expect(clippy::unnecessary_wraps, reason = "the Linux version can fail")]
pub const fn install_prerequisites(
    _root: &Data,
    _game_id: &str,
    _tool_id: &str,
    _home: &Path,
) -> Result<(), AppError> {
    Ok(())
}

/// The installer of a prerequisite, kept in the game's data folder. A copy there that matches the SHA-512 is used
/// again; a download is kept only when it matches.
#[cfg(not(windows))]
fn fetch_installer(data: &Data, prerequisite: &Prerequisite) -> Result<PathBuf, AppError> {
    let dir = data.dir.join(PREREQUISITES_DIR);
    fs::create_dir_all(&dir).map_err(io_at(&dir))?;
    let file_name = prerequisite.url.rsplit('/').next().unwrap_or_default();
    let target = dir.join(naming::safe_limited_file_name(file_name));

    if target.is_file()
        && crate::infra::io::digest_file::<Sha512>(&target, &mut |_| {}).map_err(io_at(&target))?
            == prerequisite.sha512
    {
        return Ok(target);
    }

    let mut part = target.clone().into_os_string();
    part.push(crate::infra::http::PART_SUFFIX);
    let part = PathBuf::from(part);
    crate::infra::http::download(&prerequisite.url, &part, &mut |_, _| true)?;

    if crate::infra::io::digest_file::<Sha512>(&part, &mut |_| {}).map_err(io_at(&part))?
        != prerequisite.sha512
    {
        let _ = fs::remove_file(&part);
        return Err(AppError::new("tools.prerequisite_checksum")
            .with("name", &prerequisite.name)
            .kind(ErrorKind::Permanent));
    }

    fs::rename(&part, &target).map_err(io_at(&target))?;
    Ok(target)
}
