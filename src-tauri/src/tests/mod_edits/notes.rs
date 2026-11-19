//! The notes a user types on a mod: the environment line and the instructions.

use super::*;
use crate::library::set_mod_instructions;
use crate::packs::export::export_pack;
use crate::packs::import::import_pack;
use crate::packs::model::PackArchives;
use crate::settings::config::Settings;
use crate::status::sync_profiles as status;

#[test]
fn saving_refuses_a_bad_environment_line() {
    let f = fixture("env-bad");
    let err = f
        .save_settings(
            SLOT_GAME,
            Settings {
                launch_env: "BEZ_ZNAKU".to_string(),
                ..f.settings()
            },
        )
        .unwrap_err();
    assert_eq!(err.code, "launch.env_no_equals", "{err}");
}

#[test]
fn instructions_are_trimmed_and_exported_only_where_they_are_set() {
    let f = fixture("instructions");
    f.import_two_mods();
    let mod_a = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|m| m.folder == "modA")
        .unwrap()
        .id
        .clone();

    set_mod_instructions(&f.data, SLOT_GAME, &mod_a, "  Copy into Mods.  ").unwrap();

    let shown = status(&f.data, SLOT_GAME).unwrap();
    let text = shown.mods.iter().find(|m| m.id == mod_a).unwrap();
    assert_eq!(text.instructions, "Copy into Mods.");
    // The instructions travel with a pack and land where the receiver has none.
    let pack = f.base.join("instructions.gobbopack");
    export_pack(
        &f.data,
        SLOT_GAME,
        &pack,
        &pack_options(&f, "Notes", PackArchives::All),
    )
    .unwrap();
    let target = fixture("instructions-target");
    import_pack(&target.data, SLOT_GAME, &pack).unwrap();
    let received = status(&target.data, SLOT_GAME).unwrap();
    assert!(
        received
            .mods
            .iter()
            .any(|m| m.instructions == "Copy into Mods.")
    );
}
