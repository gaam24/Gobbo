//! Per-profile game data outside the overlay: `mods.settings`, saves and other files or folders.
//!
//! Each profile keeps its copy under `state/<profile>/data/`. Before a launch the active profile's copy is
//! restored to the real paths, and after the game exits the real paths are copied back. The owner file and the switch
//! order are shared with the `user_data` sets in `store.rs`.
//!
//! The paths come from `profile_data` in `game.toml` (templates), `Settings.profile_data` (user extras)
//! and the game's settings file. Game-specific file names never live in this module.
//! Copying is deliberate and simple: it works for games whose saves live outside the game folder and needs no
//! platform mount support. Large folders cost a copy, so the UI warns and the user chooses what is isolated.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::store::{self, MaterializedPath, OwnerFile, ProfileStore};
use crate::context::target::{path_context, settings_file};
use crate::context::{Data, Settings};
use crate::domain::naming;
use crate::domain::pathkey;
use crate::error::AppError;
use crate::games::GameDef;
use crate::infra::json;

const OWNER: OwnerFile = OwnerFile {
    name: "data_owner.json",
    invalid_code: "profile_data.owner_invalid",
    journal: "profile-switch-data.json",
};
const MANIFEST_FILE: &str = "data_paths.json";
const DATA_DIR: &str = "data";

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    paths: Vec<String>,
}

/// The manifest of the paths materialized for a profile.
pub fn manifest(state: &Path) -> PathBuf {
    state.join(MANIFEST_FILE)
}

/// The profile whose data is currently materialized in the real paths.
pub fn owner(data: &Data) -> Result<Option<String>, AppError> {
    OWNER.read(data)
}

/// Paths from the game definition, resolved for this installation. The definition is the only place that
/// knows which files a game has; the code only resolves the templates.
pub fn game_paths(data: &Data, game: &GameDef, settings: &Settings) -> Vec<String> {
    if game.profile_data.is_empty() {
        return Vec::new();
    }

    if settings.game_root.trim().is_empty() {
        return Vec::new();
    }

    let context = path_context(data, game, settings);
    game.profile_data
        .iter()
        .filter_map(|template| {
            context
                .resolve(template)
                .ok()
                .map(|path| path.display().to_string())
        })
        .filter(|path| !inside_user_data(data, game, settings, Path::new(path)))
        .collect()
}

/// The paths materialized for one profile: the game definition, the user's extra list and the settings file.
pub fn paths(data: &Data, game: &GameDef, settings: &Settings) -> Vec<String> {
    let mut paths = game_paths(data, game, settings);
    paths.extend(
        settings
            .profile_data
            .iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty()),
    );

    if let Some(file) = settings_file(game, settings) {
        let path = file.display().to_string();

        if !path.is_empty() {
            paths.push(path);
        }
    }
    // A user-data set owns its whole subtree. The legacy copy list must not also capture the order file or a path the
    // user added inside it, or the real folder would be written while the overlay redirects the set.
    paths.retain(|path| !inside_user_data(data, game, settings, Path::new(path)));
    let mut seen = std::collections::BTreeSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    paths
}

/// Drops the paths a user-data set owns, even from an old manifest written before the set was declared.
fn without_user_data(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    paths: Vec<String>,
) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| !inside_user_data(data, game, settings, Path::new(path)))
        .collect()
}

/// Whether `path` is a user-data root or lies below one. Windows compares ignoring case; Linux compares the paths as
/// they are.
pub(super) fn inside_user_data(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    path: &Path,
) -> bool {
    crate::context::profile::user_data::real_roots(data, game, settings)
        .into_iter()
        .any(|(_, root)| root.is_ok_and(|root| pathkey::is_inside(&root, path)))
}

/// Makes `target` the materialized profile. The owner's data is saved first. On the first run the current real
/// files seed the first profile; later profiles start from their own store and do not inherit another profile's data.
pub fn switch(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    target: &str,
) -> Result<(), AppError> {
    store::switch(&DataStore { game, settings }, data, target)
}

/// Copies the currently materialized profile's real files back into its store. Safe to call when no profile
/// owns the data. Used after the game exits and before an undo.
pub fn capture_current(data: &Data, game: &GameDef, settings: &Settings) -> Result<(), AppError> {
    store::capture_current(&DataStore { game, settings }, data)
}

/// Captures only the settings file after a deployment wrote it. Other data is already in the store.
pub fn capture_settings(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
) -> Result<(), AppError> {
    let Some(file) = settings_file(game, settings) else {
        return Ok(());
    };

    if inside_user_data(data, game, settings, &file) {
        return Ok(());
    }

    let path = file.display().to_string();

    if path.is_empty() {
        return Ok(());
    }

    store::capture(&[MaterializedPath {
        store: store_path(data, profile, &path),
        real: file,
    }])
}

/// Restores the target profile's stored data to the real paths. `remove_missing` makes a profile without a
/// stored entry start empty instead of inheriting the previous profile's files.
pub fn restore_profile(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    remove_missing: bool,
) -> Result<(), AppError> {
    store::restore_profile(&DataStore { game, settings }, data, profile, remove_missing)
}

/// The legacy copy list as a store: the paths come from the definition, the user's extras and the settings file, and
/// the manifest keeps the list that was materialized even when the settings change later.
struct DataStore<'a> {
    game: &'a GameDef,
    settings: &'a Settings,
}

impl ProfileStore for DataStore<'_> {
    fn owner(&self) -> OwnerFile {
        OWNER
    }

    fn entries(&self, data: &Data, profile: &str) -> Result<Vec<MaterializedPath>, AppError> {
        let paths = without_user_data(
            data,
            self.game,
            self.settings,
            read_manifest(data, profile).unwrap_or_else(|| paths(data, self.game, self.settings)),
        );
        Ok(paths
            .into_iter()
            .map(|path| MaterializedPath {
                store: store_path(data, profile, &path),
                real: PathBuf::from(path),
            })
            .collect())
    }

    fn prepare(&self, data: &Data, profile: &str) -> Result<(), AppError> {
        write_manifest(data, profile, &paths(data, self.game, self.settings))
    }
}

fn store_path(data: &Data, profile: &str, source: &str) -> PathBuf {
    data.profile_state(profile)
        .join(DATA_DIR)
        .join(naming::path_hash(source))
}

fn manifest_path(data: &Data, profile: &str) -> PathBuf {
    data.profile_state(profile).join(MANIFEST_FILE)
}

fn read_manifest(data: &Data, profile: &str) -> Option<Vec<String>> {
    crate::infra::json::read_json_or_none::<Manifest>(&manifest_path(data, profile))
        .map(|manifest| manifest.paths)
}

fn write_manifest(data: &Data, profile: &str, paths: &[String]) -> Result<(), AppError> {
    json::write_json(
        &manifest_path(data, profile),
        &Manifest {
            paths: paths.to_vec(),
        },
    )
}

#[cfg(test)]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
mod tests {
    use super::*;
    use crate::context::Data;
    use crate::test_support::scratch;
    use std::fs;

    /// A game definition template, not a format string.
    const SETTINGS_FILE_TEMPLATE: &str = "{settings}/mods.settings";

    fn fixture(name: &str) -> (Data, GameDef, Settings, PathBuf, PathBuf) {
        let base = scratch!(name);
        let settings_dir = base.join("settings");
        let saves = base.join("saves");
        fs::create_dir_all(&settings_dir).unwrap();
        fs::create_dir_all(&saves).unwrap();
        let settings_file = settings_dir.join("mods.settings");
        fs::write(&settings_file, "one").unwrap();
        fs::write(saves.join("slot.sav"), "one").unwrap();
        let game = crate::test_support::games::slot();
        let settings = Settings {
            // No game root: this test must never touch the developer's real Documents folder through a
            // `{documents}` template in a game definition.
            game_root: String::new(),
            settings_dir: settings_dir.display().to_string(),
            profile_data: vec![saves.display().to_string()],
            ..Settings::default()
        };
        (
            Data::new(base.join("data")),
            game,
            settings,
            settings_file,
            saves,
        )
    }

    #[test]
    fn game_definition_paths_are_resolved_and_do_not_duplicate_the_settings_file() {
        let (data, mut game, mut settings, settings_file, saves) = fixture("game-paths");
        // A real game root keeps `{game}` and `{settings}` resolvable without touching the home folder.
        settings.game_root = saves.display().to_string();
        game.profile_data = vec![SETTINGS_FILE_TEMPLATE.to_string()];

        let resolved = game_paths(&data, &game, &settings);
        assert_eq!(resolved, [settings_file.display().to_string()]);

        let all = paths(&data, &game, &settings);
        let expected = settings_file.display().to_string();
        assert_eq!(all.iter().filter(|path| **path == expected).count(), 1);
    }

    #[test]
    fn a_settings_file_inside_a_user_data_set_is_left_to_the_set() {
        let (data, mut game, mut settings, settings_file, _) = fixture("user-data-overlap");
        let game_root = settings_file.parent().unwrap();
        settings.game_root = game_root.display().to_string();
        game.user_data = vec![crate::games::UserData {
            id: "documents".into(),
            kind: crate::domain::user_data::Kind::Settings,
            path: "{game}".into(),
            path_linux: None,
            mode: crate::domain::user_data::Mode::Redirect,
            default: crate::domain::user_data::Isolation::Isolated,
            inherit: true,
        }];

        let all = paths(&data, &game, &settings);
        assert!(
            !all.contains(&settings_file.display().to_string()),
            "the legacy list must not also capture the order file: {all:?}"
        );
    }

    #[test]
    fn a_missing_settings_folder_adds_no_settings_file() {
        let (data, mut game, mut settings, _, _) = fixture("no-settings");
        game.profile_data.clear();
        settings.profile_data.clear();
        settings.settings_dir.clear();
        assert!(paths(&data, &game, &settings).is_empty());
    }

    #[test]
    fn switching_profiles_keeps_settings_and_save_folders_apart() {
        let (data, game, settings, settings_file, saves) = fixture("switch");
        switch(&data, &game, &settings, "p1").unwrap();
        assert_eq!(fs::read_to_string(&settings_file).unwrap(), "one");
        assert_eq!(fs::read_to_string(saves.join("slot.sav")).unwrap(), "one");

        fs::write(&settings_file, "one changed").unwrap();
        fs::write(saves.join("slot.sav"), "one changed").unwrap();
        switch(&data, &game, &settings, "p2").unwrap();
        assert!(!settings_file.exists(), "the second profile starts empty");
        assert!(!saves.exists(), "the second profile starts empty");

        fs::create_dir_all(&saves).unwrap();
        fs::write(&settings_file, "two").unwrap();
        fs::write(saves.join("slot.sav"), "two").unwrap();
        switch(&data, &game, &settings, "p1").unwrap();
        assert_eq!(fs::read_to_string(&settings_file).unwrap(), "one changed");
        assert_eq!(
            fs::read_to_string(saves.join("slot.sav")).unwrap(),
            "one changed"
        );
    }

    #[test]
    fn capture_current_saves_changes_after_the_game_exits() {
        let (data, game, settings, settings_file, saves) = fixture("capture");
        switch(&data, &game, &settings, "p1").unwrap();
        fs::write(&settings_file, "after game").unwrap();
        fs::write(saves.join("slot.sav"), "after game").unwrap();
        capture_current(&data, &game, &settings).unwrap();

        switch(&data, &game, &settings, "p2").unwrap();
        switch(&data, &game, &settings, "p1").unwrap();
        assert_eq!(fs::read_to_string(&settings_file).unwrap(), "after game");
        assert_eq!(
            fs::read_to_string(saves.join("slot.sav")).unwrap(),
            "after game"
        );
    }
}
