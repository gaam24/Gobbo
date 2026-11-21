//! Taking a snapshot: copying the profile's stores and recording every file in the manifest.

use std::fs;
use std::path::Path;

use sha2::Sha256;

use super::manifest::{
    LEGACY_ID, Manifest, ManifestFile, ManifestSet, OVERWRITE_ID, ProfileBackup, SnapshotKind,
    Target, VERSION, save, total_of,
};
use super::view::{SnapshotView, view_of};
use super::{SnapshotScope, scope, set_dir_in};
use crate::context::Data;
use crate::context::Settings;
use crate::context::load_profiles;
use crate::context::profile::state as profile_state;
use crate::context::profile::user_data;
use crate::domain::user_data::Kind;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::clock::now_seconds;
use crate::infra::fs::{copy_tree_hashed, remove_dir_if_exists};
use crate::infra::store::unique_suffix;

/// Saves the profile's data and answers the new snapshot. `app` is the app's data folder; the snapshot is stored under
/// it, while the profile is read from `app.game(&game.id)`. An error names the first file or folder that could not be
/// read; a partial folder is removed, so a failed snapshot is never listed.
pub fn create(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    kind: SnapshotKind,
) -> Result<SnapshotView, AppError> {
    let scope = scope(app, &game.id)?;
    create_in(&scope, game, settings, profile, kind)
}

/// The same as `create`, in a scope the caller already has. An automatic snapshot uses this.
pub(super) fn create_in(
    scope: &SnapshotScope,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    kind: SnapshotKind,
) -> Result<SnapshotView, AppError> {
    let profiles = load_profiles(&scope.game)?;
    let name = profiles.get(profile)?.name.clone();
    let created_at = now_seconds();
    let id = format!("{created_at}-{}", unique_suffix());
    let dir = scope.dir(&id);
    let result = capture(
        scope, game, settings, profile, &name, kind, created_at, &id, &dir,
    );

    match result {
        Ok(manifest) => {
            if let Err(error) = save(scope, &dir, &manifest) {
                let _ = fs::remove_dir_all(&dir);
                return Err(error);
            }

            Ok(view_of(&manifest))
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&dir);
            Err(error)
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the values are the manifest fields this function fills"
)]
fn capture(
    scope: &SnapshotScope,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    profile_name: &str,
    kind: SnapshotKind,
    created_at: u32,
    id: &str,
    dir: &Path,
) -> Result<Manifest, AppError> {
    let profiles = load_profiles(&scope.game)?;
    let profile_view = profiles.get(profile)?;
    let state = scope.game.profile_state(profile);
    let sets = user_data::sets(&scope.game, game, settings, profile_view, &state)?;
    let mut manifest_sets = Vec::new();

    for set in sets.iter().filter(|set| set.isolated) {
        let (target, source) = if set.redirects() {
            (Target::Upper, &set.upper)
        } else {
            (Target::Copy, &set.store)
        };
        let (whiteouts, whiteouts_journal, seeded) = if set.redirects() {
            (
                read_optional(&user_data::whiteouts(&set.upper))?,
                read_optional(&user_data::whiteouts_journal(&set.upper))?,
                user_data::seeded(&set.upper).is_file(),
            )
        } else {
            (None, None, false)
        };
        // A redirect can hide files with whiteouts even when its upper layer is gone. Copy the state without files then.
        if !source.is_dir() && whiteouts.is_none() && whiteouts_journal.is_none() && !seeded {
            continue;
        }

        let set_dir = set_dir_in(dir, &set.id);
        let files = if source.is_dir() {
            copy_set(source, &set_dir)?.unwrap_or_default()
        } else {
            Vec::new()
        };
        manifest_sets.push(ManifestSet {
            id: set.id.clone(),
            kind: set.kind,
            target,
            bytes: total_of(&files),
            files,
            whiteouts,
            whiteouts_journal,
            seeded,
        });
    }

    let legacy = profile_state::legacy_data(&state);

    if legacy.is_dir() {
        let set_dir = set_dir_in(dir, LEGACY_ID);

        if let Some(files) = copy_set(&legacy, &set_dir)? {
            manifest_sets.push(ManifestSet {
                id: LEGACY_ID.to_string(),
                kind: Kind::UserData,
                target: Target::Legacy,
                bytes: total_of(&files),
                files,
                whiteouts: None,
                whiteouts_journal: None,
                seeded: false,
            });
        }
    }

    let overwrite_dir = profile_state::overwrite(&state);
    let overwrite = capture_overwrite(&overwrite_dir, &set_dir_in(dir, OVERWRITE_ID))?;
    let profile_state = ProfileBackup {
        list: profile_view.list_state(),
        user_data: profile_view.user_data.clone(),
        pack: profile_view.pack.clone(),
        collections: profile_view.collections.clone(),
    };
    Ok(Manifest {
        version: VERSION,
        id: id.to_string(),
        game: game.id.clone(),
        profile: profile.to_string(),
        profile_name: profile_name.to_string(),
        kind,
        created_at,
        sets: manifest_sets,
        profile_state: Some(profile_state),
        overwrite: Some(overwrite),
    })
}

/// Reads a whiteout file, or `None` when it does not exist. An unreadable one fails the snapshot instead of silently
/// losing the paths the game deleted.
fn read_optional(path: &Path) -> Result<Option<String>, AppError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_at(path)(error)),
    }
}

/// Copies the overwrite layer. An empty layer is still recorded, so a restore can clear it.
fn capture_overwrite(source: &Path, target: &Path) -> Result<ManifestSet, AppError> {
    let files = if source.is_dir() {
        copy_set(source, target)?.unwrap_or_default()
    } else {
        let _ = remove_dir_if_exists(target);
        Vec::new()
    };
    Ok(ManifestSet {
        id: OVERWRITE_ID.to_string(),
        kind: Kind::Config,
        target: Target::Overwrite,
        bytes: total_of(&files),
        files,
        whiteouts: None,
        whiteouts_journal: None,
        seeded: false,
    })
}

/// Copies a set folder and records every copied file in the same pass. `None` means the folder had no files; the empty
/// target is removed. The hash comes from the bytes written, so a file the game writes during the copy still has a
/// matching manifest.
fn copy_set(source: &Path, target: &Path) -> Result<Option<Vec<ManifestFile>>, AppError> {
    let mut files: Vec<ManifestFile> = copy_tree_hashed::<Sha256>(source, target)
        .map_err(io_at(target))?
        .into_iter()
        .map(|(path, size, sha256)| ManifestFile { path, size, sha256 })
        .collect();
    if files.is_empty() {
        let _ = fs::remove_dir_all(target);
        return Ok(None);
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Some(files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::io::digest_file;
    use crate::snapshots::manifest::{list, read_manifest};
    use crate::snapshots::test_support::{fixture, store};
    use crate::test_support::write;

    #[test]
    fn a_snapshot_manifest_hashes_the_tree_it_copied() {
        let (app, game, settings) = fixture("snapshot-hashes");
        let store = store(&app, &game.id);
        write(&store.join("slot.sav"), "one");
        write(&store.join("nested/deep.sav"), "deep");
        write(&store.join("empty.sav"), "");
        let source_hashes = [
            (
                "empty.sav",
                0,
                digest_file::<Sha256>(&store.join("empty.sav"), &mut |_| {}).unwrap(),
            ),
            (
                "nested/deep.sav",
                4,
                digest_file::<Sha256>(&store.join("nested/deep.sav"), &mut |_| {}).unwrap(),
            ),
            (
                "slot.sav",
                3,
                digest_file::<Sha256>(&store.join("slot.sav"), &mut |_| {}).unwrap(),
            ),
        ];

        let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        // A source that changes after the copy must not change the manifest: it describes the bytes in the snapshot.
        write(&store.join("slot.sav"), "changed");
        write(&store.join("nested/deep.sav"), "changed too");

        let scope = SnapshotScope::new(&app, &game.id);
        let manifest = read_manifest(&scope, &scope.dir(&snapshot.id)).unwrap();
        let set = manifest.sets.iter().find(|set| set.id == "saves").unwrap();
        let entries: Vec<(&str, u64, &str)> = set
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.size, file.sha256.as_str()))
            .collect();
        assert_eq!(
            entries,
            source_hashes
                .iter()
                .map(|(path, size, sha256)| (*path, *size, sha256.as_str()))
                .collect::<Vec<_>>()
        );

        let dir = set_dir_in(&scope.dir(&snapshot.id), "saves");
        assert_eq!(fs::read_to_string(dir.join("slot.sav")).unwrap(), "one");
        assert_eq!(
            fs::read_to_string(dir.join("nested/deep.sav")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn an_empty_profile_still_saves_its_list_and_an_empty_overwrite() {
        let (app, game, settings) = fixture("snapshot-empty");
        let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        assert_eq!(snapshot.files, 0);
        assert_eq!(snapshot.overwrite_files, 0);
        assert_eq!(snapshot.mods, 0);
        assert_eq!(list(&app, &game.id).len(), 1);
    }
}
