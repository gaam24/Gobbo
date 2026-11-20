//! Conflicts between mods: who wins each file, the deployment preview and files the game wrote itself.

use super::*;
use crate::deploy::ops::apply_deployment;
use crate::deploy::ops::undo_deployment;
use crate::deploy::overwrite::overwrite_files;
use crate::deploy::overwrite::overwrite_to_mod;
use crate::deploy::preview::preview_deployment;
use crate::library::set_file_winner;
use crate::profiles::actions::save_profile;
use crate::status::details::mod_details;
use crate::status::sync_profiles as status;
use crate::status::views::FileState;

#[test]
fn conflicting_file_counts_wins_and_loses() {
    let f = fixture("conflict");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);

    let status = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(status.conflict_count, 1);
    assert_eq!((status.mods[0].wins, status.mods[0].loses), (1, 0));
    assert_eq!((status.mods[1].wins, status.mods[1].loses), (0, 1));
    let details = mod_details(&f.data, SLOT_GAME, "modA").unwrap();
    assert_eq!(details.conflicts.len(), 1);
    assert_eq!(details.conflicts[0].winner, status.mods[0].id);
}

#[test]
fn the_conflict_explorer_matches_the_status_and_leaves_disabled_mods_out() {
    let f = fixture("explorer");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);

    let shared = status(&f.data, SLOT_GAME).unwrap();
    let explorer = crate::deploy::explorer::conflicts(&f.data, SLOT_GAME).unwrap();
    assert_eq!(explorer.total, shared.conflict_count);
    assert_eq!(explorer.entries.len(), 1);
    let entry = &explorer.entries[0];
    assert_eq!(entry.path, "content/x.ws");
    assert_eq!(entry.winner, shared.mods[0].id);
    assert_eq!(entry.losers, [shared.mods[1].id.clone()]);
    assert_eq!(entry.copies, 2);
    assert!(entry.bytes > 0);

    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", false)]).unwrap();
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().conflict_count, 0);
    assert!(
        crate::deploy::explorer::conflicts(&f.data, SLOT_GAME)
            .unwrap()
            .entries
            .is_empty()
    );
}

#[test]
fn the_conflict_explorer_matches_a_path_that_differs_only_in_case() {
    let f = fixture("explorer-case");

    for (name, file, text) in [
        ("modA.zip", "Mods/modA/content/X.ws", "from A"),
        ("modB.zip", "Mods/modB/content/x.ws", "from B"),
    ] {
        let zip = f.zip(name, &[(file, text)]);
        import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    }

    f.enable_all(SLOT_GAME);

    let view = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(view.conflict_count, 1);
    let mod_b = view
        .mods
        .iter()
        .find(|mod_view| mod_view.folder == "modB")
        .unwrap()
        .id
        .clone();
    // The chosen copy has another spelling than the conflict's path.
    set_file_winner(&f.data, SLOT_GAME, "content/X.ws", Some(&mod_b)).unwrap();

    let explorer = crate::deploy::explorer::conflicts(&f.data, SLOT_GAME).unwrap();
    assert_eq!(explorer.entries.len(), 1);
    assert_eq!(explorer.entries[0].winner, mod_b);
    assert!(
        explorer.entries[0].bytes > 0,
        "the winning copy's size is looked up through its path key"
    );
}

#[test]
fn a_conflict_preview_reads_an_image_copy_from_the_library_only() {
    let f = fixture("explorer-preview-image");
    let zip_a = f.zip("modA.zip", &[("Mods/modA/content/x.png", "image-a")]);
    let zip_b = f.zip("modB.zip", &[("Mods/modB/content/x.png", "image-b")]);
    import(&f.data, SLOT_GAME, &zip_a, None, &mut |_, _| {}).unwrap();
    import(&f.data, SLOT_GAME, &zip_b, None, &mut |_, _| {}).unwrap();
    f.enable_all(SLOT_GAME);
    let explorer = crate::deploy::explorer::conflicts(&f.data, SLOT_GAME).unwrap();
    let entry = &explorer.entries[0];

    let winner =
        crate::deploy::explorer::preview(&f.data, SLOT_GAME, &entry.path, &entry.winner).unwrap();
    assert_eq!(winner.extension, "png");
    assert!(
        winner
            .data_url
            .as_deref()
            .is_some_and(|url| url.starts_with("data:image/png;base64,"))
    );
    // The loser is read from its own folder, not from the winner's copy.
    let loser = crate::deploy::explorer::preview(&f.data, SLOT_GAME, &entry.path, &entry.losers[0])
        .unwrap();
    assert_ne!(loser.data_url.as_deref(), winner.data_url.as_deref());
    // A path that is not in the plan never reaches the disk.
    let outside =
        crate::deploy::explorer::preview(&f.data, SLOT_GAME, "../secret.png", &entry.winner)
            .unwrap();
    assert_eq!(outside.data_url, None);
}

#[test]
fn a_conflict_preview_says_when_the_window_cannot_show_a_format() {
    let f = fixture("explorer-preview-texture");
    let zip_a = f.zip("modA.zip", &[("Mods/modA/content/x.dds", "texture-a")]);
    let zip_b = f.zip("modB.zip", &[("Mods/modB/content/x.dds", "texture-b")]);
    import(&f.data, SLOT_GAME, &zip_a, None, &mut |_, _| {}).unwrap();
    import(&f.data, SLOT_GAME, &zip_b, None, &mut |_, _| {}).unwrap();
    f.enable_all(SLOT_GAME);
    let explorer = crate::deploy::explorer::conflicts(&f.data, SLOT_GAME).unwrap();
    let entry = &explorer.entries[0];

    let preview =
        crate::deploy::explorer::preview(&f.data, SLOT_GAME, &entry.path, &entry.winner).unwrap();
    assert_eq!(preview.extension, "dds");
    assert!(preview.bytes > 0);
    assert_eq!(preview.data_url, None);
}

#[test]
fn mod_details_says_who_wins_each_file() {
    let f = fixture("details");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);

    let loser = mod_details(&f.data, SLOT_GAME, "modB").unwrap();
    assert_eq!(loser.files[0].path, "content/x.ws");
    assert_eq!(loser.files[0].state, FileState::Lose);
    assert_eq!(loser.files[0].result.code, "file.overridden_by");
    assert_eq!(loser.files[0].result.params["winner"], "modA");

    let winner = mod_details(&f.data, SLOT_GAME, "modA").unwrap();
    assert_eq!(winner.files[0].state, FileState::Win);
}

#[test]
fn files_the_game_wrote_become_a_mod_of_their_own() {
    let f = fixture("overwrite");
    f.import_two_mods();
    let overwrite = f.data.game(SLOT_GAME).profile_state("p1").join("overwrite");
    crate::test_support::write(
        &overwrite.join("Mods/mod0000_MergedFiles/content/scripts/merged.ws"),
        "merged",
    );
    assert!(status(&f.data, SLOT_GAME).unwrap().has_overwrite);
    assert_eq!(overwrite_files(&f.data, SLOT_GAME).unwrap().files.len(), 1);

    overwrite_to_mod(&f.data, SLOT_GAME, "Merged scripts").unwrap();
    let now = status(&f.data, SLOT_GAME).unwrap();
    assert!(!now.has_overwrite);
    let merged = &now.mods[0];
    assert_eq!(
        (merged.folder.as_str(), merged.enabled, merged.slot.as_str()),
        ("Merged scripts", true, "")
    );
    // Its files keep their game paths, so the merged mod lands where Script Merger put it.
    let plan_files = mod_details(&f.data, SLOT_GAME, "Merged scripts")
        .unwrap()
        .files;
    assert_eq!(
        plan_files[0].path,
        "Mods/mod0000_MergedFiles/content/scripts/merged.ws"
    );
    assert_eq!(
        overwrite_to_mod(&f.data, SLOT_GAME, "Again")
            .unwrap_err()
            .code,
        "overwrite.empty"
    );
}

#[test]
fn a_preview_names_the_changes_a_deployment_makes() {
    let f = fixture("preview");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let first = preview_deployment(&f.data, SLOT_GAME).unwrap();
    assert_eq!(first.added.len(), 2);
    assert!(!first.replaces);
    // Both mods ship `content/x.ws`, a script the game reads from one mod only.
    assert_eq!(first.conflicts, 1);
    assert!(first.problems.is_empty(), "{:?}", first.problems);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    save_profile(&f.data, SLOT_GAME, &[row("modB", true)]).unwrap();
    let second = preview_deployment(&f.data, SLOT_GAME).unwrap();
    assert!(second.replaces);
    assert!(second.added.is_empty());
    assert_eq!(second.removed, ["modA"]);
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
fn a_chosen_file_winner_is_kept_per_profile_and_can_be_cleared() {
    let f = fixture("file-winner");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let mod_b = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|m| m.folder == "modB")
        .unwrap()
        .id
        .clone();
    let key = path_key("content/x.ws");

    set_file_winner(&f.data, SLOT_GAME, "content/x.ws", Some(&mod_b)).unwrap();
    let chosen = load_profiles(&f.data.game(SLOT_GAME)).unwrap();
    assert_eq!(chosen.active().winners.get(&key), Some(&mod_b));
    assert_eq!(chosen.active().winner_changes(), 1);

    set_file_winner(&f.data, SLOT_GAME, "content/x.ws", None).unwrap();
    let cleared = load_profiles(&f.data.game(SLOT_GAME)).unwrap();
    assert!(cleared.active().winners.is_empty());
    assert_eq!(cleared.active().winner_changes(), 0);

    assert!(set_file_winner(&f.data, SLOT_GAME, "content/x.ws", Some("nope")).is_err());
}
