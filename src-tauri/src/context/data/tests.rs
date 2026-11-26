use super::*;
use crate::context::library::Library;
use crate::domain::profiles::Profiles;
use std::fs;

#[test]
fn every_game_handle_shares_one_index_cache() {
    let root = Data::with_cache(PathBuf::from("data"));
    let game = root.game("slot");
    assert!(Arc::ptr_eq(
        root.caches.as_ref().expect("the root cache"),
        game.caches.as_ref().expect("the game cache")
    ));
}

#[test]
fn an_owned_definition_is_a_copy_of_the_shared_value() {
    use crate::test_support::games as test_games;

    test_games::install();
    let data = Data::with_cache(PathBuf::from("data"));
    let mut owned = data.definition(test_games::SLOT_ID).unwrap();
    owned.name = "Changed Game".into();
    let shared = data.definition_shared(test_games::SLOT_ID).unwrap();
    assert_eq!(owned.name, "Changed Game");
    assert_ne!(shared.name, "Changed Game");
    assert_eq!(shared.id, test_games::SLOT_ID);
}

#[test]
fn a_saved_index_is_read_again_but_an_unchanged_one_is_reused() {
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;

    let base = scratch!("data-cache-load");
    let root = Data::with_cache(base);
    let data = root.game(test_games::SLOT_ID);
    let game = test_games::slot();
    let folder = data.library().join("modA");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("content.esp"), "x").unwrap();
    let mut library = Library::default();
    library.versions.insert(
        "modA".into(),
        library::scan_mod(&data.library(), "modA", &game).unwrap(),
    );
    library.save(&data.index()).unwrap();

    let first = load_library_shared(&data, &game).unwrap();
    let second = load_library_shared(&data, &game).unwrap();
    assert!(Arc::ptr_eq(&first, &second));

    library.versions.clear();
    library.save(&data.index()).unwrap();
    let third = load_library_shared(&data, &game).unwrap();
    assert!(!Arc::ptr_eq(&first, &third));
    assert!(third.versions.is_empty());
}

#[test]
fn a_shared_profiles_read_reuses_the_value_and_sees_a_save() {
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;

    let base = scratch!("data-profiles-shared");
    let root = Data::with_cache(base);
    let data = root.game(test_games::SLOT_ID);
    Profiles::with_default(Vec::new())
        .save(&data.profiles_file())
        .unwrap();

    let first = load_profiles_shared(&data).unwrap();
    let second = load_profiles_shared(&data).unwrap();
    assert!(Arc::ptr_eq(&first, &second));

    let mut changed = (*first).clone();
    changed.profiles[0].name = "Changed".into();
    data.save_profiles(&changed).unwrap();
    let third = load_profiles_shared(&data).unwrap();
    assert!(
        !Arc::ptr_eq(&first, &third),
        "a save replaces the cached value"
    );
    assert_eq!(third.profiles[0].name, "Changed");
}

#[test]
fn a_profiles_and_library_pair_is_remembered_in_step_until_either_changes() {
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;

    let base = scratch!("data-profiles-in-step");
    let root = Data::with_cache(base);
    let data = root.game(test_games::SLOT_ID);
    let game = test_games::slot();
    Library::default().save(&data.index()).unwrap();
    Profiles::with_default(Vec::new())
        .save(&data.profiles_file())
        .unwrap();

    let library = load_library_shared(&data, &game).unwrap();
    let profiles = load_profiles_shared(&data).unwrap();
    assert!(!data.profiles_in_step(&profiles, &library));
    data.remember_profiles_in_step(&profiles, &library);
    assert!(data.profiles_in_step(&profiles, &library));

    // A save is a new library value, so the pair the next status reads is a new one.
    let library = data.save_library(Library::default()).unwrap();
    assert!(!data.profiles_in_step(&profiles, &library));
}

/// A game's data with a journal for profile `p1` and `marker` as its completion marker.
fn state_with_marker(name: &str, marker: &[u8]) -> (Data, PathBuf) {
    use crate::test_support::scratch;

    let data = Data::new(scratch!(name));
    let state = data.profile_state("p1");
    fs::create_dir_all(&state).unwrap();
    fs::write(state.join(JOURNAL_FILE), b"").unwrap();
    fs::write(state.join(DEPLOYMENT_FILE), marker).unwrap();
    (data, state)
}

#[test]
fn a_marker_this_build_cannot_trust_is_an_interrupted_apply() {
    let cases: [(&str, &[u8]); 4] = [
        ("data-marker-empty", b""),
        ("data-marker-corrupt", b"{ not json"),
        ("data-marker-no-fields", b"{}"),
        (
            "data-marker-newer",
            br#"{"schema_version":99,"method":"hardlink","files":1,"plan_hash":"abc"}"#,
        ),
    ];

    for (name, marker) in cases {
        let (data, state) = state_with_marker(name, marker);

        assert!(data.prepared_states().is_empty(), "{name}");
        assert_eq!(
            data.deployment_states().incomplete.get("p1"),
            Some(&state),
            "{name}"
        );
        assert!(data.deployed_state().is_none(), "{name}");
        assert_eq!(data.incomplete_state(), Some(state.clone()), "{name}");
        assert!(
            state.join(DEPLOYMENT_FILE).is_file(),
            "the untrusted marker is kept for diagnosis: {name}"
        );
    }
}

#[test]
fn a_marker_of_this_build_or_an_older_one_is_applied() {
    let cases: [(&str, &[u8]); 2] = [
        (
            "data-marker-current",
            br#"{"schema_version":1,"method":"hardlink","files":1,"plan_hash":"abc"}"#,
        ),
        (
            "data-marker-old",
            br#"{"method":"hardlink","files":1,"plan_hash":"abc"}"#,
        ),
    ];

    for (name, marker) in cases {
        let (data, state) = state_with_marker(name, marker);

        assert_eq!(data.prepared_states().get("p1"), Some(&state), "{name}");
        assert!(data.deployment_states().incomplete.is_empty(), "{name}");
        assert_eq!(data.deployed_state(), Some(state), "{name}");
        assert!(data.incomplete_state().is_none(), "{name}");
    }
}
