//! Paths and set resolution: which folders a profile owns, where Gobbo writes into an isolated redirect, and which
//! folders a redirect needs before a launch.

use std::fs;
use std::path::{Path, PathBuf};

use crate::context::target::path_context;
use crate::context::{Data, Settings};
use crate::domain::pathkey;
use crate::domain::profiles::Profile;
use crate::domain::user_data::{Mode, Set};
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::fs::{WalkErrors, walk};

const DATA_DIR: &str = "userdata";
/// The old upper layer kept aside when a redirect's data is moved back to the real folder.
const MOVED_UPPER_PREFIX: &str = "upper.moved-";
/// The files moved out of the upper layer by a completed move-back, so they can be recovered once.
const REMOVED_DIR: &str = "removed";

/// The folder of a profile's user-data sets inside its state folder.
pub fn state_dir(state: &Path) -> PathBuf {
    state.join(DATA_DIR)
}

/// Whether a folder next to a user-data set is a recovery copy from a completed move-back, not a live layer.
pub fn is_recovery(name: &str) -> bool {
    name.starts_with(MOVED_UPPER_PREFIX) || name == REMOVED_DIR
}

/// The whiteout snapshot next to a redirect's upper layer. It names paths the game deleted from the lower layers.
pub fn whiteouts(upper: &Path) -> PathBuf {
    upper.with_extension("whiteouts")
}

/// The journal of whiteout changes next to a redirect's upper layer.
pub fn whiteouts_journal(upper: &Path) -> PathBuf {
    upper.with_extension("whiteouts.journal")
}

/// The marker that a non-inherited redirect was seeded from the real folder once.
pub fn seeded(upper: &Path) -> PathBuf {
    upper.parent().unwrap_or(upper).join("seeded")
}

/// The real roots of a game's user-data sets, keyed by set id. A path that cannot be resolved is returned as an error
/// so each caller can decide whether it must fail or just leave the path out.
pub(in crate::context::profile) fn real_roots(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
) -> Vec<(String, Result<PathBuf, AppError>)> {
    if game.user_data.is_empty() || settings.game_root.trim().is_empty() {
        return Vec::new();
    }

    let mut context = path_context(data, game, settings);
    // A manual Windows install owns a prefix whose path is known before the first launch creates it. Resolve a shared
    // user-data set against that path so a missing prefix does not turn into a blocking mods problem; an isolated set
    // still fails in `prepare` with a clear message until the first launch.
    if context.prefix.is_none()
        && crate::context::target::runtime(game, settings)
            == crate::games::runtime::Runtime::OwnedProton
    {
        context.prefix = Some(crate::games::prefix::other_prefix(
            data,
            &game.id,
            &settings.prefix_dir,
        ));
    }

    game.user_data_sets()
        .iter()
        .map(|definition| {
            (
                definition.id.clone(),
                context.resolve(definition.path_for_platform(crate::platform::is_windows())),
            )
        })
        .collect()
}

/// The sets of one profile for the next launch or deployment. A missing profile choice means `shared`: a profile that
/// predates the definition must not start isolating data on its own.
pub fn sets(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
    state: &Path,
) -> Result<Vec<Set>, AppError> {
    if game.user_data.is_empty() || settings.game_root.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut sets = Vec::with_capacity(game.user_data.len());

    for (definition, (id, real)) in game
        .user_data_sets()
        .iter()
        .zip(real_roots(data, game, settings))
    {
        let real = real?;

        if real.exists() && !real.is_dir() {
            return Err(AppError::user("user_data.not_a_folder")
                .with("id", &id)
                .with("path", real.display()));
        }

        if pathkey::is_inside(Path::new(settings.game_root.trim()), &real) {
            return Err(AppError::user("user_data.inside_game").with("id", &id));
        }

        let isolated = profile.user_data_isolation(&definition.id).is_isolated();
        // The Windows hook is the first implementation. Linux keeps the copy fallback until the overlayfs mounts are
        // ready; do not claim a redirect the platform does not perform.
        let mode = if definition.mode == Mode::Redirect
            && crate::platform::supports_user_data_redirect()
        {
            Mode::Redirect
        } else {
            Mode::Copy
        };
        let folder = state.join(DATA_DIR).join(&definition.id);
        let base = if definition.inherit {
            real.clone()
        } else {
            folder.join("base")
        };
        sets.push(Set {
            id,
            kind: definition.kind,
            real,
            mode,
            isolated,
            inherit: definition.inherit,
            base,
            upper: folder.join("upper"),
            store: folder.join("copy"),
        });
    }

    check_overlaps(&sets)?;
    Ok(sets)
}

/// Creates the folders a redirect needs. Called before a deployment or a launch, not by the status view.
pub fn prepare(sets: &[Set]) -> Result<(), AppError> {
    for set in sets.iter().filter(|set| set.is_redirected()) {
        fs::create_dir_all(&set.upper).map_err(io_at(&set.upper))?;

        if !set.inherit {
            fs::create_dir_all(&set.base).map_err(io_at(&set.base))?;
        }
        // The game discovers the folder by listing its parent. An empty real folder makes the redirect visible without
        // writing any file into it. Never create the parent: a wrong prefix user or a drive that is not mounted must
        // be a clear error, not a new empty folder in the wrong place.
        if let Some(parent) = set.real.parent()
            && !parent.is_dir()
        {
            return Err(AppError::user("user_data.parent_missing").with("path", parent.display()));
        }

        fs::create_dir_all(&set.real).map_err(io_at(&set.real))?;
    }

    Ok(())
}

/// Gives a new isolated redirect its own copy of the existing folder once. The real folder stays the read-only source;
/// later launches read and write only the profile's upper layer. A marker makes this a one-time action, so a file the
/// user deletes in the game is not silently brought back from the real folder.
pub(super) fn seed_redirect(set: &Set) -> Result<(), AppError> {
    if !set.is_redirected() || set.inherit || !set.real.is_dir() {
        return Ok(());
    }

    let Some(folder) = set.upper.parent() else {
        return Ok(());
    };
    let marker = folder.join("seeded");

    if marker.exists() {
        return Ok(());
    }

    fs::create_dir_all(&set.upper).map_err(io_at(&set.upper))?;
    walk(
        &set.real,
        WalkErrors::Fail,
        &mut |entry, relative, is_dir| {
            let target = set.upper.join(relative);

            if is_dir {
                if !target.exists() {
                    fs::create_dir_all(&target)?;
                }
            } else if !target.exists() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::copy(entry.path(), &target)?;
            }

            Ok(())
        },
    )
    .map_err(io_at(&set.real))?;
    // The marker is written atomically, as the snapshot restore writes it, so a crash never leaves a half-written one.
    crate::infra::store::write_atomic(&marker, b"").map_err(AppError::from)
}

/// Where Gobbo writes a file of the game: the profile's upper layer for an isolated redirect, otherwise the real path.
pub fn write_path(sets: &[Set], path: &Path) -> PathBuf {
    sets.iter()
        .find_map(|set| set.write_path(path))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Whether a path belongs to an isolated redirect. Used by the settings writer to skip the legacy copy store.
pub fn is_redirected(sets: &[Set], path: &Path) -> bool {
    sets.iter()
        .any(|set| set.relative(path).is_some() && set.is_redirected())
}

/// One resolved set must not contain another. The definition check catches equal templates; this one catches different
/// templates that resolve to the same folder, for example `{documents}/Game` and `{home}/Documents/Game`.
fn check_overlaps(sets: &[Set]) -> Result<(), AppError> {
    for (index, first) in sets.iter().enumerate() {
        for second in &sets[index + 1..] {
            if first.relative(&second.real).is_some() || second.relative(&first.real).is_some() {
                return Err(AppError::user("user_data.overlap")
                    .with("first", &first.id)
                    .with("second", &second.id));
            }
        }
    }

    Ok(())
}
