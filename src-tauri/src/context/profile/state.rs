//! The per-profile state materialized outside the game folder. The legacy copy list (`profile_data`) and the
//! definition-driven user-data sets (`user_data`) are two stores with one lifecycle, so callers switch, save and
//! restore both together instead of remembering the second call.

use super::{data, user_data};
use crate::context::{Data, Settings};
use crate::domain::profiles::Profile;
use crate::error::AppError;
use crate::games::GameDef;
use std::path::{Path, PathBuf};

/// The overwrite layer of a profile: the files the game and its tools wrote through the overlay.
pub fn overwrite(state: &Path) -> PathBuf {
    state.join("overwrite")
}

/// The legacy per-profile data store (`state/<profile>/data`).
pub fn legacy_data(state: &Path) -> PathBuf {
    state.join("data")
}

/// Which of the two stores a profile owns, that is, whose files are in the real folders now.
#[derive(Clone, Copy)]
pub struct Stores {
    pub data: bool,
    pub user_data: bool,
}

/// The stores `profile` owns.
pub fn owned_by(data: &Data, profile: &str) -> Result<Stores, AppError> {
    Ok(Stores {
        data: data::owner(data)?.as_deref() == Some(profile),
        user_data: user_data::owner(data)?.as_deref() == Some(profile),
    })
}

/// Makes `profile` the materialized profile for both stores.
pub fn switch(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
) -> Result<(), AppError> {
    let both = Stores {
        data: true,
        user_data: true,
    };
    switch_stores(data, game, settings, profile, both)
}

/// Hands the stores `from` owns over to `to`, for a profile that is being deleted. A store `from` does not own is left
/// alone.
pub fn hand_over(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    from: &str,
    to: &Profile,
) -> Result<(), AppError> {
    let stores = owned_by(data, from)?;
    switch_stores(data, game, settings, to, stores)
}

/// Each store journals its own switch; when the user-data switch fails, the legacy store is switched back to its
/// previous owner, so a partial switch does not leave the two stores on different profiles. A rollback that fails
/// leaves that store's journal for the next switch, which recovers it first.
fn switch_stores(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
    stores: Stores,
) -> Result<(), AppError> {
    let previous = data::owner(data)?;
    if stores.data {
        data::switch(data, game, settings, &profile.id)?;
    }
    if stores.user_data
        && let Err(error) = user_data::switch(data, game, settings, profile)
    {
        if stores.data
            && let Some(previous) = previous.filter(|previous| previous != &profile.id)
            && let Err(rollback) = data::switch(data, game, settings, &previous)
        {
            let target = &profile.id;
            crate::infra::log::warn(format!(
                "could not return the data store to {previous} after switching to {target} failed: {rollback}"
            ));
        }
        return Err(error);
    }
    Ok(())
}

/// Saves the materialized profile's data after the game exits.
pub fn capture_current(data: &Data, game: &GameDef, settings: &Settings) -> Result<(), AppError> {
    data::capture_current(data, game, settings)?;
    user_data::capture_current(data, game, settings)
}

/// Captures the settings file after a deployment wrote it. A settings file that belongs to a user-data set is left
/// to that set.
pub fn capture_settings(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
) -> Result<(), AppError> {
    if !user_data::capture_settings(data, game, settings, profile)? {
        data::capture_settings(data, game, settings, &profile.id)?;
    }
    Ok(())
}

/// Restores the profile's stored data, for the undo path.
pub fn restore_profile(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    remove_missing: bool,
) -> Result<(), AppError> {
    data::restore_profile(data, game, settings, profile, remove_missing)?;
    user_data::restore_profile(data, game, settings, profile, remove_missing)
}

#[cfg(test)]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
mod tests {
    use super::*;
    use crate::domain::profiles::Profiles;
    use crate::domain::user_data::Isolation;
    use crate::infra::fs::fail_after;
    use crate::test_support::scratch;
    use std::fs;
    use std::path::PathBuf;

    struct Fixture {
        data: Data,
        game: GameDef,
        settings: Settings,
        profiles: Profiles,
        /// The id the second profile got, after the default `p1`.
        second: String,
        real_data: PathBuf,
        real_user: PathBuf,
    }

    fn fixture(name: &str) -> Fixture {
        let base = scratch!(name);
        let game_root = base.join("game");
        let settings_dir = base.join("settings");
        let real_user = settings_dir.join("user");
        let real_data = base.join("real/save.txt");
        fs::create_dir_all(&game_root).unwrap();
        fs::create_dir_all(&real_user).unwrap();
        fs::create_dir_all(real_data.parent().unwrap()).unwrap();
        fs::write(&real_data, "data-one").unwrap();
        fs::write(real_user.join("save.txt"), "user-one").unwrap();

        let game = GameDef::parse(
            r#"
schema = 1
id = "state-switch"
name = "State Switch"
[detect]
markers = ["game.exe"]
[[executables]]
name = "Game"
path = "game.exe"
[deploy]
[[user_data]]
id = "saves"
path = "{settings}/user"
mode = "copy"
default = "isolated"
"#,
        )
        .unwrap();
        let settings = Settings {
            game_root: game_root.display().to_string(),
            settings_dir: settings_dir.display().to_string(),
            profile_data: vec![real_data.display().to_string()],
            ..Settings::default()
        };
        let mut profiles = Profiles::with_default(Vec::new());
        let second = profiles.create("Second", None).unwrap();
        for id in ["p1", second.as_str()] {
            profiles
                .get_mut(id)
                .unwrap()
                .set_user_data_isolation("saves", Isolation::Isolated);
        }
        let data = Data::new(base.join("data-root"));
        data.save_profiles(&profiles).unwrap();
        Fixture {
            data,
            game,
            settings,
            profiles,
            second,
            real_data,
            real_user,
        }
    }

    impl Fixture {
        fn switch_to(&self, profile: &str) -> Result<(), AppError> {
            super::switch(
                &self.data,
                &self.game,
                &self.settings,
                self.profiles.get(profile)?,
            )
        }

        fn journal(&self, store: &str) -> PathBuf {
            self.data
                .dir
                .join("state")
                .join(format!("profile-switch-{store}.json"))
        }
    }

    #[test]
    fn a_failed_user_data_store_returns_the_legacy_store_to_its_previous_owner() {
        let f = fixture("state-switch-rollback");
        f.switch_to("p1").unwrap();
        // Give p2 its own copies, so restoring it copies real files instead of only removing them.
        f.switch_to(&f.second).unwrap();
        fs::write(&f.real_data, "data-two").unwrap();
        fs::create_dir_all(&f.real_user).unwrap();
        fs::write(f.real_user.join("save.txt"), "user-two").unwrap();
        f.switch_to("p1").unwrap();
        assert_eq!(fs::read_to_string(&f.real_data).unwrap(), "data-one");
        assert_eq!(
            fs::read_to_string(f.real_user.join("save.txt")).unwrap(),
            "user-one"
        );

        // The user-data store copies two folders; the second copy fails after the legacy store already switched.
        fail_after(2);
        let error = f.switch_to(&f.second).unwrap_err();
        fail_after(0);
        assert!(error.to_string().contains("injected"), "{error}");
        assert_eq!(
            crate::context::profile::data::owner(&f.data)
                .unwrap()
                .as_deref(),
            Some("p1")
        );
        assert_eq!(
            crate::context::profile::user_data::owner(&f.data)
                .unwrap()
                .as_deref(),
            Some("p1")
        );
        assert_eq!(fs::read_to_string(&f.real_data).unwrap(), "data-one");
        assert_eq!(
            fs::read_to_string(f.real_user.join("save.txt")).unwrap(),
            "user-one"
        );
        assert!(!f.journal("data").exists());
        assert!(!f.journal("user-data").exists());

        f.switch_to(&f.second).unwrap();
        assert_eq!(fs::read_to_string(&f.real_data).unwrap(), "data-two");
        assert_eq!(
            fs::read_to_string(f.real_user.join("save.txt")).unwrap(),
            "user-two"
        );
        assert_eq!(
            crate::context::profile::data::owner(&f.data)
                .unwrap()
                .as_deref(),
            Some(f.second.as_str())
        );
        assert_eq!(
            crate::context::profile::user_data::owner(&f.data)
                .unwrap()
                .as_deref(),
            Some(f.second.as_str())
        );
    }
}
