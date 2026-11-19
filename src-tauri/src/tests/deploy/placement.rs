//! The import and the deployment agree on where a file lands: the library path the import stores and the game path
//! the plan writes are one rule seen from two sides. A change to a slot, mirror or alias must keep both sides equal.

use super::*;
use crate::deploy::ops::apply_deployment;
use crate::test_support::games as test_games;
use crate::test_support::import;
use crate::test_support::write;

/// The overlay layer of the profile that is active now.
fn layer(f: &Fixture, game_id: &str) -> PathBuf {
    f.data
        .game(game_id)
        .profile_state(&f.active_id(game_id))
        .join("mods")
}

/// Imports `entries` into `game_id`, enables the new mods and deploys the active profile into the overlay layer.
fn import_and_deploy(f: &Fixture, game_id: &str, name: &str, entries: &[(&str, &str)]) {
    let zip = f.zip(name, entries);
    import(&f.data, game_id, &zip, None, &mut |_, _| {}).unwrap();
    f.enable_all(game_id);
    apply_deployment(&f.data, game_id).unwrap();
}

#[test]
fn a_mirrored_archive_deploys_to_the_game_paths_it_stored() {
    let f = mirror_fixture("placement-mirror");
    import_and_deploy(
        &f,
        MIRROR_GAME,
        "Mirror Mod.zip",
        &[
            ("archive/pc/mod/a.archive", "a"),
            ("r6/scripts/MirrorMod/a.reds", "r"),
        ],
    );
    assert!(
        layer(&f, MIRROR_GAME)
            .join("archive/pc/mod/a.archive")
            .is_file()
    );
    assert!(
        layer(&f, MIRROR_GAME)
            .join("r6/scripts/MirrorMod/a.reds")
            .is_file()
    );
}

#[test]
fn a_slot_archive_deploys_into_its_mod_folder() {
    let f = fixture("placement-slot");
    import_and_deploy(
        &f,
        SLOT_GAME,
        "Slot Mod.zip",
        &[("Mods/modSlot/content/x.ws", "x")],
    );
    assert!(
        layer(&f, SLOT_GAME)
            .join("Mods/modSlotMod/content/x.ws")
            .is_file()
    );
}

#[test]
fn a_flat_archive_deploys_straight_into_the_slot() {
    let f = game_fixture("placement-flat", test_games::FLAT_ID, &["ph_ft/source"]);
    import_and_deploy(
        &f,
        test_games::FLAT_ID,
        "Flat Mod.zip",
        &[("ph_ft/source/custom.pak", "pak")],
    );
    assert!(
        layer(&f, test_games::FLAT_ID)
            .join("ph_ft/source/custom.pak")
            .is_file()
    );
}

#[test]
fn an_aliased_archive_folder_deploys_to_the_game_folder() {
    let f = game_fixture("placement-alias", test_games::ALIASED_ID, &["BepInEx/core"]);
    write(&f.game_root.join("winhttp.dll"), "loader");
    write(&f.game_root.join("BepInEx/core/BepInEx.dll"), "core");
    import_and_deploy(
        &f,
        test_games::ALIASED_ID,
        "Alias Mod.zip",
        &[("plugins/Example/Example.dll", "dll")],
    );
    assert!(
        layer(&f, test_games::ALIASED_ID)
            .join("BepInEx/plugins/AliasMod/Example.dll")
            .is_file()
    );
}
