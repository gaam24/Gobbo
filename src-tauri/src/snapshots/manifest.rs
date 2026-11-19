//! The manifest and the snapshot folder helpers: the format, the read and write helpers and the retention rule.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::SnapshotScope;
use super::view::{SnapshotView, view_of};
use crate::context::Data;
use crate::context::load_profiles;
use crate::domain::naming;
use crate::domain::profiles::{ListState, PackOrigin};
use crate::domain::user_data::{Isolation, Kind};
use crate::error::{AppError, io_at};

/// The manifest format. A newer manifest is refused instead of restored with guessed fields.
pub(super) const VERSION: u32 = 1;
/// Automatic snapshots kept per profile. A manual snapshot is the user's own and is never removed by this.
const KEEP_AUTOMATIC: usize = 5;
const MANIFEST_FILE: &str = "manifest.json";
pub(super) const SETS_DIR: &str = "sets";
/// The legacy `profile_data` store, which has no user-data set id of its own.
pub(super) const LEGACY_ID: &str = "legacy";
/// The reserved id of the overwrite layer in a snapshot.
pub(super) const OVERWRITE_ID: &str = "overwrite";

/// Why a snapshot was taken. Only the automatic kinds are pruned by retention; a manual one is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    /// Started by the user from the game menu.
    Manual,
    /// Taken before the profile that owns the data changes.
    BeforeSwitch,
    /// Taken before a profile and its state folder are removed.
    BeforeDelete,
    /// Taken before another profile's data replaces this profile's data.
    BeforeCopy,
    /// Taken by a restore, so the restore itself can be undone.
    BeforeRestore,
}

impl SnapshotKind {
    const fn automatic(self) -> bool {
        !matches!(self, Self::Manual)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Target {
    /// The writable layer of an isolated redirect.
    Upper,
    /// The copy of an isolated set that is copied around a launch.
    Copy,
    /// The legacy `state/<profile>/data` folder.
    Legacy,
    /// The overwrite layer of the profile.
    Overwrite,
}

/// The editable settings of a profile: its list and its user-data choices. The deployment ledger is deliberately left
/// out, because a restore makes the list a draft rather than claiming it was applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProfileBackup {
    pub(super) list: ListState,
    #[serde(default)]
    pub(super) user_data: BTreeMap<String, Isolation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) pack: Option<PackOrigin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) collections: Vec<crate::domain::collection::CollectionOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ManifestFile {
    /// Relative to the set folder, with `/` as the separator.
    pub(super) path: String,
    pub(super) size: u64,
    pub(super) sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ManifestSet {
    pub(super) id: String,
    pub(super) kind: Kind,
    pub(super) target: Target,
    pub(super) files: Vec<ManifestFile>,
    pub(super) bytes: u32,
    /// The whiteouts of a redirect's upper layer, when it has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) whiteouts: Option<String>,
    /// The journal of whiteout changes of a redirect's upper layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) whiteouts_journal: Option<String>,
    /// Whether a non-inherited redirect was already seeded from the real folder.
    #[serde(default)]
    pub(super) seeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub(super) version: u32,
    pub(super) id: String,
    pub(super) game: String,
    pub(super) profile: String,
    pub(super) profile_name: String,
    pub(super) kind: SnapshotKind,
    pub(super) created_at: u32,
    pub(super) sets: Vec<ManifestSet>,
    /// The profile's editable settings. `None` in snapshots written before they were captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) profile_state: Option<ProfileBackup>,
    /// The profile's overwrite layer. `Some` also when it was empty, so a restore can clear it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) overwrite: Option<ManifestSet>,
}

/// Writes the manifest and prunes the profile's old automatic snapshots. A failure removes the new folder, so a
/// snapshot that cannot be listed never stays on disk.
pub(super) fn save(scope: &SnapshotScope, dir: &Path, manifest: &Manifest) -> Result<(), AppError> {
    let result = crate::infra::json::write_json(&dir.join(MANIFEST_FILE), manifest)
        .and_then(|()| prune(scope, &manifest.profile));
    if let Err(error) = result {
        let _ = fs::remove_dir_all(dir);
        return Err(error);
    }
    Ok(())
}

/// Every snapshot of the game, newest first. `app` is the app's data folder; the profile names come from the game's
/// data folder. A folder with an unreadable, newer or mismatched manifest is skipped: one damaged snapshot must not
/// hide the others.
pub fn list(app: &Data, game_id: &str) -> Vec<SnapshotView> {
    if !naming::is_plain_folder(game_id) {
        return Vec::new();
    }
    list_in(&SnapshotScope::new(app, game_id))
}

fn list_in(scope: &SnapshotScope) -> Vec<SnapshotView> {
    let Ok(entries) = fs::read_dir(scope.root()) else {
        return Vec::new();
    };
    let profiles = load_profiles(&scope.game).ok();
    let mut views = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let Some(manifest) = read_manifest(scope, &entry.path()) else {
            continue;
        };
        if manifest.version > VERSION {
            continue;
        }
        let mut view = view_of(&manifest);
        if let Some(profiles) = &profiles
            && let Ok(profile) = profiles.get(&manifest.profile)
        {
            view.profile_name.clone_from(&profile.name);
        }
        views.push(view);
    }
    views.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    views
}

pub(super) fn load(scope: &SnapshotScope, id: &str) -> Result<Manifest, AppError> {
    if !naming::is_plain_folder(id) {
        return Err(AppError::user("snapshot.not_found"));
    }
    let dir = scope.dir(id);
    let manifest =
        read_manifest(scope, &dir).ok_or_else(|| AppError::user("snapshot.not_found"))?;
    if manifest.version > VERSION {
        return Err(AppError::user("snapshot.newer")
            .with("found", manifest.version)
            .with("supported", VERSION));
    }
    Ok(manifest)
}

/// Reads the manifest of `dir`, accepting it only when its id names the folder and its game is the one in `scope`. A
/// folder copied from another game, or renamed by hand, is ignored instead of listed or restored.
pub(super) fn read_manifest(scope: &SnapshotScope, dir: &Path) -> Option<Manifest> {
    let manifest: Manifest = crate::infra::json::read_json_or_none(&dir.join(MANIFEST_FILE))?;
    let id = dir.file_name()?.to_string_lossy().into_owned();
    (manifest.id == id && manifest.game == scope.game_id).then_some(manifest)
}

pub(super) fn total_of(files: &[ManifestFile]) -> u32 {
    clamp_u32(files_bytes(files))
}

/// The bytes a set writes, from its files rather than the clamped view size.
pub(super) fn files_bytes(files: &[ManifestFile]) -> u64 {
    files
        .iter()
        .map(|file| file.size)
        .fold(0u64, u64::saturating_add)
}

/// The bytes a restore writes at most, from every set of the manifest.
pub(super) fn manifest_bytes(manifest: &Manifest) -> u64 {
    manifest
        .sets
        .iter()
        .map(|set| files_bytes(&set.files))
        .fold(0u64, u64::saturating_add)
}

/// Whether `needed_bytes` fit in `free_kb` kilobytes. The disk reports kilobytes and the manifest records bytes, so the
/// multiplication saturates: a huge free figure never wraps to a small one.
pub(super) const fn fits(free_kb: u64, needed_bytes: u64) -> bool {
    free_kb.saturating_mul(1024) >= needed_bytes
}

pub(super) const fn clamp_u32(value: u64) -> u32 {
    if value > u32::MAX as u64 {
        u32::MAX
    } else {
        value as u32
    }
}

/// Removes automatic snapshots beyond the newest `KEEP_AUTOMATIC` of one profile. Manual snapshots are never touched.
fn prune(scope: &SnapshotScope, profile: &str) -> Result<(), AppError> {
    let mut automatic: Vec<SnapshotView> = list_in(scope)
        .into_iter()
        .filter(|view| view.profile == profile && view.kind.automatic())
        .collect();
    automatic.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    for view in automatic.into_iter().skip(KEEP_AUTOMATIC) {
        // The manifest checks the id too; this second check keeps a folder name from reaching outside the game.
        if !naming::is_plain_folder(&view.id) {
            continue;
        }
        let dir = scope.dir(&view.id);
        fs::remove_dir_all(&dir).map_err(io_at(&dir))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshots::capture::create;
    use crate::snapshots::restore::{preview, restore};
    use crate::snapshots::test_support::{fixture, manifest_with, snapshot_with_file, store};
    use crate::test_support::write;

    #[test]
    fn automatic_snapshots_keep_five_and_manual_ones_stay() {
        let (app, game, settings) = fixture("snapshot-retention");
        let store = store(&app, &game.id);
        for index in 0..7 {
            write(&store.join("slot.sav"), &index.to_string());
            create(&app, &game, &settings, "p1", SnapshotKind::BeforeSwitch).unwrap();
        }
        create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
        let views = list(&app, &game.id);
        assert_eq!(
            views
                .iter()
                .filter(|view| view.kind == SnapshotKind::BeforeSwitch)
                .count(),
            KEEP_AUTOMATIC
        );
        assert_eq!(
            views
                .iter()
                .filter(|view| view.kind == SnapshotKind::Manual)
                .count(),
            1
        );
    }

    #[test]
    fn a_manifest_of_another_game_or_id_is_not_listed_or_restored() {
        let (app, game, settings, _store, snapshot) = snapshot_with_file("snapshot-manifest-check");
        let scope = SnapshotScope::new(&app, &game.id);
        let dir = scope.dir(&snapshot.id);
        let mut manifest = read_manifest(&scope, &dir).unwrap();

        manifest.id = "another-id".to_string();
        crate::infra::json::write_json(&dir.join(MANIFEST_FILE), &manifest).unwrap();
        assert!(list(&app, &game.id).is_empty());
        assert_eq!(
            preview(&app, &game, &settings, &snapshot.id)
                .unwrap_err()
                .code,
            "snapshot.not_found"
        );

        manifest.id.clone_from(&snapshot.id);
        manifest.game = "another-game".to_string();
        crate::infra::json::write_json(&dir.join(MANIFEST_FILE), &manifest).unwrap();
        assert!(list(&app, &game.id).is_empty());
        assert_eq!(
            restore(&app, &game, &settings, &snapshot.id)
                .unwrap_err()
                .code,
            "snapshot.not_found"
        );
    }

    #[test]
    fn a_five_gib_manifest_fits_a_hundred_gib_disk() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let manifest = manifest_with("1-large", 5 * GIB);
        assert!(fits(100 * 1024 * 1024, manifest_bytes(&manifest)));
        assert!(!fits(4 * 1024 * 1024, manifest_bytes(&manifest)));
    }

    #[test]
    fn a_huge_free_figure_saturates_instead_of_wrapping() {
        // 2^54 KB * 1024 would wrap to zero, which would report a full disk as full.
        assert!(fits(1 << 54, u64::MAX));
        assert!(!fits(1 << 44, u64::MAX));
        assert!(!fits(0, 1));
    }

    #[test]
    fn a_failed_manifest_write_removes_the_snapshot_folder() {
        let (app, game, _) = fixture("snapshot-manifest-path");
        let scope = SnapshotScope::new(&app, &game.id);
        let dir = scope.dir("1-failed");
        write(&dir.join("sets/saves/slot.sav"), "one");
        // A folder where the manifest file belongs makes the write fail.
        fs::create_dir_all(dir.join(MANIFEST_FILE)).unwrap();

        assert!(save(&scope, &dir, &manifest_with("1-failed", 3)).is_err());
        assert!(!dir.exists());
    }
}
