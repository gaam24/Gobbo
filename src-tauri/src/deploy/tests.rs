//! Deployment tests: the layer, undo, and the plan written to disk.

mod apply;
mod layout;
#[cfg(target_os = "linux")]
mod overlay;

use super::*;
use crate::domain::profiles::entry::ProfileMod;
use crate::test_support::games as test_games;
use crate::test_support::{scratch, write};

/// Whether both paths are the same file on disk (a hard link).
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let (a, b) = (fs::metadata(a).unwrap(), fs::metadata(b).unwrap());
    a.ino() == b.ino()
}

struct Fixture {
    deploy: Deploy,
    library: PathBuf,
    root: PathBuf,
    target: Target,
}

impl Fixture {
    fn plan(&self, mods: &[ProfileMod]) -> Result<DeployPlan, DeployError> {
        plan_from_disk(&self.deploy, &self.library, mods)
    }

    /// Plans the mods and applies the plan to the overlay layer.
    fn apply(&self, mods: &[ProfileMod]) -> Result<ApplyReport, DeployError> {
        apply(&self.plan(mods)?, &self.library, &self.target)
    }
}

/// The game folder's files, and the settings file's path and text, as they are before a deployment.
fn snapshot(f: &Fixture) -> (Vec<String>, PathBuf, Option<String>) {
    let files = scan::list_files(&f.root).unwrap();
    let settings_path = f.target.settings.clone().unwrap();
    let settings = settings::read(&settings_path).unwrap();
    (files, settings_path, settings)
}

/// A fixture under `base`: the game folder `base/game` with its target, and the library `base/library`.
fn fixture_in(base: &Path, deploy: Deploy, settings: Option<PathBuf>) -> Fixture {
    let mut target = crate::test_support::target(base);
    target.settings = settings;
    let root = target.root.clone();
    Fixture {
        deploy,
        library: base.join("library"),
        target,
        root,
    }
}

/// A slot game with two folders that Gobbo does not create: a foreign merger output and a disabled `~mod`.
fn fixture(name: &str) -> Fixture {
    let base = scratch!(name);
    let mut deploy = test_games::slot().deploy;
    deploy.shared.clear();
    let f = fixture_in(&base, deploy, Some(base.join("settings/mods.settings")));
    write(
        &f.root.join("Mods/mod0000_MergedFiles/content/merged.ws"),
        "merged",
    );
    write(
        &f.root.join("Mods/~modMapQuestInsight/content/map.ws"),
        "map",
    );
    write(
        f.target.settings.as_deref().unwrap(),
        "[mod0000_MergedFiles]\r\nEnabled=1\r\nPriority=1\r\n\r\n[~modMapQuestInsight]\r\nEnabled=0\r\nPriority=2\r\n",
    );
    write(&f.library.join("modA/content/scripts/a.ws"), "a");
    write(&f.library.join("modA/content/x.ws"), "from A");
    write(&f.library.join("modB/content/x.ws"), "from B");
    f
}

/// A mirror game: in the library, files at game paths; in the game, only the exe.
fn mirror_fixture(name: &str) -> Fixture {
    let f = fixture_in(&scratch!(name), test_games::mirror().deploy, None);
    write(&f.root.join("bin/x64/game.exe"), "game");
    write(&f.library.join("modA/archive/pc/mod/a.archive"), "from A");
    write(&f.library.join("modA/r6/scripts/A/a.reds"), "a");
    f
}

/// A game that keeps its mods outside its folder, in a flat slot `source`.
fn external_fixture(name: &str) -> (Fixture, PathBuf) {
    let base = scratch!(name);
    let external = base.join("userdata");
    let library = base.join("library");
    write(&library.join("modA/custom.pak"), "pak");
    let mut deploy = test_games::slot().deploy;
    deploy.slots[0].dir = "source".into();
    deploy.slots[0].flat = true;
    deploy.slots[0].marker = None;
    let fixture = Fixture {
        deploy,
        library,
        root: base.join("game"),
        target: Target {
            root: external.clone(),
            game_root: base.join("game"),
            settings: None,
            state: base.join("state"),
            user_data: Vec::new(),
        },
    };
    (fixture, external)
}

fn profile(folders: &[&str], slot: &str) -> Vec<ProfileMod> {
    crate::test_support::profile_mods(folders, slot)
}

fn mods(folders: &[&str]) -> Vec<ProfileMod> {
    profile(folders, "Mods")
}

#[test]
fn only_a_layer_prepared_journal_is_an_overlay_state() {
    let base = scratch!("state-is-overlay");
    let state = base.join("state");
    fs::create_dir_all(&state).unwrap();
    assert!(!state_is_overlay(&state));

    let journal = Journal::new(state.join(JOURNAL_FILE));
    journal
        .append(&Entry::FileDeployed {
            path: base.join("game/x"),
        })
        .unwrap();
    assert!(!state_is_overlay(&state));

    journal
        .append(&Entry::LayerPrepared {
            mods: state.join("mods"),
            overwrite: state.join("overwrite"),
        })
        .unwrap();
    assert!(state_is_overlay(&state));
}
