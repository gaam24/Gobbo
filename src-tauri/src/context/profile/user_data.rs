//! Per-profile user data outside the game folder: saves, settings and similar folders declared by `[[user_data]]` in
//! `game.toml`. A set is either shared, redirected through the overlay, or copied around a launch when the platform
//! cannot redirect it. The copy path is the fallback for a platform without the overlay redirect.
//!
//! The module owns the sets and the paths; the owner file and the switch order are shared with the legacy copy list
//! in `store.rs`. Redirect sets are never materialized: their writes already live in the profile's upper layer.

use std::fs;
use std::path::{Path, PathBuf};

use super::store::{self, Entry, OwnerFile, Store};
use crate::context::target::{path_context, settings_file};
use crate::context::{Data, Settings, load_profiles};
use crate::domain::pathkey;
use crate::domain::profiles::Profile;
use crate::domain::user_data::{Mode, Set};
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::fs::{WalkErrors, walk};

const OWNER: OwnerFile = OwnerFile {
    name: "user_data_owner.json",
    invalid_code: "user_data.owner_invalid",
    journal: "profile-switch-user-data.json",
};
const DATA_DIR: &str = "userdata";

/// The whiteout snapshot next to a redirect's upper layer. It names paths the game deleted from the lower layers.
pub fn whiteouts(upper: &Path) -> PathBuf {
    upper.with_extension("whiteouts")
}

/// The journal of whiteout changes next to a redirect's upper layer.
pub fn whiteouts_journal(upper: &Path) -> PathBuf {
    upper.with_extension("whiteouts.journal")
}

/// The marker that a non-inherited redirect was seeded from the real folder once.
pub fn seeded(upper: &Path) -> PathBuf {
    upper.parent().unwrap_or(upper).join("seeded")
}

/// The sets of one profile for the next launch or deployment. A missing profile choice means `shared`: a profile that
/// predates the definition must not start isolating data on its own.
pub fn sets(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
    state: &Path,
) -> Result<Vec<Set>, AppError> {
    if game.user_data.is_empty() || settings.game_root.trim().is_empty() {
        return Ok(Vec::new());
    }

    let context = path_context(data, game, settings);
    let mut sets = Vec::with_capacity(game.user_data.len());

    for definition in game.user_data_sets() {
        let real = context.resolve(definition.path_for_platform())?;

        if real.exists() && !real.is_dir() {
            return Err(AppError::user("user_data.not_a_folder")
                .with("id", &definition.id)
                .with("path", real.display()));
        }

        if pathkey::is_inside(Path::new(settings.game_root.trim()), &real) {
            return Err(AppError::user("user_data.inside_game").with("id", &definition.id));
        }

        let isolated = profile.user_data_isolation(&definition.id).is_isolated();
        // The Windows hook is the first implementation. Linux keeps the copy fallback until the overlayfs mounts are
        // ready; do not claim a redirect the platform does not perform.
        let mode = if definition.mode == Mode::Redirect
            && crate::platform::supports_user_data_redirect()
        {
            Mode::Redirect
        } else {
            Mode::Copy
        };
        let folder = state.join(DATA_DIR).join(&definition.id);
        let base = if definition.inherit {
            real.clone()
        } else {
            folder.join("base")
        };
        sets.push(Set {
            id: definition.id.clone(),
            kind: definition.kind,
            real,
            mode,
            isolated,
            inherit: definition.inherit,
            base,
            upper: folder.join("upper"),
            store: folder.join("copy"),
        });
    }

    check_overlaps(&sets)?;
    Ok(sets)
}

/// The sets for the status view. Unlike `sets`, a missing path variable gives an empty list instead of failing the
/// whole status: the interface can still show the game and the user can fix its settings.
/// Creates the folders a redirect needs. Called before a deployment or a launch, not by the status view.
pub fn prepare(sets: &[Set]) -> Result<(), AppError> {
    for set in sets.iter().filter(|set| set.redirects()) {
        fs::create_dir_all(&set.upper).map_err(io_at(&set.upper))?;

        if !set.inherit {
            fs::create_dir_all(&set.base).map_err(io_at(&set.base))?;
        }
        // The game discovers the folder by listing its parent. An empty real folder makes the redirect visible without
        // writing any file into it. Never create the parent: a wrong prefix user or a drive that is not mounted must
        // be a clear error, not a new empty folder in the wrong place.
        if let Some(parent) = set.real.parent()
            && !parent.is_dir()
        {
            return Err(AppError::user("user_data.parent_missing").with("path", parent.display()));
        }

        fs::create_dir_all(&set.real).map_err(io_at(&set.real))?;
    }

    Ok(())
}

/// Gives a new isolated redirect its own copy of the existing folder once. The real folder stays the read-only source;
/// later launches read and write only the profile's upper layer. A marker makes this a one-time action, so a file the
/// user deletes in the game is not silently brought back from the real folder.
fn seed_redirect(set: &Set) -> Result<(), AppError> {
    if !set.redirects() || set.inherit || !set.real.is_dir() {
        return Ok(());
    }

    let Some(folder) = set.upper.parent() else {
        return Ok(());
    };
    let marker = folder.join("seeded");

    if marker.exists() {
        return Ok(());
    }

    fs::create_dir_all(&set.upper).map_err(io_at(&set.upper))?;
    walk(
        &set.real,
        WalkErrors::Fail,
        &mut |entry, relative, is_dir| {
            let target = set.upper.join(relative);

            if is_dir {
                if !target.exists() {
                    fs::create_dir_all(&target)?;
                }
            } else if !target.exists() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::copy(entry.path(), &target)?;
            }

            Ok(())
        },
    )
    .map_err(io_at(&set.real))?;
    // The marker is written atomically, as the snapshot restore writes it, so a crash never leaves a half-written one.
    crate::infra::store::write_atomic(&marker, b"").map_err(AppError::from)
}

/// Where Gobbo writes a file of the game: the profile's upper layer for an isolated redirect, otherwise the real path.
pub fn write_path(sets: &[Set], path: &Path) -> PathBuf {
    sets.iter()
        .find_map(|set| set.write_path(path))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Whether a path belongs to an isolated redirect. Used by the settings writer to skip the legacy copy store.
pub fn is_redirected(sets: &[Set], path: &Path) -> bool {
    sets.iter()
        .any(|set| set.relative(path).is_some() && set.redirects())
}

/// Switches the profile whose copy-mode sets are materialized. Redirect sets are untouched; they are already per
/// profile. The first call seeds the target from the real data instead of removing it.
pub fn switch(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    target: &Profile,
) -> Result<(), AppError> {
    store::switch(
        &UserDataStore {
            game,
            settings,
            target: Some(target),
        },
        data,
        &target.id,
    )
}

/// Saves the current owner's copy-mode sets after a launch or before an undo.
pub fn capture_current(data: &Data, game: &GameDef, settings: &Settings) -> Result<(), AppError> {
    store::capture_current(
        &UserDataStore {
            game,
            settings,
            target: None,
        },
        data,
    )
}

/// Restores a profile's copy-mode sets, for the undo path. Redirect sets are left alone.
pub fn restore_profile(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    remove_missing: bool,
) -> Result<(), AppError> {
    store::restore_profile(
        &UserDataStore {
            game,
            settings,
            target: None,
        },
        data,
        profile,
        remove_missing,
    )
}

/// Captures the settings file after a deployment wrote it. Returns whether the file belongs to a user-data set; when it
/// does, the legacy `profile_data` store must not also copy it. A redirect set already holds the file in its upper
/// layer, so there is nothing to copy.
pub fn capture_settings(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
) -> Result<bool, AppError> {
    let Some(file) = settings_file(game, settings) else {
        return Ok(false);
    };
    let state = data.profile_state(&profile.id);
    let sets = sets(data, game, settings, profile, &state)?;
    let Some(set) = sets.iter().find(|set| set.relative(&file).is_some()) else {
        return Ok(false);
    };

    if set.redirects() || !set.isolated {
        return Ok(true);
    }

    let Some(relative) = set.relative(&file) else {
        return Ok(true);
    };
    store::capture(&[Entry {
        store: set.store.join(relative),
        real: file,
    }])?;
    Ok(true)
}

/// Whether the profile currently owns the copy stores. The owner file is shared by all copy-mode sets of a profile.
pub fn owner(data: &Data) -> Result<Option<String>, AppError> {
    OWNER.read(data)
}

/// The `user_data` sets as a store: only isolated copy sets take part, because a redirect already lives in the
/// profile's upper layer and a shared set is the real folder itself.
struct UserDataStore<'a> {
    game: &'a GameDef,
    settings: &'a Settings,
    /// The target profile when the caller already has it, so a switch does not read it again.
    target: Option<&'a Profile>,
}

impl UserDataStore<'_> {
    fn resolved(&self, data: &Data, profile: &str) -> Result<Vec<Set>, AppError> {
        let state = data.profile_state(profile);

        if let Some(target) = self.target.filter(|target| target.id == profile) {
            return sets(data, self.game, self.settings, target, &state);
        }

        let profiles = load_profiles(data)?;
        sets(
            data,
            self.game,
            self.settings,
            profiles.get(profile)?,
            &state,
        )
    }
}

impl Store for UserDataStore<'_> {
    fn owner(&self) -> OwnerFile {
        OWNER
    }

    fn active(&self) -> bool {
        !self.game.user_data.is_empty()
    }

    fn entries(&self, data: &Data, profile: &str) -> Result<Vec<Entry>, AppError> {
        Ok(entries_of(&self.resolved(data, profile)?))
    }

    fn prepare(&self, data: &Data, profile: &str) -> Result<(), AppError> {
        let sets = self.resolved(data, profile)?;
        prepare(&sets)?;

        for set in sets.iter().filter(|set| set.redirects()) {
            seed_redirect(set)?;
        }

        Ok(())
    }

    fn exists(&self, data: &Data, profile: &str) -> Result<bool, AppError> {
        if self.target.is_some_and(|target| target.id == profile) {
            return Ok(true);
        }

        Ok(load_profiles(data)?.get(profile).is_ok())
    }
}

/// The copy entries of the isolated copy sets. A redirect set already lives in the profile's upper layer and a shared
/// set is the real folder itself.
fn entries_of(sets: &[Set]) -> Vec<Entry> {
    sets.iter()
        .filter(|set| set.isolated && !set.redirects())
        .map(|set| Entry {
            real: set.real.clone(),
            store: set.store.clone(),
        })
        .collect()
}

/// One resolved set must not contain another. The definition check catches equal templates; this one catches different
/// templates that resolve to the same folder, for example `{documents}/Game` and `{home}/Documents/Game`.
fn check_overlaps(sets: &[Set]) -> Result<(), AppError> {
    for (index, first) in sets.iter().enumerate() {
        for second in &sets[index + 1..] {
            if first.relative(&second.real).is_some() || second.relative(&first.real).is_some() {
                return Err(AppError::user("user_data.overlap")
                    .with("first", &first.id)
                    .with("second", &second.id));
            }
        }
    }

    Ok(())
}

/// The copy loop of the module tests, which check one set without a profile switch.
#[cfg(test)]
fn capture_copy(sets: &[Set]) -> Result<(), AppError> {
    store::capture(&entries_of(sets))
}

/// The restore loop of the module tests, which check one set without a profile switch.
#[cfg(test)]
fn restore_copy(sets: &[Set], remove_missing: bool) -> Result<(), AppError> {
    store::restore(&entries_of(sets), remove_missing)
}

#[cfg(test)]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
mod tests {
    use super::*;
    use crate::context::Data;
    use crate::domain::user_data::{Isolation, Kind, Mode};
    use crate::test_support::scratch;
    use std::collections::BTreeMap;

    fn fixture(name: &str) -> (Data, GameDef, Settings, PathBuf) {
        let base = scratch!(name);
        let game_root = base.join("game");
        let settings_dir = base.join("settings");
        fs::create_dir_all(&game_root).unwrap();
        fs::create_dir_all(&settings_dir).unwrap();
        let mut game = crate::test_support::games::slot();
        game.user_data = vec![crate::games::UserData {
            id: "documents".into(),
            kind: Kind::Saves,
            path: "{settings}/user".into(),
            path_linux: None,
            mode: Mode::Redirect,
            default: Isolation::Isolated,
            inherit: true,
        }];
        let settings = Settings {
            game_root: game_root.display().to_string(),
            settings_dir: settings_dir.display().to_string(),
            ..Settings::default()
        };
        (
            Data::new(base.join("data")),
            game,
            settings,
            settings_dir.join("user"),
        )
    }

    fn profile(isolated: bool) -> Profile {
        let mut profile = Profile {
            id: "p1".into(),
            name: "Default".into(),
            mods: Vec::new(),
            separators: Vec::new(),
            groups: Vec::new(),
            deployed_mods: Vec::new(),
            deployed_at: None,
            winners: BTreeMap::default(),
            deployed_winners: BTreeMap::default(),
            user_data: BTreeMap::default(),
            pack: None,
            collections: Vec::new(),
            waiting: Vec::new(),
        };

        if isolated {
            profile.set_user_data_isolation("documents", Isolation::Isolated);
        }

        profile
    }

    // The Windows hook performs a redirect; the other platforms use the copy fallback for an isolated set, so only
    // Windows can resolve a set as `Mode::Redirect`.
    #[cfg(windows)]
    #[test]
    fn a_resolved_set_uses_the_profile_upper_layer_for_a_redirect() {
        let (data, game, settings, _) = fixture("user-data-set");
        let profile = profile(true);
        let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        assert_eq!(sets.len(), 1);
        assert!(sets[0].redirects());
        assert_eq!(sets[0].kind, Kind::Saves);
    }

    #[test]
    fn a_missing_choice_is_shared() {
        let (data, game, settings, _) = fixture("user-data-shared");
        let profile = profile(false);
        let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        assert!(!sets[0].isolated);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_linux_template_is_used_when_it_is_set() {
        let (data, mut game, settings, _) = fixture("user-data-linux");
        game.user_data[0].path_linux = Some("{settings}/linux-user".into());
        let profile = profile(false);
        let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        assert_eq!(
            sets[0].real,
            PathBuf::from(&settings.settings_dir).join("linux-user")
        );
    }

    #[test]
    fn a_copy_set_is_captured_and_restored() {
        let (data, game, settings, real) = fixture("user-data-copy");
        let profile = profile(true);
        let mut game = game;
        game.user_data[0].mode = Mode::Copy;
        let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("save.txt"), "one").unwrap();
        capture_copy(&sets).unwrap();
        fs::write(real.join("save.txt"), "two").unwrap();
        restore_copy(&sets, true).unwrap();
        assert_eq!(fs::read_to_string(real.join("save.txt")).unwrap(), "one");
    }

    #[cfg(windows)]
    #[test]
    fn a_non_inherited_redirect_is_seeded_once() {
        let (data, mut game, settings, real) = fixture("user-data-seed");
        game.user_data[0].inherit = false;
        let profile = profile(true);
        fs::create_dir_all(real.join("gamesaves")).unwrap();
        fs::write(real.join("gamesaves/a.sav"), "save").unwrap();
        let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        prepare(&sets).unwrap();
        seed_redirect(&sets[0]).unwrap();
        let seeded = sets[0].upper.join("gamesaves/a.sav");
        assert_eq!(fs::read_to_string(&seeded).unwrap(), "save");

        // The marker keeps a file the user deleted in the game from coming back on the next launch.
        fs::remove_file(&seeded).unwrap();
        seed_redirect(&sets[0]).unwrap();
        assert!(!seeded.exists());
    }

    #[test]
    fn the_seed_marker_is_written_whole_and_only_once() {
        let (data, mut game, settings, real) = fixture("user-data-seed-marker");
        game.user_data[0].inherit = false;
        let profile = profile(true);
        fs::create_dir_all(real.join("gamesaves")).unwrap();
        fs::write(real.join("gamesaves/a.sav"), "save").unwrap();
        let mut sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        // A redirect is a Windows mode; the marker is written the same way on every platform, so the test sets the mode.
        sets[0].mode = Mode::Redirect;

        seed_redirect(&sets[0]).unwrap();
        let marker = seeded(&sets[0].upper);
        assert!(marker.is_file());
        assert_eq!(fs::read(&marker).unwrap(), b"");
        // A second seed is a no-op: the marker is the proof that the seed already ran.
        fs::remove_file(sets[0].upper.join("gamesaves/a.sav")).unwrap();
        seed_redirect(&sets[0]).unwrap();
        assert!(!sets[0].upper.join("gamesaves/a.sav").exists());
    }

    #[cfg(windows)]
    #[test]
    fn the_order_file_of_a_redirect_is_written_to_the_profile_upper() {
        let base = scratch!("user-data-order");
        let root = base.join("game");
        let settings_dir = base.join("settings");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&settings_dir).unwrap();
        let text = r#"
schema = 1
id = "test"
name = "Test"
[detect]
markers = ["game.exe"]
[deploy]
[[deploy.slots]]
dir = "Mods"
[load_order]
format = "mods_settings"
file = "{settings}/mods.settings"
[[user_data]]
id = "documents"
path = "{settings}"
mode = "redirect"
default = "isolated"
"#;
        let game = GameDef::parse(text).unwrap();
        let profile = profile(true);
        let settings = Settings {
            game_root: root.display().to_string(),
            settings_dir: settings_dir.display().to_string(),
            ..Settings::default()
        };
        let data = Data::new(base.join("data"));
        let target =
            crate::deploy::target::target_for(&game, &settings, data.profile_state("p1"), &profile)
                .unwrap();
        assert_eq!(
            target.settings,
            Some(
                data.profile_state("p1")
                    .join("userdata/documents/upper/mods.settings")
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_redirect_prepares_the_upper_layer_and_the_real_folder() {
        let (data, game, settings, real) = fixture("user-data-prepare");
        let profile = profile(true);
        let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
        let _ = fs::remove_dir_all(&real);
        prepare(&sets).unwrap();
        assert!(real.is_dir());
        assert!(sets[0].upper.is_dir());
    }
}

#[cfg(test)]
mod prefix_tests {
    use super::*;
    use crate::domain::user_data::Kind;
    use crate::test_support::scratch;

    #[test]
    fn prepare_does_not_create_a_missing_parent_folder() {
        let base = scratch!("user-data-parent");
        let real = base.join("missing/child");
        let set = Set {
            id: "saves".into(),
            kind: Kind::Saves,
            real: real.clone(),
            mode: Mode::Redirect,
            isolated: true,
            inherit: true,
            base: real.clone(),
            upper: base.join("upper"),
            store: base.join("copy"),
        };
        let error = prepare(&[set]).unwrap_err();
        assert_eq!(error.code, "user_data.parent_missing");
        assert!(!real.parent().unwrap().exists(), "the parent stays absent");
    }
}
