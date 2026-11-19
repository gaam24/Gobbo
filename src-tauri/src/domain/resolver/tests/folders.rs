//! Mod folders in slots: flat slots, mods that never conflict, and names of families that share one.

use super::*;

#[test]
fn mods_in_their_own_folders_never_conflict() {
    let files = ["content/blob0.bundle", "content/a.ws"];
    let plan = run(
        &plain(),
        &[m("modA", true, &files), m("modB", true, &files)],
    );
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.files.len(), 4);
}

#[test]
fn a_flat_slot_puts_files_in_its_folder_and_one_copy_wins() {
    let mods = [
        m("modA", true, &["content/a.pak"]),
        m("modB", true, &["other/a.pak"]),
    ];
    let plan = run(&flat(), &mods);
    assert_eq!(dests(&plan), ["Mods/a.pak"]);
    assert_eq!(plan.conflicts[0].path, "Mods/a.pak");
    assert_eq!(file_set(&plan), ["modA/content/a.pak"]);

    let chosen = choosing(&flat(), &mods, "Mods/a.pak", "modB");
    assert_eq!(file_set(&chosen), ["modB/other/a.pak"]);
}

#[test]
fn different_families_with_the_same_name_get_stable_suffixes() {
    let first = ModInput {
        display_name: "Same Name".into(),
        ..m("local:first", true, &["content/a.ws"])
    };
    let second = ModInput {
        display_name: "Same Name".into(),
        ..m("local:second", true, &["content/b.ws"])
    };
    let plan = run(&plain(), &[first, second]);
    let dest_of = |rel: &str| {
        plan.files
            .iter()
            .find(|file| plan.rel_path(file) == rel)
            .unwrap()
            .dest
            .clone()
    };
    assert_eq!(dest_of("content/a.ws"), "Mods/modSameName/content/a.ws");
    let second = dest_of("content/b.ws");
    assert!(second.starts_with("Mods/modSameName-"), "{second}");
    assert_eq!(
        second.len(),
        "Mods/modSameName-".len() + 8 + "/content/b.ws".len()
    );
}
