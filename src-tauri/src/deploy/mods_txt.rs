//! The mod list UE4SS reads, `mods.txt`: one line per mod folder, `Name : 1` to load the mod or `Name : 0` to keep it off,
//! and UE4SS loads the mods in the order their lines come. Gobbo writes only the mods the user put in the load order.
//! The list the package ships, or the game already has, keeps its other lines. The ordered mods take the place of the
//! first line of their own, so a mod the user moves ahead of a package default loads ahead of it.

use std::fs;
use std::path::Path;

use rustc_hash::FxHashSet;

use super::settings;
use crate::deploy::Target;
use crate::deploy::error::DeployError;
use crate::domain::pathkey::path_key;
use crate::domain::resolver::{DeployPlan, SettingsRow};
use crate::error::FileError;
use crate::infra::fs::remove_file_if_exists;

/// The text of the list with `rows` in their priority order. `base` is the list the package or the game ships, `None`
/// when there is none. `None` when there is no row: then no list is written, and the layer keeps what it has.
fn merge(base: Option<&str>, rows: &[SettingsRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    // A list that uses plain newlines keeps them; Windows files and new lists use CRLF.
    let eol = if base.is_some_and(|text| text.contains('\n') && !text.contains("\r\n")) {
        "\n"
    } else {
        "\r\n"
    };

    let mut ordered: Vec<&SettingsRow> = rows.iter().collect();
    ordered.sort_by_key(|row| row.priority);
    let block: Vec<String> = ordered
        .iter()
        .map(|row| format!("{} : {}", row.folder, u8::from(row.enabled)))
        .collect();
    let keys: FxHashSet<String> = ordered.iter().map(|row| path_key(&row.folder)).collect();

    let mut lines: Vec<String> = Vec::new();
    let mut placed = false;

    for line in base.unwrap_or_default().lines() {
        let ours = name_of(line).is_some_and(|name| keys.contains(&path_key(name)));

        if !ours {
            lines.push(line.to_string());
        } else if !placed {
            lines.extend(block.iter().cloned());
            placed = true;
        }
    }

    if !placed {
        lines.extend(block);
    }

    Some(settings::join(&lines, eol))
}

/// The mod folder a line names: the text before the colon, when the line is not a comment or blank.
fn name_of(line: &str) -> Option<&str> {
    let trimmed = line.trim().trim_start_matches('\u{feff}').trim();

    if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with("//") {
        return None;
    }

    let (name, _) = trimmed.split_once(':')?;
    let name = name.trim();
    (!name.is_empty()).then_some(name)
}

/// The text the list starts from: the file the plan places at `rel`, read from the library, else the game's own file.
/// The planned copy is a hardlink, so this only reads it; the write never goes through it.
fn base_text(
    plan: &DeployPlan,
    library: &Path,
    root: &Path,
    rel: &str,
) -> Result<Option<String>, DeployError> {
    let key = path_key(rel);

    if let Some(file) = plan.files.iter().find(|file| path_key(&file.dest) == key) {
        let source = library.join(plan.folder(file)).join(plan.rel_path(file));
        return settings::read(&source);
    }

    settings::read(&root.join(rel))
}

/// Writes the target's list into the mod layer, when the game keeps one in the deployment root. `layer_base` is the
/// layer folder of the deployment root: the list sits at the same relative path below it. Nothing is written when the
/// plan has no row: a rebuilt layer then has no list, and a reused one keeps the file the plan placed (`matches_plan`
/// checks it).
pub fn write_order(
    plan: &DeployPlan,
    library: &Path,
    target: &Target,
    layer_base: &Path,
) -> Result<(), DeployError> {
    let Some(rel) = target.order_file.as_deref() else {
        return Ok(());
    };
    // Without rows nothing is written, so the list the package or the game ships is not read either.
    if plan.settings.is_empty() {
        return Ok(());
    }
    let base = base_text(plan, library, &target.root, rel)?;

    if let Some(text) = merge(base.as_deref(), &plan.settings) {
        write_layer(&layer_base.join(rel), &text)?;
    }

    Ok(())
}

/// Writes the list into the mod layer at `layer_file`. The planned copy of that file, when the plan has one, is a
/// hardlink into the library: the link is removed first, so the write makes a new file and the library copy stays as
/// it is.
fn write_layer(layer_file: &Path, text: &str) -> Result<(), DeployError> {
    if let Some(parent) = layer_file.parent() {
        fs::create_dir_all(parent).map_err(|e| FileError::new(parent, e))?;
    }

    remove_file_if_exists(layer_file).map_err(|e| FileError::new(layer_file, e))?;
    settings::write(layer_file, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(folder: &str, enabled: bool, priority: u32) -> SettingsRow {
        SettingsRow {
            folder: folder.into(),
            enabled,
            priority,
        }
    }

    #[test]
    fn the_ordered_mods_come_in_priority_order_with_their_state() {
        let text = merge(None, &[row("ModB", true, 2), row("ModA", false, 1)]).unwrap();
        assert_eq!(text, "ModA : 0\r\nModB : 1\r\n");
    }

    #[test]
    fn no_row_writes_no_list() {
        assert_eq!(merge(Some("Keybinds : 1\r\n"), &[]), None);
        assert_eq!(merge(None, &[]), None);
    }

    #[test]
    fn a_package_default_keeps_its_line_and_the_ordered_mod_takes_the_first_place_of_its_own() {
        // The package lists its own mods; the user puts ModB ahead of the first of them and ModA behind the defaults.
        let base = "Keybinds : 1\r\nModA : 1\r\nBPModLoaderMod : 1\r\nModB : 0\r\n";
        let text = merge(Some(base), &[row("ModA", true, 2), row("ModB", true, 1)]).unwrap();
        assert_eq!(
            text,
            "Keybinds : 1\r\nModB : 1\r\nModA : 1\r\nBPModLoaderMod : 1\r\n"
        );
    }

    #[test]
    fn a_list_the_package_ships_is_never_written_through_its_library_link() {
        use crate::domain::deploy::Deploy;
        use crate::domain::resolver::{ModInput, PlanMode};
        use crate::test_support::{scratch, write};
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let base = scratch!("package-list");
        let rel = "Binaries/Win64/ue4ss/Mods/mods.txt";
        let library = base.join("library");
        let library_file = library.join("pkg").join(rel);
        write(&library_file, "Keybinds : 1\r\nBPModLoaderMod : 1\r\n");

        let input = ModInput {
            id: "local:pkg".into(),
            folder: "pkg".into(),
            slot: "".into(),
            display_name: "pkg".into(),
            enabled: true,
            priority: None,
            files: Arc::from(vec![rel.to_string()]),
            layout: None,
        };
        let mut plan = crate::deploy::plan_with_mode(
            &Deploy::default(),
            &[input],
            &BTreeMap::new(),
            PlanMode::Full,
        );
        plan.settings = vec![row("ModA", true, 1)];

        let mut target = crate::test_support::target(&base);
        target.order_file = Some(rel.to_string());
        let layer = base.join("mods");
        let layer_file = layer.join(rel);
        fs::create_dir_all(layer_file.parent().unwrap()).unwrap();
        // The placed copy is a hardlink to the library, as the overlay makes it.
        fs::hard_link(&library_file, &layer_file).unwrap();

        write_order(&plan, &library, &target, &layer).unwrap();

        assert_eq!(
            fs::read_to_string(&layer_file).unwrap(),
            "Keybinds : 1\r\nBPModLoaderMod : 1\r\nModA : 1\r\n"
        );
        assert_eq!(
            fs::read_to_string(&library_file).unwrap(),
            "Keybinds : 1\r\nBPModLoaderMod : 1\r\n",
            "the library copy must not change"
        );
    }

    #[test]
    fn with_no_package_list_the_game_file_is_the_base_and_the_game_keeps_it() {
        use crate::test_support::write;

        let base = crate::test_support::scratch!("game-list");
        let mut target = crate::test_support::target(&base);
        target.order_file = Some("Binaries/Win64/Mods/mods.txt".to_string());
        let game_file = target.root.join("Binaries/Win64/Mods/mods.txt");
        write(&game_file, "Keybinds : 1\r\n");
        let plan = crate::domain::resolver::DeployPlan {
            settings: vec![row("ModA", false, 3)],
            ..crate::domain::resolver::DeployPlan::default()
        };
        let layer = base.join("mods");

        write_order(&plan, &base.join("library"), &target, &layer).unwrap();

        assert_eq!(
            fs::read_to_string(layer.join("Binaries/Win64/Mods/mods.txt")).unwrap(),
            "Keybinds : 1\r\nModA : 0\r\n"
        );
        assert_eq!(fs::read_to_string(&game_file).unwrap(), "Keybinds : 1\r\n");
    }

    #[test]
    fn a_mod_the_list_does_not_have_is_appended_at_the_end() {
        let text = merge(Some("Keybinds : 1\r\n"), &[row("ModA", true, 1)]).unwrap();
        assert_eq!(text, "Keybinds : 1\r\nModA : 1\r\n");
    }

    #[test]
    fn names_match_ignoring_case_and_comments_and_blank_lines_stay() {
        let base = "; the package's own list\r\n\r\nmoda : 1\r\n";
        let text = merge(Some(base), &[row("ModA", false, 1)]).unwrap();
        assert_eq!(text, "; the package's own list\r\n\r\nModA : 0\r\n");
    }

    #[test]
    fn a_list_with_plain_newlines_keeps_them() {
        let text = merge(Some("Keybinds : 1\n"), &[row("ModA", true, 1)]).unwrap();
        assert_eq!(text, "Keybinds : 1\nModA : 1\n");
    }

    #[test]
    fn a_name_is_read_before_the_colon_only() {
        assert_eq!(name_of("  ModA : 1 "), Some("ModA"));
        assert_eq!(name_of("; ModA : 1"), None);
        assert_eq!(name_of(""), None);
        assert_eq!(name_of("no colon here"), None);
    }

    #[test]
    fn a_package_list_is_written_per_apply_and_restored_when_the_rows_go() {
        use crate::deploy::{apply, undo};
        use crate::domain::deploy::Deploy;
        use crate::domain::resolver::{ModInput, PlanMode};
        use crate::test_support::{scratch, write};
        use std::collections::BTreeMap;
        use std::sync::Arc;

        let base = scratch!("package-list-lifecycle");
        let rel = "Binaries/Win64/ue4ss/Mods/mods.txt";
        let library = base.join("library");
        let package = "Keybinds : 1\r\nBPModLoaderMod : 1\r\n";
        write(&library.join("pkg").join(rel), package);

        let input = ModInput {
            id: "local:pkg".into(),
            folder: "pkg".into(),
            slot: "".into(),
            display_name: "pkg".into(),
            enabled: true,
            priority: None,
            files: Arc::from(vec![rel.to_string()]),
            layout: None,
        };
        let mut plan = crate::deploy::plan_with_mode(
            &Deploy::default(),
            &[input],
            &BTreeMap::new(),
            PlanMode::Full,
        );
        let mut target = crate::test_support::target(&base);
        target.order_format = Some(crate::games::definition::OrderFormat::Ue4ssModsTxt);
        target.order_file = Some(rel.to_string());
        let mods = target.state.join("mods");
        let layer_list = mods.join(rel);
        let reusable = |plan: &DeployPlan, order: Option<&str>| {
            crate::deploy::overlay::matches_plan(plan, &library, &mods, Path::new(""), order)
                .unwrap()
        };

        // The rows are written over the package's list, so the layer holds a written copy, not the library's link.
        plan.settings = vec![row("ModA", true, 1)];
        apply(&plan, &library, &target).unwrap();
        assert_eq!(
            fs::read_to_string(&layer_list).unwrap(),
            "Keybinds : 1\r\nBPModLoaderMod : 1\r\nModA : 1\r\n"
        );
        undo(&target.journal_roots()).unwrap();

        // The same plan reuses the layer: the rewritten list is left out of the check, and is not without that.
        assert!(reusable(&plan, Some(rel)));
        assert!(!reusable(&plan, None));
        apply(&plan, &library, &target).unwrap();
        assert_eq!(
            fs::read_to_string(&layer_list).unwrap(),
            "Keybinds : 1\r\nBPModLoaderMod : 1\r\nModA : 1\r\n"
        );
        undo(&target.journal_roots()).unwrap();

        // Without rows the layer is built again: the package's own list comes back, and the library copy is unchanged.
        plan.settings.clear();
        assert!(!reusable(&plan, Some(rel)));
        apply(&plan, &library, &target).unwrap();
        assert_eq!(fs::read_to_string(&layer_list).unwrap(), package);
        assert_eq!(
            fs::read_to_string(library.join("pkg").join(rel)).unwrap(),
            package
        );
        undo(&target.journal_roots()).unwrap();
    }
}
