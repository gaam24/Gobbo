//! Fixtures shared by the snapshot tests.

use std::path::PathBuf;

use super::manifest::{
    Manifest, ManifestFile, ManifestSet, ProfileBackup, SnapshotKind, Target, VERSION, clamp_u32,
};
use crate::context::{Data, Settings};
use crate::domain::profiles::Profiles;
use crate::domain::user_data::{Isolation, Kind};
use crate::games::{GameDef, UserData};
use crate::test_support::scratch;
use crate::test_support::write;

/// A game with one copied save set, and a profile that keeps it separate. The profiles live in the game's data
/// folder; the app root is only where snapshots are stored.
pub(super) fn fixture(name: &str) -> (Data, GameDef, Settings) {
    let base = scratch!(name);
    let saves = base.join("saves");
    write(&saves.join("slot.sav"), "one");
    let mut game = crate::test_support::games::slot();
    game.user_data = vec![UserData {
        id: "saves".to_string(),
        kind: Kind::Saves,
        path: saves.display().to_string(),
        path_linux: None,
        mode: crate::domain::user_data::Mode::Copy,
        default: Isolation::Isolated,
        inherit: false,
    }];
    let settings = Settings {
        game_root: base.join("game").display().to_string(),
        ..Settings::default()
    };
    let app = Data::new(base.join("data"));
    let mut profiles = Profiles::with_default(Vec::new());
    profiles
        .active_mut()
        .set_user_data_isolation("saves", Isolation::Isolated);
    app.game(&game.id).save_profiles(&profiles).unwrap();
    (app, game, settings)
}

pub(super) fn store(app: &Data, game_id: &str) -> PathBuf {
    app.game(game_id)
        .profile_state("p1")
        .join("userdata/saves/copy")
}

/// A fixture with one file in the profile store and the manual snapshot already taken. Tests that only check the
/// manifest or the restore start here instead of repeating the same four setup lines.
pub(super) fn snapshot_with_file(
    name: &str,
) -> (Data, GameDef, Settings, PathBuf, super::SnapshotView) {
    let (app, game, settings) = fixture(name);
    let store = store(&app, &game.id);
    write(&store.join("slot.sav"), "one");
    let snapshot = super::create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
    (app, game, settings, store, snapshot)
}

pub(super) fn manifest_with(id: &str, bytes: u64) -> Manifest {
    Manifest {
        version: VERSION,
        id: id.to_string(),
        game: "slot".to_string(),
        profile: "p1".to_string(),
        profile_name: "Default".to_string(),
        kind: SnapshotKind::BeforeSwitch,
        created_at: 0,
        sets: vec![ManifestSet {
            id: "saves".to_string(),
            kind: Kind::Saves,
            target: Target::Copy,
            files: vec![ManifestFile {
                path: "slot.sav".to_string(),
                size: bytes,
                sha256: String::new(),
            }],
            bytes: clamp_u32(bytes),
            whiteouts: None,
            whiteouts_journal: None,
            seeded: false,
        }],
        profile_state: Some(ProfileBackup {
            collections: Vec::new(),
            list: crate::domain::profiles::ListState {
                mods: Vec::new(),
                separators: Vec::new(),
                groups: Vec::new(),
                winners: std::collections::BTreeMap::new(),
                waiting: Vec::new(),
            },
            user_data: std::collections::BTreeMap::new(),
            pack: None,
        }),
        overwrite: None,
    }
}
