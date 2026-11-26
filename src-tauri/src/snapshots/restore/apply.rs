//! Writing a snapshot back: the fresh backup, the checked copy, the profile state and the materialization.

use std::fs;
use std::path::Path;

use sha2::Sha256;

use super::files::{restored_name, targets};
use super::preview::preview;
use crate::context::profile::{history, lock_profiles};
use crate::domain::naming;
use crate::domain::profiles::Profile;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::fs::{remove_dir_if_exists, remove_file_if_exists, replace_with_staged};
use crate::infra::io::digest_file;
use crate::infra::store::{unique_suffix, write_atomic};
use crate::snapshots::capture::create_in;
use crate::snapshots::manifest::{Manifest, ManifestSet, SnapshotKind, Target, load};
use crate::snapshots::view::RestoreOutcome;
use crate::snapshots::{
    Data, Settings, SnapshotScope, game_running_at, load_profiles, relative_path, scope,
    set_dir_in, user_data,
};

/// Restores a snapshot. The game must not be running; a fresh backup is written first, every snapshot file is checked
/// against its hash, and a profile that owns the data gets it materialized at once.
pub fn restore(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    id: &str,
) -> Result<RestoreOutcome, AppError> {
    let scope = scope(app, &game.id)?;
    let _state = crate::context::lock_state(&app.dir)?;

    if game_running_at(&game.id, &settings.game_root) {
        return Err(
            AppError::user("snapshot.game_running").recovery(crate::error::Recovery::new(
                crate::error::RecoveryAction::CloseGame,
            )),
        );
    }

    let manifest = load(&scope, id)?;
    let resolved = resolved_profile(&scope, &manifest)?;
    let before = preview(app, game, settings, id)?;

    if !before.enough_space {
        return Err(AppError::user("snapshot.no_space")
            .with("needed", before.bytes)
            .with("free", before.free_kb));
    }

    let backup = if resolved.created {
        None
    } else {
        Some(create_in(
            &scope,
            game,
            settings,
            &manifest.profile,
            SnapshotKind::BeforeRestore,
        )?)
    };
    let targets = targets(&scope, game, settings, &manifest, &resolved.profile)?;
    let full = manifest.profile_state.is_some();
    let mut files = 0u32;

    for (set, target) in &targets {
        let source = set_dir_in(&scope.dir(id), &set.id);
        files = files.saturating_add(restore_set(&source, target, set, full)?);
    }

    apply_profile_state(&scope, &manifest, &resolved)?;
    // A profile that owns the real folders sees the restored copy at once; a redirected set is already live in its layer.
    crate::context::profile::restore_owned(
        &scope.game,
        game,
        settings,
        &resolved.profile.id,
        false,
    )?;
    Ok(RestoreOutcome {
        backup,
        files,
        profile: resolved.profile.id,
        profile_created: resolved.created,
    })
}

/// The profile a snapshot belongs to, as it is now or as it was saved. A deleted profile is rebuilt from the saved
/// settings; without saved settings there is nothing to restore.
pub(super) fn resolved_profile(
    scope: &SnapshotScope,
    manifest: &Manifest,
) -> Result<ResolvedProfile, AppError> {
    let profiles = load_profiles(&scope.game)?;

    if let Ok(profile) = profiles.profile(&manifest.profile) {
        let mut profile = profile.clone();

        if let Some(saved) = &manifest.profile_state {
            profile.user_data.clone_from(&saved.user_data);
            profile.pack.clone_from(&saved.pack);
            profile.collections.clone_from(&saved.collections);
        }

        return Ok(ResolvedProfile {
            profile,
            created: false,
        });
    }

    let Some(saved) = &manifest.profile_state else {
        return Err(
            AppError::user("snapshot.profile_missing").with("profile", &manifest.profile_name)
        );
    };

    if !naming::is_plain_folder(&manifest.profile) {
        return Err(AppError::user("snapshot.corrupt").with("file", &manifest.profile));
    }

    let name = restored_name(&manifest.profile_name);
    let mut profile = Profile::new(manifest.profile.clone(), name, Vec::new());
    profile.restore_list(saved.list.clone());
    profile.user_data.clone_from(&saved.user_data);
    profile.pack.clone_from(&saved.pack);
    Ok(ResolvedProfile {
        profile,
        created: true,
    })
}

/// The profile a snapshot is restored into, and whether it had to be recreated.
pub(super) struct ResolvedProfile {
    pub(super) profile: Profile,
    pub(super) created: bool,
}

/// Saves the snapshot's profile settings. An existing profile keeps its deployment ledger: the restored list is a
/// draft until the next apply. A missing profile is added with its saved id when that id is free.
fn apply_profile_state(
    scope: &SnapshotScope,
    manifest: &Manifest,
    resolved: &ResolvedProfile,
) -> Result<(), AppError> {
    let Some(saved) = &manifest.profile_state else {
        return Ok(());
    };
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&scope.game)?;

    if resolved.created {
        let id = if naming::is_plain_folder(&manifest.profile)
            && profiles.profile(&manifest.profile).is_err()
        {
            manifest.profile.clone()
        } else {
            profiles.next_id()
        };
        let name = profiles.free_name(&restored_name(&manifest.profile_name));
        let mut profile = Profile::new(id, name, Vec::new());
        profile.restore_list(saved.list.clone());
        profile.user_data.clone_from(&saved.user_data);
        profile.pack.clone_from(&saved.pack);
        profile.collections.clone_from(&saved.collections);
        profiles.profiles.push(profile);
    } else {
        let profile = profiles.profile_mut(&manifest.profile)?;
        profile.restore_list(saved.list.clone());
        profile.user_data.clone_from(&saved.user_data);
        profile.pack.clone_from(&saved.pack);
        profile.collections.clone_from(&saved.collections);
    }

    history::save_profiles(&scope.game, &profiles)
}

/// Copies the listed files from the snapshot into a staging folder, checking every hash, and then replaces the target
/// with the staged tree. The previous generation stays as `<name>.old`. An empty overwrite set clears its folder; a
/// full snapshot also restores the whiteouts and the seed marker of a redirect.
fn restore_set(
    source: &Path,
    target: &Path,
    set: &ManifestSet,
    full: bool,
) -> Result<u32, AppError> {
    let files = &set.files;

    if files.is_empty() {
        if set.target == Target::Overwrite || (full && set.target == Target::Upper) {
            remove_dir_if_exists(target).map_err(io_at(target))?;

            if full && set.target == Target::Upper {
                restore_redirect_state(target, set)?;
            }

            return Ok(0);
        }

        return Err(AppError::user("snapshot.corrupt").with("file", target.display()));
    }

    let parent = target.parent().unwrap_or(target);
    fs::create_dir_all(parent).map_err(io_at(parent))?;
    let staging = parent.join(format!(".gobbo-restore-{}", unique_suffix()));
    let result = (|| -> Result<u32, AppError> {
        for file in files {
            let relative = relative_path(&file.path)?;
            let from = source.join(&relative);
            let to = staging.join(&relative);

            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(io_at(parent))?;
            }

            fs::copy(&from, &to).map_err(io_at(&from))?;
            let meta = fs::metadata(&to).map_err(io_at(&to))?;
            let digest = digest_file::<Sha256>(&to, &mut |_| {}).map_err(io_at(&to))?;

            if meta.len() != file.size || digest != file.sha256 {
                return Err(AppError::user("snapshot.corrupt").with("file", &file.path));
            }
        }

        replace_with_staged(&staging, target, true).map_err(io_at(target))?;

        if full && set.target == Target::Upper {
            restore_redirect_state(target, set)?;
        }

        Ok(u32::try_from(files.len()).unwrap_or(u32::MAX))
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Restores the whiteouts and the seed marker next to a redirect's upper layer. An old snapshot without them leaves the
/// current whiteouts alone.
pub(super) fn restore_redirect_state(upper: &Path, set: &ManifestSet) -> Result<(), AppError> {
    if let Some(parent) = upper.parent() {
        fs::create_dir_all(parent).map_err(io_at(parent))?;
    }

    write_optional(&user_data::whiteouts(upper), set.whiteouts.as_deref())?;
    write_optional(
        &user_data::whiteouts_journal(upper),
        set.whiteouts_journal.as_deref(),
    )?;
    let marker = user_data::seeded(upper);

    if set.seeded {
        write_atomic(&marker, b"").map_err(AppError::from)?;
    } else {
        remove_file_if_exists(&marker).map_err(io_at(&marker))?;
    }

    Ok(())
}

/// Writes a text file, or removes it when the snapshot does not hold it.
fn write_optional(path: &Path, text: Option<&str>) -> Result<(), AppError> {
    text.map_or_else(
        || remove_file_if_exists(path).map_err(io_at(path)),
        |text| write_atomic(path, text.as_bytes()).map_err(AppError::from),
    )
}
