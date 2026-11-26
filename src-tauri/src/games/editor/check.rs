//! The draft lifecycle: check it, read an existing definition for editing, and write a valid one. The loader presets
//! are merged before the rules are checked, exactly as they are for a shipped game.

use std::fs;
use std::path::{Path, PathBuf};

use super::super::definition::GameDef;
use super::super::layers::DefinitionLayers;
use super::super::validate::{Section, duplicate_problems};
use super::super::{
    GameOrigin, catalog_games_dir, games_dir, invalid_user_definition, is_valid_id, user_games_dir,
};
use super::{
    DefinitionCheck, DefinitionProblemView, DefinitionSource, DefinitionSummary, ExecutableSummary,
    SavedDefinition,
};
use crate::error::{AppError, ErrorKind, io_at};
use crate::infra::store;

/// Checks a definition without touching the disk. The loader presets in `loader_dirs` are merged under it, in order.
pub fn check(text: &str) -> DefinitionCheck {
    check_with(text, &DefinitionLayers::current().loader_paths())
}

pub(super) fn check_with(text: &str, loader_dirs: &[PathBuf]) -> DefinitionCheck {
    let mut game = match GameDef::parse(text) {
        Ok(game) => game,
        Err(error) => {
            return DefinitionCheck {
                ok: false,
                id: String::new(),
                name: String::new(),
                problems: vec![problem("source", error.to_string())],
                summary: None,
            };
        }
    };
    let mut problems = Vec::new();
    // A merge replaces an entry with the same key, so a duplicate that is already in this file would disappear.
    for entry in duplicate_problems(&game.deploy, Section::Deploy) {
        problems.push(problem(section_key(entry.section), entry.message));
    }

    if let Err(detail) = super::super::loaders::apply(&mut game, loader_dirs) {
        problems.push(problem("loader", detail));
    }

    for entry in game.validate() {
        problems.push(problem(section_key(entry.section), entry.message));
    }

    let summary = Some(summary_of(&game));
    DefinitionCheck {
        ok: problems.is_empty(),
        id: game.id,
        name: game.name,
        problems,
        summary,
    }
}

/// The editor key for a check's section. These are the keys the window already knows; the message text never takes
/// part in the choice, so rewording a problem cannot move it to another section.
const fn section_key(section: Section) -> &'static str {
    match section {
        Section::General => "general",
        Section::Source | Section::Install => "source",
        Section::Loader => "loader",
        Section::Detection => "detection",
        Section::UserData => "profiles",
        Section::Deploy => "deploy",
        Section::LoadOrder => "load_order",
        Section::Steps => "steps",
        Section::Tools => "tools",
        Section::Logs => "extras",
    }
}

pub(in crate::games::editor) fn problem(
    section: &str,
    message: impl Into<String>,
) -> DefinitionProblemView {
    DefinitionProblemView {
        section: section.to_string(),
        message: message.into(),
    }
}

fn summary_of(game: &GameDef) -> DefinitionSummary {
    DefinitionSummary {
        game: game.view_with_origin(GameOrigin::User),
        requires: game.requires.clone(),
        executables: game
            .executables
            .iter()
            .map(|executable| ExecutableSummary {
                name: executable.name.clone(),
                path: executable.path.clone(),
            })
            .collect(),
        user_data: game.user_data.iter().map(|set| set.path.clone()).collect(),
    }
}

/// The error a writer gets when a draft is not valid. The editor checks first; this is the safety net.
fn invalid_error(check: &DefinitionCheck) -> AppError {
    let detail = check
        .problems
        .iter()
        .map(|problem| problem.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    invalid_user_definition(detail).with("id", &check.id)
}

/// Reads a definition for the editor. A user definition wins over an official one, which wins over a bundled file,
/// exactly as it does when the catalog loads.
pub fn source(id: &str) -> Result<DefinitionSource, AppError> {
    source_from(
        user_games_dir().as_deref(),
        catalog_games_dir().as_deref(),
        games_dir(),
        id,
    )
}

pub(super) fn source_from(
    user: Option<&Path>,
    catalog: Option<&Path>,
    shipped: &Path,
    id: &str,
) -> Result<DefinitionSource, AppError> {
    if !is_valid_id(id) {
        return Err(AppError::user("games.unknown").with("id", id));
    }

    let user_path = user.map(|dir| dir.join(id).join("game.toml"));
    let catalog_path = catalog.map(|dir| dir.join(id).join("game.toml"));
    let (path, origin) = match (user_path, catalog_path) {
        (Some(path), _) if path.is_file() => (path, GameOrigin::User),
        (_, Some(path)) if path.is_file() => (path, GameOrigin::Official),
        _ => (shipped.join(id).join("game.toml"), GameOrigin::Bundled),
    };

    if !path.is_file() {
        return Err(AppError::user("games.unknown").with("id", id));
    }

    let text = fs::read_to_string(&path).map_err(io_at(&path))?;
    Ok(DefinitionSource {
        id: id.to_string(),
        text,
        origin,
    })
}

/// Writes a definition into the user folder. Invalid text is refused before anything is written. A bundled or official
/// game may not be replaced; a user game is replaced only when `overwrite` is set.
pub fn save(text: &str, overwrite: bool) -> Result<SavedDefinition, AppError> {
    let user = user_games_dir()
        .ok_or_else(|| AppError::new("games.user_definitions_unavailable").kind(ErrorKind::Bug))?;
    save_to(
        &user,
        catalog_games_dir().as_deref(),
        games_dir(),
        &DefinitionLayers::current().loader_paths(),
        text,
        overwrite,
    )
}

pub(super) fn save_to(
    user: &Path,
    catalog: Option<&Path>,
    shipped: &Path,
    loader_dirs: &[PathBuf],
    text: &str,
    overwrite: bool,
) -> Result<SavedDefinition, AppError> {
    let check = check_with(text, loader_dirs);

    if !check.ok {
        return Err(invalid_error(&check));
    }

    let game = GameDef::parse(text).map_err(invalid_user_definition)?;

    if !is_valid_id(&game.id) {
        return Err(invalid_user_definition(format!(
            "invalid game id: {:?}",
            game.id
        )));
    }

    let bundled = shipped.join(&game.id).join("game.toml").is_file();
    let official = catalog.is_some_and(|dir| dir.join(&game.id).join("game.toml").is_file());

    if bundled || official {
        return Err(AppError::user("games.definition_bundled").with("id", &game.id));
    }

    let folder = user.join(&game.id);
    let path = folder.join("game.toml");

    if path.exists() && !overwrite {
        return Err(AppError::user("games.definition_exists").with("id", &game.id));
    }

    fs::create_dir_all(&folder).map_err(io_at(&folder))?;
    store::write_atomic(&path, text.as_bytes())?;
    super::super::forget_definition(&path);
    Ok(SavedDefinition {
        id: game.id,
        name: game.name,
    })
}

/// The merged definition behind a valid draft, for another module that needs to run the real rules on it. The import
/// preview uses it, so a test archive is placed exactly as the import would place it.
pub fn merged(text: &str) -> Result<GameDef, AppError> {
    let check = check(text);

    if !check.ok {
        return Err(invalid_error(&check));
    }

    let mut game = GameDef::parse(text).map_err(invalid_user_definition)?;
    super::super::loaders::apply(&mut game, &DefinitionLayers::current().loader_paths())
        .map_err(invalid_user_definition)?;
    Ok(game)
}

#[cfg(test)]
mod tests {
    use super::{Section, check_with, section_key};
    use crate::games::validate::Problem;

    #[test]
    fn every_section_maps_to_an_existing_editor_key() {
        let sections = [
            Section::General,
            Section::Source,
            Section::Loader,
            Section::Detection,
            Section::UserData,
            Section::Deploy,
            Section::LoadOrder,
            Section::Steps,
            Section::Tools,
            Section::Logs,
            Section::Install,
        ];
        assert_eq!(
            sections.map(section_key),
            [
                "general",
                "source",
                "loader",
                "detection",
                "profiles",
                "deploy",
                "load_order",
                "steps",
                "tools",
                "extras",
                "source",
            ]
        );
    }

    #[test]
    fn a_reworded_message_keeps_its_editor_section() {
        let before = Problem {
            section: Section::UserData,
            message: "user_data id is repeated: \"saves\"".to_string(),
        };
        let after = Problem {
            section: Section::UserData,
            message: "the same save set is declared twice".to_string(),
        };
        assert_eq!(section_key(before.section), "profiles");
        assert_eq!(section_key(after.section), section_key(before.section));
    }

    #[test]
    fn three_legacy_messages_keep_their_editor_sections() {
        use std::path::PathBuf;

        use crate::test_support::games::SLOT;

        let text = SLOT
            .replacen(
                "format = \"mods_settings\"\nfile = \"{documents}/Slot Game/mods.settings\"",
                "format = \"ue4ss_mods_txt\"\nfile = \"Other/mods.txt\"\npriority_min = 5\npriority_max = 5",
                1,
            )
            .replacen(
                "[deploy]",
                &format!(
                    "[install]\nname = \"Loader\"\nversion = \"1\"\nurl = \"https://example.com/loader.zip\"\nsha256 = \"{}\"\n\n[deploy]",
                    "a".repeat(64)
                ),
                1,
            );
        let loader_dirs = [PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../loaders")];
        let check = check_with(&text, &loader_dirs);
        let section_of = |start: &str| {
            check
                .problems
                .iter()
                .find(|problem| problem.message.starts_with(start))
                .map_or("missing", |problem| problem.section.as_str())
        };

        assert_eq!(
            section_of("load_order.file must lie in a slot folder of the game"),
            "deploy"
        );
        assert_eq!(
            section_of("priority_min must be less than priority_max"),
            "source"
        );
        assert_eq!(
            section_of("install: needs at least one check path that proves the loader is there"),
            "loader"
        );
    }
}
