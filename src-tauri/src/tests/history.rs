//! The list's history (undo, redo, restore), its restore preview, and the bisect search for a broken mod.

use super::*;
use crate::context::profile::history::list_history;
use crate::context::profile::history::redo_list;
use crate::context::profile::history::restore_list;
use crate::context::profile::history::undo_list;
use crate::deploy::remove::delete_mod;
use crate::library::Relations;
use crate::library::set_mod_relations;
use crate::profiles::actions::create_profile;
use crate::profiles::actions::delete_profile;
use crate::profiles::actions::save_profile;
use crate::profiles::actions::switch_profile;
use crate::profiles::bisect::bisect_answer;
use crate::profiles::bisect::bisect_start;
use crate::profiles::bisect::bisect_stop;
use crate::profiles::history::preview::preview_restore;
use crate::profiles::order::{preview_sort, sort_by_rules};
use crate::status::sync_profiles as status;
use crate::test_support::import;

#[test]
fn list_changes_can_be_undone_redone_and_restored() {
    let f = fixture("history");
    f.import_two_mods();
    let folders = |f: &Fixture| -> Vec<(String, bool)> {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .into_iter()
            .map(|m| (m.folder, m.enabled))
            .collect()
    };
    let initial = folders(&f);
    assert!(!status(&f.data, SLOT_GAME).unwrap().history.can_undo);
    f.enable_all(SLOT_GAME);
    let enabled = folders(&f);
    save_profile(&f.data, SLOT_GAME, &[row("modB", true), row("modA", false)]).unwrap();
    let last = folders(&f);

    undo_list(&f.data, SLOT_GAME).unwrap();
    assert_eq!(folders(&f), enabled);
    undo_list(&f.data, SLOT_GAME).unwrap();
    assert_eq!(folders(&f), initial);
    assert!(status(&f.data, SLOT_GAME).unwrap().history.can_redo);
    redo_list(&f.data, SLOT_GAME).unwrap();
    redo_list(&f.data, SLOT_GAME).unwrap();
    assert_eq!(folders(&f), last);
    assert_eq!(
        redo_list(&f.data, SLOT_GAME).unwrap_err().code,
        "history.nothing"
    );

    let kept = list_history(&f.data, SLOT_GAME).unwrap();
    let oldest = kept.last().unwrap();
    let summary = oldest
        .summary
        .as_ref()
        .expect("the oldest change has a summary");
    assert_eq!(summary.enabled_on, 2);

    // The preview is what restoring the oldest state changes now, not what that change did then.
    let preview = preview_restore(&f.data, SLOT_GAME, oldest.id).unwrap();
    assert!(preview.adds.is_empty());
    assert!(preview.removes.is_empty());
    assert_eq!(
        preview
            .switches_off
            .iter()
            .map(|entry| entry.folder.as_str())
            .collect::<Vec<_>>(),
        ["modB"]
    );

    restore_list(&f.data, SLOT_GAME, oldest.id).unwrap();
    assert_eq!(folders(&f), initial);
    // The restore itself can be undone.
    undo_list(&f.data, SLOT_GAME).unwrap();
    assert_eq!(folders(&f), last);
}

#[test]
fn a_sort_preview_writes_nothing_and_the_sort_is_one_undo() {
    let f = fixture("sort-preview");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let a = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|item| item.folder == "modA")
        .unwrap()
        .id
        .clone();
    let b = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|item| item.folder == "modB")
        .unwrap()
        .id
        .clone();
    set_mod_relations(
        &f.data,
        SLOT_GAME,
        &b,
        &Relations {
            overrides: vec![a],
            ..Relations::default()
        },
    )
    .unwrap();
    let order = |f: &Fixture| -> Vec<String> {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .into_iter()
            .map(|item| item.folder)
            .collect()
    };
    let before = order(&f);
    assert_eq!(before, ["modA", "modB"]);

    let preview = preview_sort(&f.data, SLOT_GAME).unwrap();
    assert_eq!(preview.moves.len(), 2);
    assert_eq!((preview.moves[0].from, preview.moves[0].to), (2, 1));
    assert_eq!(order(&f), before, "a preview must not write the list");

    sort_by_rules(&f.data, SLOT_GAME).unwrap();
    let sorted = order(&f);
    assert_eq!(sorted, ["modB", "modA"]);
    undo_list(&f.data, SLOT_GAME).unwrap();
    assert_eq!(order(&f), before, "the sort is one undo");
}

#[test]
fn a_restore_preview_names_the_mods_that_leave_and_come_back() {
    let f = fixture("history-preview");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    save_profile(&f.data, SLOT_GAME, &[row("modA", true)]).unwrap();

    let kept = list_history(&f.data, SLOT_GAME).unwrap();
    let before_removal = kept.first().expect("the removal is the newest change");
    assert_eq!(
        before_removal
            .summary
            .as_ref()
            .map(|summary| summary.removed),
        Some(1)
    );

    let preview = preview_restore(&f.data, SLOT_GAME, before_removal.id).unwrap();
    assert_eq!(
        preview
            .adds
            .iter()
            .map(|entry| entry.folder.as_str())
            .collect::<Vec<_>>(),
        ["modB"]
    );
    assert!(preview.removes.is_empty());

    let before_enabling = kept.last().expect("the enabling is the oldest change");
    let preview = preview_restore(&f.data, SLOT_GAME, before_enabling.id).unwrap();
    assert_eq!(
        preview
            .adds
            .iter()
            .map(|entry| entry.folder.as_str())
            .collect::<Vec<_>>(),
        ["modB"]
    );
    assert_eq!(
        preview
            .switches_off
            .iter()
            .map(|entry| entry.folder.as_str())
            .collect::<Vec<_>>(),
        ["modA"]
    );
}

#[test]
fn a_deleted_profile_does_not_leave_history_for_a_reused_id() {
    let f = fixture("history-deleted-profile");
    f.import_two_mods();
    let default = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Temp", Some(default.as_str()), false).unwrap();
    let temp = f.profile_id(SLOT_GAME, "Temp");
    switch_profile(&f.data, SLOT_GAME, &temp).unwrap();
    f.enable_all(SLOT_GAME);
    assert!(!list_history(&f.data, SLOT_GAME).unwrap().is_empty());

    delete_profile(&f.data, SLOT_GAME, &temp).unwrap();
    create_profile(&f.data, SLOT_GAME, "New", None, false).unwrap();
    let new = f.profile_id(SLOT_GAME, "New");
    switch_profile(&f.data, SLOT_GAME, &new).unwrap();
    assert_eq!(new, temp);
    assert!(list_history(&f.data, SLOT_GAME).unwrap().is_empty());
}

#[test]
fn a_preview_marks_a_mod_the_library_no_longer_has() {
    let f = fixture("history-missing");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let kept = list_history(&f.data, SLOT_GAME).unwrap();
    let oldest = kept.last().expect("the enabling is the oldest change");
    let id = oldest.id;
    delete_mod(&f.data, SLOT_GAME, "local:modb").unwrap();

    let preview = preview_restore(&f.data, SLOT_GAME, id).unwrap();
    let missing = preview
        .adds
        .iter()
        .find(|entry| entry.folder == "modB")
        .expect("the kept state still lists modB");
    assert!(missing.missing);
}

#[test]
fn an_index_without_summaries_still_lists_its_states() {
    let f = fixture("history-legacy");
    let profile = f.active_id(SLOT_GAME);
    let data = f.data.game(SLOT_GAME);
    let history = data.dir.join("history");
    std::fs::create_dir_all(&history).unwrap();
    std::fs::write(history.join("0.json"), r#"{"mods":[]}"#).unwrap();
    std::fs::write(
        history.join("index.json"),
        format!(
            r#"{{"next":1,"undo":[{{"id":0,"at":0,"profile":"{profile}","total":0,"enabled":0}}],"redo":[]}}"#
        ),
    )
    .unwrap();

    let entries = list_history(&f.data, SLOT_GAME).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].summary.is_none());
    assert_eq!(preview_restore(&f.data, SLOT_GAME, 0).unwrap().total, 0);
}

#[test]
fn the_search_for_a_broken_mod_halves_the_suspects_and_puts_the_list_back() {
    let f = fixture("bisect");
    for name in ["modA", "modB", "modC", "modD"] {
        let zip = f.zip(
            &format!("{name}.zip"),
            &[(format!("Mods/{name}/content/{name}.ws").as_str(), "x")],
        );
        import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    }
    f.enable_all(SLOT_GAME);
    bisect_start(&f.data, SLOT_GAME).unwrap();
    let on = |f: &Fixture| -> Vec<String> {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .into_iter()
            .filter(|m| m.enabled)
            .map(|m| m.folder)
            .collect()
    };
    assert_eq!(on(&f), ["modA", "modB"]);
    // The problem is gone with A and B: it is in C or D.
    bisect_answer(&f.data, SLOT_GAME, false).unwrap();
    assert_eq!(on(&f), ["modA", "modB", "modC"]);
    // The problem is back with C on: C holds it.
    bisect_answer(&f.data, SLOT_GAME, true).unwrap();
    let found = status(&f.data, SLOT_GAME).unwrap().bisect.unwrap();
    assert_eq!(found.found.as_deref(), Some("modC"));
    bisect_stop(&f.data, SLOT_GAME, true).unwrap();
    assert_eq!(on(&f), ["modA", "modB", "modD"]);
    assert!(status(&f.data, SLOT_GAME).unwrap().bisect.is_none());
}
