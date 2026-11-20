//! Applying and undoing a deployment: the overlay layer it prepares, drafts, and the profile applied.

use super::*;
use crate::deploy::DeployStage;
use crate::deploy::ops::apply_deployment;
use crate::deploy::ops::apply_deployment_with_progress;
use crate::deploy::ops::undo_deployment;
use crate::domain::profiles::Entry as ProfileEntry;
use crate::domain::profiles::diff::changed_count;
use crate::profiles::actions::create_profile;
use crate::profiles::actions::delete_profile;
use crate::profiles::actions::save_profile;
use crate::profiles::actions::set_mod_version;
use crate::profiles::actions::switch_profile;
use crate::status::details::mod_versions;
use crate::status::preflight::CheckState;
use crate::status::preflight::preflight;
use crate::status::sync_profiles as status;
use crate::test_support::import;

/// The overlay layer of the profile that is active now.
fn layer(f: &Fixture, game_id: &str) -> PathBuf {
    let state = f.data.game(game_id).profile_state(&f.active_id(game_id));
    state.join("mods")
}

#[test]
fn overlay_apply_then_undo_leaves_the_game_folder_untouched() {
    let f = fixture("apply");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);

    // The two copies of the shared script are one file in the game, so only the winner reaches the layer.
    let applied = apply_deployment(&f.data, SLOT_GAME).unwrap();
    assert_eq!(applied.files, 1);
    assert!(status(&f.data, SLOT_GAME).unwrap().deployed);
    assert!(
        layer(&f, SLOT_GAME)
            .join("Mods/modA/content/x.ws")
            .is_file()
    );
    assert!(!f.game_root.join("Mods/modA").exists());

    // Without draft changes applying again changes nothing. While a deployment runs, the profile
    // stays editable; the changes wait as a draft.
    assert!(apply_deployment(&f.data, SLOT_GAME).is_err());
    let rows = [row("modA", false), row("modB", true)];
    save_profile(&f.data, SLOT_GAME, &rows).unwrap();
    assert!(status(&f.data, SLOT_GAME).unwrap().draft_changes > 0);

    let warnings = undo_deployment(&f.data, SLOT_GAME).unwrap();
    assert!(warnings.is_empty());
    assert!(!status(&f.data, SLOT_GAME).unwrap().deployed);
    assert!(!f.game_root.join("Mods/modA").exists());
}

#[test]
fn apply_reports_progress_from_preparing_to_done() {
    let f = fixture("apply-progress");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let mut steps = Vec::new();

    apply_deployment_with_progress(&f.data, SLOT_GAME, &mut |step| {
        steps.push(step);
    })
    .unwrap();

    assert_eq!(
        steps.first().map(|step| step.stage),
        Some(DeployStage::Preparing)
    );
    assert!(steps.iter().any(|step| step.stage == DeployStage::Writing));
    assert_eq!(steps.last().map(|step| step.stage), Some(DeployStage::Done));
    assert_eq!(steps.last().map(|step| step.percent), Some(100));
    assert!(
        steps
            .windows(2)
            .all(|pair| pair[0].percent <= pair[1].percent)
    );
}

#[test]
fn preflight_names_the_folders_the_overlay_and_the_free_space() {
    let f = fixture("preflight");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);

    let report = preflight(&f.data, SLOT_GAME).unwrap();

    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "game_folder" && check.state == CheckState::Ok)
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "overlay" && check.state == CheckState::Ok)
    );
    assert!(report.checks.iter().any(|check| check.id == "space"));
    assert!(report.ready);
}

#[test]
fn draft_changes_count_what_differs_from_the_deployed_list() {
    let entry = |id: &str, folder: &str| ProfileEntry {
        id: id.to_string(),
        folder: folder.to_string(),
        enabled: true,
        priority: None,
    };
    let deployed = vec![entry("a", "a1"), entry("b", "b1")];
    assert_eq!(
        changed_count(&deployed, &[entry("a", "a1"), entry("b", "b1")]),
        0
    );
    assert_eq!(
        changed_count(&deployed, &[entry("b", "b1"), entry("a", "a1")]),
        1,
        "a swap moves one entry"
    );
    assert_eq!(changed_count(&deployed, &[entry("a", "a1")]), 1);
    assert_eq!(
        changed_count(&deployed, &[entry("a", "a2")]),
        2,
        "a version change counts as a change"
    );
    assert_eq!(changed_count(&[], &[entry("a", "a1"), entry("c", "c1")]), 2);
    assert_eq!(changed_count(&[], &[]), 0);
}

#[test]
fn apply_uses_only_the_active_profile() {
    let f = fixture("apply-profile");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let default = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Tylko B", None, false).unwrap();
    let only_b = f.profile_id(SLOT_GAME, "Tylko B");
    switch_profile(&f.data, SLOT_GAME, &only_b).unwrap();
    let rows = [row("modB", true)];
    save_profile(&f.data, SLOT_GAME, &rows).unwrap();

    let applied = apply_deployment(&f.data, SLOT_GAME).unwrap();
    assert_eq!(applied.files, 1);
    let active_layer = layer(&f, SLOT_GAME);
    assert_eq!(
        fs::read_to_string(active_layer.join("Mods/modB/content/x.ws")).unwrap(),
        "from B"
    );
    assert!(!active_layer.join("Mods/modA").exists());
    undo_deployment(&f.data, SLOT_GAME).unwrap();

    switch_profile(&f.data, SLOT_GAME, &default).unwrap();
    assert_eq!(apply_deployment(&f.data, SLOT_GAME).unwrap().files, 1);
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
fn applying_a_draft_replaces_the_active_deployment() {
    let f = fixture("apply-replace");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    assert!(
        layer(&f, SLOT_GAME)
            .join("Mods/modA/content/x.ws")
            .is_file()
    );

    // The draft drops modB: one apply undoes the old deployment and puts the new list in.
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", false)]).unwrap();
    let applied = apply_deployment(&f.data, SLOT_GAME).unwrap();

    assert_eq!(applied.files, 1);
    let status = status(&f.data, SLOT_GAME).unwrap();
    assert!(status.deployed);
    assert_eq!(status.draft_changes, 0);
    assert!(
        layer(&f, SLOT_GAME)
            .join("Mods/modA/content/x.ws")
            .is_file()
    );
    assert!(!layer(&f, SLOT_GAME).join("Mods/modB").exists());
}

#[test]
fn applying_the_viewed_profile_undoes_the_deployment_of_another_one() {
    let f = fixture("profiles-apply-other");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let default = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Drugi", Some(&default), false).unwrap();
    let other = f.profile_id(SLOT_GAME, "Drugi");
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    switch_profile(&f.data, SLOT_GAME, &other).unwrap();
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(view.deployed, "the viewed profile is deployed now");
    assert!(view.profiles.iter().any(|p| p.id == other && p.deployed));
    assert!(view.profiles.iter().any(|p| p.id == default && !p.deployed));
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
fn switching_keeps_the_deployment_of_another_profile_and_the_deployed_one_cannot_go() {
    let f = fixture("profiles-deployed");
    f.import_two_mods();
    let default = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Drugi", None, false).unwrap();
    let other = f.profile_id(SLOT_GAME, "Drugi");
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    // Switching changes nothing on disk: the default profile stays deployed, and the viewed one is not.
    switch_profile(&f.data, SLOT_GAME, &other).unwrap();
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(!view.deployed);
    assert_eq!(view.draft_changes, 0);
    assert!(view.profiles.iter().any(|p| p.id == default && p.deployed));
    assert!(view.profiles.iter().any(|p| p.id == other && !p.deployed));

    // Deleting the deployed profile cleans its deployment first; the game is closed.
    delete_profile(&f.data, SLOT_GAME, &default).unwrap();
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(view.profiles.iter().all(|p| p.id != default));
    assert!(!view.deployed);
    delete_profile(&f.data, SLOT_GAME, &other).unwrap_err();
}

#[test]
fn choosing_a_version_is_a_draft_and_changes_the_deployed_file() {
    let f = mirror_fixture("choose-version");
    let (old, new) = two_mod_versions(&f);
    import(&f.data, MIRROR_GAME, &old, None, &mut |_, _| {}).unwrap();
    // The first status creates the profile with the first version; the later import does not switch it.
    status(&f.data, MIRROR_GAME).unwrap();
    import(&f.data, MIRROR_GAME, &new, None, &mut |_, _| {}).unwrap();
    f.enable_all(MIRROR_GAME);
    apply_deployment(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(
        fs::read_to_string(layer(&f, MIRROR_GAME).join("r6/scripts/TestMod/a.reds")).unwrap(),
        "old"
    );

    let view = status(&f.data, MIRROR_GAME).unwrap();
    let family = view.mods[0].id.clone();
    let versions = mod_versions(&f.data, MIRROR_GAME, &view.mods[0].folder).unwrap();
    let newer = versions
        .iter()
        .find(|version| version.version.as_deref() == Some("1.2.1"))
        .unwrap()
        .folder
        .clone();
    set_mod_version(&f.data, MIRROR_GAME, &family, &newer).unwrap();
    assert_eq!(status(&f.data, MIRROR_GAME).unwrap().draft_changes, 1);

    apply_deployment(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(
        fs::read_to_string(layer(&f, MIRROR_GAME).join("r6/scripts/TestMod/a.reds")).unwrap(),
        "new"
    );
    undo_deployment(&f.data, MIRROR_GAME).unwrap();
}

#[test]
fn an_edit_made_during_apply_survives_and_becomes_a_draft() {
    let f = fixture("apply-edit");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let edited = std::cell::Cell::new(false);

    apply_deployment_with_progress(&f.data, SLOT_GAME, &mut |step| {
        if step.stage == DeployStage::Writing && !edited.replace(true) {
            let data = f.data.game(SLOT_GAME);
            let mut profiles = crate::context::load_profiles(&data).unwrap();
            profiles.active_mut().name = "Edited during apply".to_string();

            if let Some(entry) = profiles.active_mut().mods.first_mut() {
                entry.enabled = false;
            }

            data.save_profiles(&profiles).unwrap();
        }
    })
    .unwrap();

    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(view.deployed);
    assert!(view.draft_changes > 0, "the edit is a draft, not lost");
    let active = view.profiles.iter().find(|profile| profile.active).unwrap();
    assert_eq!(active.name, "Edited during apply");
    assert_eq!(active.enabled_count, 1, "the edit changed the current list");
    assert!(
        layer(&f, SLOT_GAME)
            .join("Mods/modA/content/x.ws")
            .is_file()
    );
}

#[test]
fn mirror_game_takes_archive_paths_and_stages_them_without_touching_the_game() {
    let f = mirror_fixture("mirror");
    let zip = f.zip(
        "Foo-1.zip",
        &[
            ("archive/pc/mod/foo.archive", "a"),
            ("r6/scripts/Foo/a.reds", "b"),
            ("bin/x64/game.exe", "game"),
        ],
    );
    import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    f.enable_all(MIRROR_GAME);
    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert!(!status.needs_settings_dir);
    assert_eq!(status.mods[0].folder, "Foo-1");

    let applied = apply_deployment(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(applied.files, 2);
    assert!(!f.game_root.join("archive").exists());
    let staged = layer(&f, MIRROR_GAME).join("archive/pc/mod/foo.archive");
    assert!(staged.is_file());
    undo_deployment(&f.data, MIRROR_GAME).unwrap();
    assert!(!f.game_root.join("archive").exists());
}

#[test]
fn an_external_root_deploys_to_the_overlay_layer() {
    let base = scratch!("external-root");
    let game_root = base.join("game");
    fs::create_dir_all(&game_root).unwrap();
    fs::write(game_root.join("game.exe"), b"").unwrap();
    let mods_root = base.join("userdata/External Game");
    fs::create_dir_all(mods_root.join("Mods")).unwrap();
    let data = Data::new(base.join("data"));
    save_folders(
        &data,
        EXTERNAL_GAME,
        &game_root,
        &mods_root.display().to_string(),
    );

    let zip = base.join("GoodModlet.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&zip).unwrap());
    let options = zip::write::SimpleFileOptions::default();

    for (name, text) in [
        ("GoodModlet/ModInfo.xml", "<xml/>"),
        ("GoodModlet/Config/items.xml", "<items/>"),
        ("readme.txt", "hello"),
    ] {
        writer.start_file(name, options).unwrap();
        Write::write_all(&mut writer, text.as_bytes()).unwrap();
    }

    writer.finish().unwrap();
    import(&data, EXTERNAL_GAME, &zip, None, &mut |_, _| {}).unwrap();

    let status = status(&data, EXTERNAL_GAME).unwrap();
    assert!(status.needs_mods_root);
    assert!(status.mods_ready);
    assert_eq!(status.mods_root, mods_root.display().to_string());
    assert_eq!(status.mods[0].folder, "GoodModlet");

    save_profile(&data, EXTERNAL_GAME, &[row("GoodModlet", true)]).unwrap();
    apply_deployment(&data, EXTERNAL_GAME).unwrap();
    let staged = data
        .game(EXTERNAL_GAME)
        .deployed_state()
        .unwrap()
        .join("mods/Mods/GoodModlet/ModInfo.xml");
    assert!(staged.is_file());
    assert!(!mods_root.join("Mods/GoodModlet").exists());

    undo_deployment(&data, EXTERNAL_GAME).unwrap();
    assert!(!mods_root.join("Mods/GoodModlet").exists());
    let _ = fs::remove_dir_all(&base);
}
