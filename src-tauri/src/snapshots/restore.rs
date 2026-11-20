//! Restoring a snapshot: the preview, the fresh backup, the checked copy back and the materialization.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::Sha256;

use super::SnapshotScope;
use super::capture::create_in;
use super::manifest::{
    LEGACY_ID, Manifest, ManifestSet, SnapshotKind, Target, clamp_u32, files_bytes, fits, load,
    manifest_bytes,
};
use super::view::{RestoreOutcome, RestorePreview};
use super::{Data, Settings, game_running_at, load_profiles, profile_state, user_data};
use super::{relative_path, scope, set_dir_in};
use crate::context::profile::{history, lock_profiles};
use crate::domain::naming;
use crate::domain::profiles::Profile;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::disk;
use crate::infra::fs::{
    list_files, remove_dir_if_exists, remove_file_if_exists, replace_with_staged,
};
use crate::infra::io::digest_file;
use crate::infra::store::{unique_suffix, write_atomic};

/// What a restore would change, without writing anything. `running` tells the window to keep the action disabled.
pub fn preview(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    id: &str,
) -> Result<RestorePreview, AppError> {
    let scope = scope(app, &game.id)?;
    let manifest = load(&scope, id)?;
    let resolved = resolved_profile(&scope, &manifest)?;
    let targets = targets(&scope, game, settings, &manifest, &resolved.profile)?;
    let mut preview = RestorePreview {
        added: 0,
        replaced: 0,
        removed: 0,
        bytes: 0,
        free_kb: u32::MAX,
        enough_space: true,
        running: game_running_at(&game.id, &settings.game_root),
        mods: manifest.profile_state.as_ref().map_or(0, |state| {
            u32::try_from(state.list.mods.len()).unwrap_or(u32::MAX)
        }),
        recreates_profile: resolved.created,
    };
    let mut bytes = 0u64;
    let mut free_kb = u64::MAX;

    for (set, target) in &targets {
        let current: BTreeSet<String> = if target.is_dir() {
            list_files(target)
                .map_err(io_at(target))?
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut saved = BTreeSet::new();

        for file in &set.files {
            saved.insert(file.path.clone());
            let path = target.join(relative_path(&file.path)?);

            match fs::metadata(&path) {
                Ok(meta) if meta.is_file() && meta.len() == file.size => {
                    let digest = digest_file::<Sha256>(&path, &mut |_| {}).map_err(io_at(&path))?;

                    if digest != file.sha256 {
                        preview.replaced += 1;
                        bytes = bytes.saturating_add(file.size);
                    }
                }
                Ok(meta) if meta.is_file() => {
                    preview.replaced += 1;
                    bytes = bytes.saturating_add(file.size);
                }
                _ => {
                    preview.added += 1;
                    bytes = bytes.saturating_add(file.size);
                }
            }
        }

        preview.removed += u32::try_from(current.difference(&saved).count()).unwrap_or(u32::MAX);

        if let Some(space) = disk::space_for(target) {
            free_kb = free_kb.min(space.free_kb);

            if !fits(space.free_kb, files_bytes(&set.files)) {
                preview.bytes = clamp_u32(bytes);
                preview.free_kb = clamp_u32(free_kb);
                preview.enough_space = false;
                return Ok(preview);
            }
        }
    }
    // The fresh backup is written under the app's data folder when the profile still exists, so that disk needs the
    // snapshot size as well. A recreated profile has nothing to back up.
    if !resolved.created
        && let Some(space) = disk::space_for(&scope.app.dir)
    {
        free_kb = free_kb.min(space.free_kb);

        if !fits(space.free_kb, manifest_bytes(&manifest)) {
            preview.bytes = clamp_u32(bytes);
            preview.free_kb = clamp_u32(free_kb);
            preview.enough_space = false;
            return Ok(preview);
        }
    }

    preview.bytes = clamp_u32(bytes);
    preview.free_kb = clamp_u32(free_kb);
    Ok(preview)
}

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
fn resolved_profile(
    scope: &SnapshotScope,
    manifest: &Manifest,
) -> Result<ResolvedProfile, AppError> {
    let profiles = load_profiles(&scope.game)?;

    if let Ok(profile) = profiles.get(&manifest.profile) {
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

struct ResolvedProfile {
    profile: Profile,
    created: bool,
}

/// The name a recreated profile starts from. It is made unique when it is added.
fn restored_name(name: &str) -> String {
    let name = name.trim();

    if name.is_empty() {
        "Restored profile".to_string()
    } else {
        name.chars().take(60).collect()
    }
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
            && profiles.get(&manifest.profile).is_err()
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
        let profile = profiles.get_mut(&manifest.profile)?;
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
fn restore_redirect_state(upper: &Path, set: &ManifestSet) -> Result<(), AppError> {
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

/// The current target of every set in the manifest, by set id. A set that the definition no longer declares, or one
/// that is no longer isolated, cannot be restored into a profile store.
fn targets(
    scope: &SnapshotScope,
    game: &GameDef,
    settings: &Settings,
    manifest: &Manifest,
    profile: &Profile,
) -> Result<Vec<(ManifestSet, PathBuf)>, AppError> {
    let state = scope.game.profile_state(&profile.id);
    let sets = user_data::sets(game, settings, profile, &state)?;
    let mut targets = Vec::with_capacity(manifest.sets.len() + 1);

    for set in &manifest.sets {
        if set.id == LEGACY_ID {
            targets.push((set.clone(), profile_state::legacy_data(&state)));
            continue;
        }

        let Some(current) = sets.iter().find(|current| current.id == set.id) else {
            return Err(AppError::user("snapshot.set_missing").with("set", &set.id));
        };

        if !current.isolated {
            return Err(AppError::user("snapshot.set_shared").with("set", &set.id));
        }

        let target = if current.redirects() {
            current.upper.clone()
        } else {
            current.store.clone()
        };
        targets.push((set.clone(), target));
    }

    if let Some(overwrite) = &manifest.overwrite {
        targets.push((overwrite.clone(), profile_state::overwrite(&state)));
    }

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::naming;
    use crate::snapshots::capture::create;
    use crate::snapshots::manifest::{SETS_DIR, SnapshotKind, list};
    use crate::snapshots::test_support::{fixture, manifest_with, snapshot_with_file, store};
    use crate::test_support::write;

    #[test]
    fn a_snapshot_keeps_the_profile_store_and_a_restore_puts_it_back() {
        let (app, game, settings) = fixture("snapshot-roundtrip");
        let store = store(&app, &game.id);
        write(&store.join("slot.sav"), "one");
        let legacy = app
            .game(&game.id)
            .profile_state("p1")
            .join("data/legacy.sav");
        write(&legacy, "saved");

        let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        assert_eq!(snapshot.files, 2);
        assert_eq!(snapshot.bytes, 8);
        // The snapshot is stored in the app's folder, not in the game's data folder.
        assert!(
            app.dir
                .join("snapshots")
                .join(&game.id)
                .join(&snapshot.id)
                .is_dir()
        );
        assert!(!app.game(&game.id).dir.join("snapshots").exists());
        // A non-legacy set is stored under its path hash; the legacy store keeps its own name.
        let scope = SnapshotScope::new(&app, &game.id);
        let sets = scope.dir(&snapshot.id).join(SETS_DIR);
        assert!(
            sets.join(naming::path_hash("saves"))
                .join("slot.sav")
                .is_file()
        );
        assert!(sets.join(LEGACY_ID).join("legacy.sav").is_file());

        write(&store.join("slot.sav"), "two");
        write(&legacy, "changed");
        let preview = preview(&app, &game, &settings, &snapshot.id).unwrap();
        assert_eq!(preview.replaced, 2);
        assert_eq!(preview.added, 0);
        assert_eq!(preview.removed, 0);
        assert!(!preview.running);

        let outcome = restore(&app, &game, &settings, &snapshot.id).unwrap();
        assert_eq!(outcome.files, 2);
        assert_eq!(fs::read_to_string(store.join("slot.sav")).unwrap(), "one");
        assert_eq!(fs::read_to_string(&legacy).unwrap(), "saved");
        assert_eq!(
            fs::read_to_string(
                app.game(&game.id)
                    .profile_state("p1")
                    .join("data.old/legacy.sav")
            )
            .unwrap(),
            "changed"
        );
        assert_eq!(
            outcome.backup.expect("a backup").kind,
            SnapshotKind::BeforeRestore
        );
        assert_eq!(list(&app, &game.id).len(), 2);
    }

    #[test]
    fn a_restore_counts_an_added_and_a_removed_file() {
        let (app, game, settings) = fixture("snapshot-preview");
        let store = store(&app, &game.id);
        write(&store.join("slot.sav"), "one");

        let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        fs::remove_file(store.join("slot.sav")).unwrap();
        write(&store.join("other.sav"), "x");

        let preview = preview(&app, &game, &settings, &snapshot.id).unwrap();
        assert_eq!(preview.added, 1);
        assert_eq!(preview.removed, 1);
        assert_eq!(preview.bytes, 3);
    }

    #[test]
    fn a_tampered_snapshot_is_refused_and_leaves_the_store_alone() {
        let (app, game, settings, store, snapshot) = snapshot_with_file("snapshot-corrupt");
        let scope = SnapshotScope::new(&app, &game.id);
        let saved = set_dir_in(&scope.dir(&snapshot.id), "saves").join("slot.sav");
        write(&saved, "tampered");
        write(&store.join("slot.sav"), "current");

        let error = restore(&app, &game, &settings, &snapshot.id).unwrap_err();
        assert_eq!(error.code, "snapshot.corrupt");
        assert_eq!(
            fs::read_to_string(store.join("slot.sav")).unwrap(),
            "current"
        );
    }

    #[test]
    fn a_restore_puts_back_the_profile_list_and_the_overwrite() {
        let (app, game, settings) = fixture("snapshot-profile");
        let data = app.game(&game.id);
        let mut profiles = crate::context::load_profiles(&data).unwrap();
        let active = profiles.active_mut();
        active.mods.push(crate::domain::profiles::Entry {
            id: "local:one".to_string(),
            folder: "one".to_string(),
            enabled: true,
            priority: Some(3),
        });
        active.separators.push(crate::domain::profiles::Separator {
            id: "sep1".to_string(),
            name: "Group".to_string(),
            before: Some("local:one".to_string()),
        });
        data.save_profiles(&profiles).unwrap();
        let overwrite = data.profile_state("p1").join("overwrite");
        write(&overwrite.join("bin/config.ini"), "saved");

        let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        assert_eq!(snapshot.mods, 1);
        assert_eq!(snapshot.overwrite_files, 1);

        let mut profiles = crate::context::load_profiles(&data).unwrap();
        profiles.active_mut().mods.clear();
        profiles.active_mut().separators.clear();
        data.save_profiles(&profiles).unwrap();
        write(&overwrite.join("bin/config.ini"), "changed");

        let preview = preview(&app, &game, &settings, &snapshot.id).unwrap();
        assert_eq!(preview.mods, 1);
        assert!(!preview.recreates_profile);

        let outcome = restore(&app, &game, &settings, &snapshot.id).unwrap();
        assert!(!outcome.profile_created);
        assert_eq!(
            fs::read_to_string(overwrite.join("bin/config.ini")).unwrap(),
            "saved"
        );
        let profiles = crate::context::load_profiles(&data).unwrap();
        assert_eq!(profiles.active().mods.len(), 1);
        assert_eq!(profiles.active().mods[0].priority, Some(3));
        assert_eq!(profiles.active().separators[0].name, "Group");
    }

    #[test]
    fn a_restore_recreates_a_deleted_profile() {
        let (app, game, settings) = fixture("snapshot-recreate");
        let data = app.game(&game.id);
        let mut profiles = crate::context::load_profiles(&data).unwrap();
        let second = profiles.create("Second", None).unwrap();
        let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        profiles.delete("p1").unwrap();
        profiles.switch(&second).unwrap();
        data.save_profiles(&profiles).unwrap();

        let outcome = restore(&app, &game, &settings, &snapshot.id).unwrap();
        assert!(outcome.profile_created);
        assert_eq!(outcome.profile, "p1");
        assert!(outcome.backup.is_none());
        let profiles = crate::context::load_profiles(&data).unwrap();
        assert!(profiles.get("p1").is_ok());
    }

    #[test]
    fn a_restore_writes_the_whiteouts_and_the_seed_marker_of_a_redirect() {
        let base = crate::test_support::scratch!("snapshot-whiteouts");
        let upper = base.join("upper");
        fs::create_dir_all(&upper).unwrap();
        let mut set = manifest_with("1-redirect", 0).sets.remove(0);
        set.target = Target::Upper;
        set.whiteouts = Some("gone.txt\n".to_string());
        set.whiteouts_journal = Some("later.txt\n".to_string());
        set.seeded = true;
        restore_redirect_state(&upper, &set).unwrap();
        assert_eq!(
            fs::read_to_string(user_data::whiteouts(&upper)).unwrap(),
            "gone.txt\n"
        );
        assert_eq!(
            fs::read_to_string(user_data::whiteouts_journal(&upper)).unwrap(),
            "later.txt\n"
        );
        assert!(user_data::seeded(&upper).is_file());

        set.whiteouts = None;
        set.whiteouts_journal = None;
        set.seeded = false;
        restore_redirect_state(&upper, &set).unwrap();
        assert!(!user_data::whiteouts(&upper).exists());
        assert!(!user_data::whiteouts_journal(&upper).exists());
        assert!(!user_data::seeded(&upper).exists());
    }
}
