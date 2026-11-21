use crate::context::Data;
use crate::context::load_profiles;
use crate::context::load_settings;
use crate::context::profile::lock_profiles;
use crate::context::profile::user_data;
use crate::context::running::game_running;
use crate::domain::user_data::{Isolation, Mode};
use crate::error::AppError;
use crate::error::game_running as game_is_running_error;
#[cfg(windows)]
use crate::error::io_at;
use crate::games;

/// Sets whether the profile keeps a user-data set separate. Refuses while the game runs: the next launch would see a
/// different layer than the running one, which is too easy to misread.
pub fn set_profile_user_data(
    root: &Data,
    game_id: &str,
    profile: &str,
    set_id: &str,
    isolated: bool,
) -> Result<(), AppError> {
    crate::context::edit_profiles(root, game_id, |_, game, profiles| {
        if !game.user_data.iter().any(|set| set.id == set_id) {
            return Err(AppError::user("user_data.unknown_set").with("id", set_id));
        }

        if game_running(root, game_id)? {
            return Err(game_is_running_error());
        }

        profiles.get_mut(profile)?.set_user_data_isolation(
            set_id,
            if isolated {
                Isolation::Isolated
            } else {
                Isolation::Shared
            },
        );
        Ok(())
    })
}

/// Moves a profile's isolated redirect data back into the real folder and makes the set shared again. The profile's
/// layer is merged over the real folder, then kept as `upper.moved-<unix>` so a bad merge can be recovered. A path the
/// game deleted (a whiteout) is moved to `removed/<unix>/` under the profile's set folder instead of being deleted.
///
/// The action refuses while the game runs, and for a set that is shared or copied. On Linux the overlayfs layer is not
/// moved back yet: the call answers `user_data.move_unsupported` and writes nothing.
pub fn move_profile_user_data(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    set_id: &str,
) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    let _profiles = lock_profiles();
    let data = root.game(game_id);
    let game = games::load(game_id)?;
    let state = data.profile_state(profile_id);
    let mut profiles = load_profiles(&data)?;
    let settings = load_settings(&data);

    if game_running(root, game_id)? {
        return Err(game_is_running_error());
    }

    let definition = game
        .user_data
        .iter()
        .find(|set| set.id == set_id)
        .ok_or_else(|| AppError::user("user_data.unknown_set").with("id", set_id))?;
    let profile = profiles.get(profile_id)?;

    if !profile.user_data_isolation(set_id).is_isolated() {
        return Err(AppError::user("user_data.not_isolated").with("id", set_id));
    }

    if definition.mode == Mode::Copy {
        return Err(AppError::user("user_data.not_redirect").with("id", set_id));
    }

    move_set(&data, &game, &settings, &state, profile, set_id)?;
    profiles
        .get_mut(profile_id)?
        .set_user_data_isolation(set_id, Isolation::Shared);
    data.save_profiles(&profiles)
}

/// The resolved sets of one profile, for the move and the size read.
fn resolved_sets(
    data: &Data,
    game: &crate::games::GameDef,
    settings: &crate::context::Settings,
    state: &std::path::Path,
    profile: &crate::domain::profiles::Profile,
) -> Result<Vec<crate::domain::user_data::Set>, AppError> {
    user_data::sets(data, game, settings, profile, state)
}

/// The set with `set_id`, or the user-facing `user_data.unknown_set` error.
fn find_set<'a>(
    sets: &'a [crate::domain::user_data::Set],
    set_id: &str,
) -> Result<&'a crate::domain::user_data::Set, AppError> {
    sets.iter()
        .find(|set| set.id == set_id)
        .ok_or_else(|| AppError::user("user_data.unknown_set").with("id", set_id))
}

/// Resolves the set, checks the real folder and moves the profile layer back. Windows only: the hook is the only
/// producer of this layer format.
#[cfg(windows)]
fn move_set(
    data: &Data,
    game: &crate::games::GameDef,
    settings: &crate::context::Settings,
    state: &std::path::Path,
    profile: &crate::domain::profiles::Profile,
    set_id: &str,
) -> Result<(), AppError> {
    let sets = resolved_sets(data, game, settings, state, profile)?;
    let set = find_set(&sets, set_id)?;

    if !set.redirects() {
        return Err(AppError::user("user_data.not_redirect").with("id", set_id));
    }

    check_real_folder(set)?;
    move_redirect_to_real(set)
}

/// The Linux overlayfs layer is not moved back yet; copying an overlay upper blindly could lose the files it hides.
#[cfg(not(windows))]
fn move_set(
    _data: &Data,
    _game: &crate::games::GameDef,
    _settings: &crate::context::Settings,
    _state: &std::path::Path,
    _profile: &crate::domain::profiles::Profile,
    set_id: &str,
) -> Result<(), AppError> {
    Err(AppError::user("user_data.move_unsupported").with("id", set_id))
}

/// The size of the profile's own data for one user-data set, in whole kilobytes. A redirect counts its upper layer and
/// a copy set its store; the real folder is never counted, because it belongs to every profile. A missing folder is
/// zero. This is computed on demand: `get_status` stays a pure read and does not walk the data.
pub fn user_data_size(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    set_id: &str,
) -> Result<u32, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let profiles = load_profiles(&data)?;
    let profile = profiles.get(profile_id)?;

    if !game.user_data.iter().any(|set| set.id == set_id) {
        return Err(AppError::user("user_data.unknown_set").with("id", set_id));
    }

    let sets = resolved_sets(
        &data,
        &game,
        &settings,
        &data.profile_state(profile_id),
        profile,
    )?;
    let set = find_set(&sets, set_id)?;

    if !set.isolated {
        return Ok(0);
    }

    let folder = if set.redirects() {
        &set.upper
    } else {
        &set.store
    };
    Ok(crate::context::size_kb(crate::infra::fs::folder_size(
        folder,
    )))
}

/// Whether the real folder can receive the profile layer: it is a folder or absent, and the folder above it exists.
#[cfg(windows)]
fn check_real_folder(set: &crate::domain::user_data::Set) -> Result<(), AppError> {
    if set.real.exists() && !set.real.is_dir() {
        return Err(AppError::user("user_data.not_a_folder")
            .with("id", &set.id)
            .with("path", set.real.display()));
    }

    if !set.real.exists()
        && let Some(parent) = set.real.parent()
        && !parent.is_dir()
    {
        return Err(AppError::user("user_data.parent_missing").with("path", parent.display()));
    }

    Ok(())
}

/// Merges the profile's upper layer into the real folder, moves the whiteout targets aside, and keeps the layer as a
/// recovery copy. Windows only: the hook is the only producer of this layer format.
#[cfg(windows)]
fn move_redirect_to_real(set: &crate::domain::user_data::Set) -> Result<(), AppError> {
    use crate::infra::clock::now_seconds;
    use crate::infra::fs::{CopyError, CopyMode, copy_tree, remove_file_if_exists};
    use std::fs;

    let unix = now_seconds();
    let removed = set.upper.with_file_name("removed").join(unix.to_string());

    // A whiteout names a path the game deleted from the lower layer. Move the real path aside before the merge, so the
    // merged copy of a path the game recreated is not removed again.
    for relative in whiteout_paths(&set.upper)? {
        let source = set.real.join(&relative);

        if !source.exists() {
            continue;
        }

        let target = removed.join(&relative);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }

        fs::rename(&source, &target).map_err(io_at(&source))?;
    }

    if set.upper.exists() {
        match copy_tree(&set.upper, &set.real, CopyMode::Merge) {
            Ok(()) => {}
            Err(CopyError::Conflict(conflict)) => {
                return Err(AppError::user("user_data.merge_conflict")
                    .with("path", conflict.path.display()));
            }
            Err(CopyError::Io(error)) => return Err(io_at(&set.real)(error)),
        }
    }

    rename_if_exists(&set.upper, &moved_path(&set.upper, unix, ""))?;

    for suffix in ["whiteouts", "whiteouts.journal"] {
        rename_if_exists(
            &set.upper.with_extension(suffix),
            &moved_path(&set.upper, unix, &format!(".{suffix}")),
        )?;
    }

    let marker = set.upper.with_file_name("seeded");
    remove_file_if_exists(&marker).map_err(io_at(&marker))?;
    Ok(())
}

/// The whiteout paths of a profile layer: the compacted snapshot plus the complete lines of its journal. A final line
/// without a newline is a write a crash interrupted, so it is left out.
#[cfg(windows)]
fn whiteout_paths(upper: &std::path::Path) -> Result<Vec<std::path::PathBuf>, AppError> {
    use std::fs;
    use std::io;

    let mut paths = Vec::new();
    let snapshot = upper.with_extension("whiteouts");

    match fs::read_to_string(&snapshot) {
        Ok(text) => paths.extend(whiteout_relatives(&text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_at(&snapshot)(error)),
    }

    let journal = upper.with_extension("whiteouts.journal");

    match fs::read(&journal) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(complete_lines(&bytes));
            paths.extend(whiteout_relatives(&text));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_at(&journal)(error)),
    }

    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

/// The complete lines of a whiteout journal. A final line without a newline is a write that a crash interrupted; it is
/// left out so a half path cannot move an unrelated file.
#[cfg(windows)]
fn complete_lines(bytes: &[u8]) -> &[u8] {
    match bytes.iter().rposition(|byte| *byte == b'\n') {
        Some(last) => &bytes[..=last],
        None => &[],
    }
}

/// The plain relative paths of a whiteout text. A path that is absolute or climbs out with `..` is not ours and is
/// left alone; the hook never writes one.
#[cfg(windows)]
fn whiteout_relatives(text: &str) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;

    gobbo_overlay_core::Whiteouts::parse(text)
        .iter()
        .filter_map(|path| {
            let mut relative = PathBuf::new();

            for part in path.split(['/', '\\']) {
                match part {
                    "" | "." => {}
                    ".." => return None,
                    part if part.contains(':') => return None,
                    part => relative.push(part),
                }
            }

            (!relative.as_os_str().is_empty()).then_some(relative)
        })
        .collect()
}

/// `base.moved-<unix><suffix>`, next to the layer.
#[cfg(windows)]
fn moved_path(base: &std::path::Path, unix: u32, suffix: &str) -> std::path::PathBuf {
    let name = base.file_name().unwrap_or_default().to_string_lossy();
    base.with_file_name(format!("{name}.moved-{unix}{suffix}"))
}

/// Renames a path when it exists. A missing path means an earlier run already moved it.
#[cfg(windows)]
fn rename_if_exists(from: &std::path::Path, to: &std::path::Path) -> Result<(), AppError> {
    if from.exists() {
        std::fs::rename(from, to).map_err(io_at(from))?;
    }

    Ok(())
}
