//! Mod packs: rebuilding profiles in another library, mods to fetch and refusing packs of other games.

use super::*;
use crate::context::profile::history::save_profiles;
use crate::domain::library::Family;
use crate::domain::profiles::GroupKind;
use crate::domain::profiles::ModGroup;
use crate::import::ImportRequest;
use crate::import::model::ImportStage;
use crate::library::set_mod_note;
use crate::packs::check::check_pack;
use crate::packs::download::import_downloaded;
use crate::packs::export::export_pack;
use crate::packs::import::MAX_PACK_ARCHIVE_BYTES;
use crate::packs::import::MAX_PACK_TOTAL_BYTES;
use crate::packs::import::import_pack;
use crate::packs::inspect::inspect_pack;
use crate::packs::model::PackArchives;
use crate::profiles::actions::save_profile;
use crate::profiles::actions::switch_profile;
use crate::status::sync_profiles as status;
use std::io::Read;
use std::io::Write;

/// Gives a family a Nexus source, with the page data when the test has some.
fn set_nexus_source(
    family: &mut Family,
    mod_id: u32,
    meta: Option<crate::providers::nexus::NexusMeta>,
) {
    family.sources.insert(
        crate::domain::source::ProviderId::new("nexus"),
        crate::domain::source::SourceMod {
            mod_id: mod_id.to_string(),
            part_id: None,
            part_name: None,
            part_category: None,
            meta: meta.map(crate::domain::source::SourceMeta::from),
        },
    );
}

/// Switches to the profile the pack import created, named `Default (2)`.
fn switch_to_default_2(f: &Fixture) {
    let profile = status(&f.data, SLOT_GAME)
        .unwrap()
        .profiles
        .into_iter()
        .find(|profile| profile.name == "Default (2)")
        .unwrap();
    switch_profile(&f.data, SLOT_GAME, &profile.id).unwrap();
}

/// Rewrites the entries of a pack file, so a test can build a pack that does not match its manifest.
fn rewrite_pack(pack: &Path, mut change: impl FnMut(&str, &[u8]) -> Option<Vec<u8>>) {
    let mut reader = zip::ZipArchive::new(fs::File::open(pack).unwrap()).unwrap();
    let mut entries = Vec::new();

    for index in 0..reader.len() {
        let mut entry = reader.by_index(index).unwrap();
        let name = entry.name().unwrap().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        entries.push((name, bytes));
    }

    drop(reader);

    let temp = pack.with_extension("rewritten");
    let mut writer = zip::ZipWriter::new(fs::File::create(&temp).unwrap());
    let options = zip::write::SimpleFileOptions::default();

    for (name, bytes) in entries {
        writer.start_file(&name, options).unwrap();
        writer
            .write_all(&change(&name, &bytes).unwrap_or(bytes))
            .unwrap();
    }

    writer.finish().unwrap();
    fs::rename(&temp, pack).unwrap();
}

/// Changes the size the manifest declares for the first archive in the pack.
fn set_archive_size(pack: &Path, size: u64) {
    rewrite_pack(pack, |name, bytes| {
        if name != "pack.json" {
            return None;
        }

        let mut manifest: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let mods = manifest["profiles"][0]["mods"].as_array_mut().unwrap();
        let archive = mods
            .iter_mut()
            .find_map(|entry| entry.get_mut("archive"))
            .unwrap();
        archive["size"] = serde_json::json!(size);
        Some(serde_json::to_vec(&manifest).unwrap())
    });
}

/// Adds bytes to every archive inside the pack, so the manifest hash and size no longer match.
fn tamper_bundled_archives(pack: &Path) {
    rewrite_pack(pack, |name, bytes| {
        if !name.starts_with("archives/") {
            return None;
        }

        let mut changed = bytes.to_vec();
        changed.extend_from_slice(b"tampered");
        Some(changed)
    });
}

/// Imports `count` mods, each with its own script folder.
fn import_mods(f: &Fixture, count: usize) {
    for number in 0..count {
        let entry = format!("Mods/mod{number}/content/x.ws");
        let zip = f.zip(&format!("mod{number}.zip"), &[(entry.as_str(), "x")]);
        crate::test_support::import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    }
}

/// Two mods with Nexus sources and a list with a separator, ready to export.
fn source_with_nexus_sources(name: &str) -> Fixture {
    let source = fixture(name);
    source.import_two_mods();
    let data = source.data.game(SLOT_GAME);
    let game = test_games::slot();
    let mut library = load_library(&data, &game).unwrap();
    let rows = status(&source.data, SLOT_GAME).unwrap().mods;

    for (row, nexus) in rows.iter().zip([101, 102]) {
        set_nexus_source(library.families.get_mut(&row.id).unwrap(), nexus, None);
    }

    library.save(&data.index()).unwrap();
    save_profile(
        &source.data,
        SLOT_GAME,
        &[
            row("modB", true),
            separator_row("", "Scripts"),
            row("modA", false),
        ],
    )
    .unwrap();
    source
}

/// A game with one user-data set outside its folder, for the isolation round trip.
fn user_data_fixture(name: &str) -> Fixture {
    test_games::install();
    let base = scratch!(name);
    let game_root = base.join("game");
    let settings_dir = base.join("settings");
    fs::create_dir_all(game_root.join("Mods")).unwrap();
    fs::create_dir_all(&settings_dir).unwrap();
    let fixture = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir,
        base,
    };
    save_settings(&fixture.data, test_games::USER_DATA_ID, fixture.settings()).unwrap();
    fixture
}

#[test]
fn a_pack_carries_the_isolation_choices_of_its_profile() {
    use crate::domain::user_data::Isolation;
    use crate::profiles::actions::set_profile_user_data;

    let game = test_games::USER_DATA_ID;
    let source = user_data_fixture("pack-isolation-source");
    let profile = source.active_id(game);
    // The definition defaults to isolated; the author shares the set instead.
    set_profile_user_data(&source.data, game, &profile, "documents", false).unwrap();
    let pack = source.base.join("isolation.gobbopack");
    export_pack(
        &source.data,
        game,
        &pack,
        &PackOptions {
            info: PackInfo {
                name: "Isolation".into(),
                version: "1.0".into(),
                ..PackInfo::default()
            },
            profiles: vec![profile],
            archives: PackArchives::None,
        },
    )
    .unwrap();

    // The choice travels as a set id and a value, with no path from the author's machine.
    let mut reader = zip::ZipArchive::new(fs::File::open(&pack).unwrap()).unwrap();
    let mut entry = reader.by_name("pack.json").unwrap();
    let mut manifest = String::new();
    entry.read_to_string(&mut manifest).unwrap();
    assert!(
        manifest.contains("\"user_data\":{\"documents\":\"shared\"}"),
        "{manifest}"
    );
    drop(entry);
    drop(reader);

    // A receiver starts from its definition defaults and takes the author's choice.
    let target = user_data_fixture("pack-isolation-target");
    import_pack(&target.data, game, &pack).unwrap();
    let imported = status(&target.data, game)
        .unwrap()
        .profiles
        .into_iter()
        .find(|row| row.name == "Default (2)")
        .unwrap();
    let saved = load_profiles(&target.data.game(game)).unwrap();
    assert_eq!(
        saved
            .profile(&imported.id)
            .unwrap()
            .user_data_isolation("documents"),
        Isolation::Shared
    );
}

#[test]
fn a_pack_rebuilds_its_profiles_in_another_library() {
    let source = fixture("pack-source");
    source.import_two_mods();
    source.enable_all(SLOT_GAME);
    let rows = status(&source.data, SLOT_GAME).unwrap().mods;
    save_profile(
        &source.data,
        SLOT_GAME,
        &[
            separator_row("", "Scripts"),
            row(&rows[1].folder, true),
            row(&rows[0].folder, false),
        ],
    )
    .unwrap();
    set_mod_note(
        &source.data,
        SLOT_GAME,
        &rows[0].id,
        "Keep the 2K textures".into(),
    )
    .unwrap();
    let data = source.data.game(SLOT_GAME);
    let game = test_games::slot();
    let mut library = load_library(&data, &game).unwrap();
    set_nexus_source(
        library.families.get_mut(&rows[0].id).unwrap(),
        700,
        Some(crate::providers::nexus::NexusMeta {
            summary: Some("Keeps the textures sharp".into()),
            author: Some("TestAuthor".into()),
            checked_at: Some(1_730_000_000),
            picture: Some("nexus-1.jpg".into()),
            picture_url: Some("https://staticdelivery.nexusmods.com/a.jpg".into()),
            ..Default::default()
        }),
    );
    library.save(&data.index()).unwrap();
    let pack = source.base.join("pack.gobbopack");

    let exported = export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();
    assert_eq!((exported.mods, exported.archives), (2, 2));

    let target = fixture("pack-target");
    let preview = inspect_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!((preview.total, preview.have, preview.bundled), (2, 0, 2));
    assert!(preview.missing.is_empty() && preview.updates.is_empty());

    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(imported.imported, 2);
    assert!(imported.failed.is_empty() && imported.missing.is_empty());
    assert_eq!(imported.profiles_created, 1);
    let made = status(&target.data, SLOT_GAME).unwrap();
    let profile = made
        .profiles
        .iter()
        .find(|profile| profile.name == "Default (2)")
        .unwrap();
    switch_profile(&target.data, SLOT_GAME, &profile.id).unwrap();
    let rebuilt = status(&target.data, SLOT_GAME).unwrap();
    let order: Vec<(&str, bool)> = rebuilt
        .mods
        .iter()
        .map(|m| (m.folder.as_str(), m.enabled))
        .collect();
    assert_eq!(order, [("modB", true), ("modA", false)]);
    assert_eq!(rebuilt.separators.len(), 1);
    let extra = crate::status::details::mod_extras(&target.data, SLOT_GAME, &["modA".to_string()])
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(extra.notes, "Keep the 2K textures");
    let page = extra.source.as_ref().unwrap();
    assert_eq!(page.summary.as_deref(), Some("Keeps the textures sharp"));
    // The picture file and the check time stay with the author; the receiver's own check brings them.
    assert_eq!((page.picture.as_deref(), page.checked_at), (None, None));
    assert_eq!(
        rebuilt.separators[0].before.as_deref(),
        Some(rebuilt.mods[0].id.as_str())
    );

    // The same pack again finds everything in the library and updates the profile instead of making another.
    let again = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(
        (
            again.imported,
            again.profiles_created,
            again.profiles_updated
        ),
        (0, 0, 1)
    );
}

#[test]
fn a_pack_without_archives_lists_the_mods_to_get() {
    let source = fixture("pack-recipe");
    source.import_two_mods();
    let pack = source.base.join("recipe.gobbopack");
    // Local mods without a Nexus id cannot be rebuilt from a recipe alone, and the check says so.
    let issues = check_pack(
        &source.data,
        SLOT_GAME,
        &pack_options(&source, "1.0", PackArchives::None),
    )
    .unwrap();
    assert_eq!(issues.iter().filter(|issue| issue.error).count(), 2);
    let issues = check_pack(
        &source.data,
        SLOT_GAME,
        &pack_options(&source, "1.0", PackArchives::Unlisted),
    )
    .unwrap();
    assert!(issues.iter().all(|issue| !issue.error));

    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::None),
    )
    .unwrap();
    let target = fixture("pack-recipe-target");
    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(imported.missing.len(), 2);
    assert_eq!(imported.imported, 0);
}

#[test]
fn a_pack_from_another_game_is_refused() {
    let source = fixture("pack-other-game");
    source.import_two_mods();
    let pack = source.base.join("other.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();
    let target = mirror_fixture("pack-other-game-target");
    assert_eq!(
        inspect_pack(&target.data, MIRROR_GAME, &pack)
            .unwrap_err()
            .code,
        "pack.other_game"
    );
}

#[test]
fn mods_the_receiver_lacks_keep_their_place_when_they_arrive() {
    let source = source_with_nexus_sources("pack-late-source");
    let pack = source.base.join("late.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::None),
    )
    .unwrap();

    // The receiver has no mods yet: they come from Nexus, so the pack brings only the recipe.
    let target = fixture("pack-late-target");
    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(imported.missing.len(), 2);
    // Then the mods arrive, by any means, and the profile is not imported again.
    target.import_two_mods();
    switch_to_default_2(&target);
    let rebuilt = status(&target.data, SLOT_GAME).unwrap();
    let order: Vec<(&str, bool)> = rebuilt
        .mods
        .iter()
        .map(|m| (m.folder.as_str(), m.enabled))
        .collect();
    assert_eq!(order, [("modB", true), ("modA", false)]);
    assert_eq!(rebuilt.separators.len(), 1);
    assert_eq!(
        rebuilt.separators[0].before.as_deref(),
        Some(rebuilt.mods[1].id.as_str())
    );
}

#[test]
fn a_profile_still_waiting_for_mods_keeps_them_when_it_is_packed_again() {
    let source = source_with_nexus_sources("pack-waiting-source");
    let first = source.base.join("first.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &first,
        &pack_options(&source, "1.0", PackArchives::None),
    )
    .unwrap();

    // The receiver imports the recipe, and packs its own profile again before any mod has come in.
    let middle = fixture("pack-waiting-middle");
    import_pack(&middle.data, SLOT_GAME, &first).unwrap();
    let profile = status(&middle.data, SLOT_GAME)
        .unwrap()
        .profiles
        .into_iter()
        .find(|profile| profile.name == "Default (2)")
        .unwrap();
    let mut options = pack_options(&middle, "1.1", PackArchives::None);
    options.profiles = vec![profile.id];
    let second = middle.base.join("second.gobbopack");
    export_pack(&middle.data, SLOT_GAME, &second, &options).unwrap();

    let target = fixture("pack-waiting-target");
    let imported = import_pack(&target.data, SLOT_GAME, &second).unwrap();
    assert_eq!(imported.missing.len(), 2);
    target.import_two_mods();
    switch_to_default_2(&target);
    let rebuilt = status(&target.data, SLOT_GAME).unwrap();
    let order: Vec<(&str, bool)> = rebuilt
        .mods
        .iter()
        .map(|m| (m.folder.as_str(), m.enabled))
        .collect();
    assert_eq!(order, [("modB", true), ("modA", false)]);
    assert_eq!(
        rebuilt.separators[0].before.as_deref(),
        Some(rebuilt.mods[1].id.as_str())
    );
}

#[test]
fn a_pack_archive_that_does_not_match_its_manifest_is_refused() {
    let source = fixture("pack-mismatch");
    source.import_two_mods();
    let pack = source.base.join("mismatch.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();
    tamper_bundled_archives(&pack);

    let target = fixture("pack-mismatch-target");
    let error = import_pack(&target.data, SLOT_GAME, &pack).unwrap_err();
    assert_eq!(error.code, "pack.archive_mismatch");
}

#[test]
fn an_archive_the_manifest_declares_too_large_is_refused() {
    let source = fixture("pack-archive-too-large");
    source.import_two_mods();
    let pack = source.base.join("archive-too-large.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();
    set_archive_size(&pack, MAX_PACK_ARCHIVE_BYTES + 1);

    let target = fixture("pack-archive-too-large-target");
    let error = import_pack(&target.data, SLOT_GAME, &pack).unwrap_err();
    assert_eq!(error.code, "pack.archive_too_large");
}

#[test]
fn a_pack_whose_manifest_declares_too_much_is_refused() {
    let source = fixture("pack-total-too-large");
    source.import_two_mods();
    let pack = source.base.join("total-too-large.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();
    set_archive_size(&pack, MAX_PACK_TOTAL_BYTES + 1);

    let target = fixture("pack-total-too-large-target");
    let error = import_pack(&target.data, SLOT_GAME, &pack).unwrap_err();
    assert_eq!(error.code, "pack.too_large");
}

#[test]
fn a_pack_import_can_be_canceled_between_batches() {
    let source = fixture("pack-batch-cancel");
    import_mods(&source, crate::import::BATCH_SIZE + 1);
    let pack = source.base.join("cancel.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();

    let target = fixture("pack-batch-cancel-target");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let error = crate::packs::import::import_pack_with_progress(
        &target.data,
        SLOT_GAME,
        &pack,
        &mut |stage, percent| {
            if stage == ImportStage::Extracting && percent == 100 {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        },
        Some(&cancel),
    )
    .unwrap_err();
    assert_eq!(error.code, "pack.cancelled");
    assert_eq!(
        status(&target.data, SLOT_GAME).unwrap().mods.len(),
        crate::import::BATCH_SIZE,
        "the group already committed stays in the library"
    );
}

#[test]
fn a_pack_download_keeps_only_the_failed_files() {
    let f = fixture("pack-download-batch");
    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let good =
        crate::test_support::make_zip(&downloads, "Good.zip", &[("Mods/good/content/x.ws", "x")]);
    let bad = downloads.join("Bad.zip");
    fs::write(&bad, "not an archive").unwrap();

    let failed = import_downloaded(
        &f.data,
        SLOT_GAME,
        vec![
            ("Good".to_string(), ImportRequest::new(good.clone())),
            ("Bad".to_string(), ImportRequest::new(bad.clone())),
        ],
        None,
    )
    .unwrap();

    assert_eq!(failed, vec!["Bad".to_string()]);
    assert!(!good.exists(), "a successful import forgets its download");
    assert!(bad.is_file(), "a failed import keeps its download");
}

/// Gives the active profile one Bundle group and one Translation group, both naming the current mods by family id.
fn add_groups_to_active(source: &Fixture) -> Vec<String> {
    let data = source.data.game(SLOT_GAME);
    let profile_id = source.active_id(SLOT_GAME);
    let mut profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile_mut(&profile_id).unwrap();
    let members = status(&source.data, SLOT_GAME)
        .unwrap()
        .mods
        .into_iter()
        .map(|row| row.id)
        .collect::<Vec<_>>();
    profile.groups = vec![
        ModGroup {
            id: "author-bundle".into(),
            name: "Core Set".into(),
            members: members.clone(),
            kind: GroupKind::Bundle,
            language: None,
        },
        ModGroup {
            id: "author-translation".into(),
            name: "Localization".into(),
            members: members.clone(),
            kind: GroupKind::Translation,
            language: Some("de".into()),
        },
    ];
    save_profiles(&data, &profiles).unwrap();
    members
}

/// Loads the profile an import made, named `Default (2)`.
fn imported_profile(target: &Fixture) -> crate::domain::profiles::Profile {
    let id = status(&target.data, SLOT_GAME)
        .unwrap()
        .profiles
        .into_iter()
        .find(|profile| profile.name == "Default (2)")
        .unwrap()
        .id;
    load_profiles(&target.data.game(SLOT_GAME))
        .unwrap()
        .profile(&id)
        .unwrap()
        .clone()
}

#[test]
fn a_pack_carries_bundle_and_translation_groups() {
    let source = fixture("pack-groups-source");
    source.import_two_mods();
    source.enable_all(SLOT_GAME);
    let members = add_groups_to_active(&source);
    assert_eq!(members.len(), 2);
    let pack = source.base.join("groups.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();

    let target = fixture("pack-groups-target");
    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(imported.profiles_created, 1);
    let rebuilt = imported_profile(&target);
    assert_eq!(rebuilt.groups.len(), 2);

    let bundle = rebuilt
        .groups
        .iter()
        .find(|group| group.name == "Core Set")
        .unwrap();
    assert_eq!(bundle.kind, GroupKind::Bundle);
    assert_eq!(bundle.language, None);
    let translation = rebuilt
        .groups
        .iter()
        .find(|group| group.name == "Localization")
        .unwrap();
    assert_eq!(translation.kind, GroupKind::Translation);
    assert_eq!(translation.language.as_deref(), Some("de"));

    // The author's local handles never travel; the receiver makes fresh ids.
    assert!(
        rebuilt
            .groups
            .iter()
            .all(|group| group.id.starts_with("grp") && group.id != "author-bundle")
    );
    // Member indices resolve to this library's families, in the list order the author wrote.
    let expected: Vec<&str> = rebuilt.mods.iter().map(|entry| entry.id.as_str()).collect();
    let resolved: Vec<&str> = bundle.members.iter().map(String::as_str).collect();
    assert_eq!(resolved, expected);
    assert_eq!(translation.members, bundle.members);
}

#[test]
fn a_pack_without_groups_imports_with_an_empty_group_list() {
    let source = fixture("pack-no-groups-source");
    source.import_two_mods();
    let pack = source.base.join("no-groups.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();

    // The optional field is absent from an older-style pack.
    let mut reader = zip::ZipArchive::new(fs::File::open(&pack).unwrap()).unwrap();
    let mut entry = reader.by_name("pack.json").unwrap();
    let mut manifest = String::new();
    entry.read_to_string(&mut manifest).unwrap();
    assert!(!manifest.contains("\"groups\""), "{manifest}");
    drop(entry);
    drop(reader);

    let target = fixture("pack-no-groups-target");
    import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert!(imported_profile(&target).groups.is_empty());
}

#[test]
fn a_group_with_an_out_of_range_member_does_not_break_an_import() {
    let source = fixture("pack-bad-group-source");
    source.import_two_mods();
    let pack = source.base.join("bad-group.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();

    // A hand-edited manifest names a mod past the end of the list.
    rewrite_pack(&pack, |name, bytes| {
        if name != "pack.json" {
            return None;
        }

        let mut manifest: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let kind = serde_json::to_value(GroupKind::Bundle).unwrap();
        manifest["profiles"][0]["groups"] =
            serde_json::json!([{ "name": "Broken Group", "kind": kind, "members": [0, 99] }]);
        Some(serde_json::to_vec(&manifest).unwrap())
    });

    let target = fixture("pack-bad-group-target");
    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(imported.profiles_created, 1);
    // The one member that resolves is not enough, so the group is dropped instead of crashing the import.
    assert!(imported_profile(&target).groups.is_empty());
}

#[test]
fn a_winner_and_a_group_of_a_mod_still_waiting_name_it_until_it_joins() {
    let source = source_with_nexus_sources("pack-waiting-names-source");
    let members = add_groups_to_active(&source);
    let family_a = status(&source.data, SLOT_GAME)
        .unwrap()
        .mods
        .into_iter()
        .find(|row| row.folder == "modA")
        .unwrap()
        .id;
    let data = source.data.game(SLOT_GAME);
    let mut profiles = load_profiles(&data).unwrap();
    profiles
        .profile_mut(&source.active_id(SLOT_GAME))
        .unwrap()
        .winners =
        std::collections::BTreeMap::from([("textures/a.dds".to_string(), family_a.clone())]);
    save_profiles(&data, &profiles).unwrap();
    let pack = source.base.join("waiting-names.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::None),
    )
    .unwrap();

    // The receiver has neither mod yet: the winner and the group name the pack keys of both.
    let target = fixture("pack-waiting-names-target");
    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(imported.missing.len(), 2);
    let waiting = imported_profile(&target);
    assert_eq!(waiting.winners.get("textures/a.dds"), Some(&family_a));
    let core = waiting
        .groups
        .iter()
        .find(|group| group.name == "Core Set")
        .unwrap();
    assert_eq!(core.members, members);

    // Once the mods arrive, the names follow them to the families of this library.
    target.import_two_mods();
    switch_to_default_2(&target);
    let local_a = status(&target.data, SLOT_GAME)
        .unwrap()
        .mods
        .into_iter()
        .find(|row| row.folder == "modA")
        .unwrap()
        .id;
    let joined = imported_profile(&target);
    assert_eq!(joined.winners.get("textures/a.dds"), Some(&local_a));
    let core = joined
        .groups
        .iter()
        .find(|group| group.name == "Core Set")
        .unwrap();
    assert!(core.members.contains(&local_a), "{:?}", core.members);
    assert_eq!(core.members.len(), 2);
}

#[test]
fn a_pack_does_not_import_an_archive_the_library_already_has() {
    let source = fixture("pack-known-archive-source");
    source.import_two_mods();
    let pack = source.base.join("known.gobbopack");
    export_pack(
        &source.data,
        SLOT_GAME,
        &pack,
        &pack_options(&source, "1.0", PackArchives::All),
    )
    .unwrap();

    let target = fixture("pack-known-archive-target");
    // The receiver already has one of the pack's archives, byte for byte, from another import.
    let already = source.base.join("modA.zip");
    crate::test_support::import(&target.data, SLOT_GAME, &already, None, &mut |_, _| {}).unwrap();

    let imported = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(
        imported.imported, 1,
        "only the archive the library lacks is brought in"
    );
    assert!(imported.failed.is_empty(), "{:?}", imported.failed);
    assert_eq!(status(&target.data, SLOT_GAME).unwrap().mods.len(), 2);

    // Importing the same pack again brings nothing and does not duplicate the mods.
    let again = import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    assert_eq!(again.imported, 0);
    assert_eq!(status(&target.data, SLOT_GAME).unwrap().mods.len(), 2);
}
