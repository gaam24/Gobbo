//! The mount set of one session: the game folder plus the profile's user-data redirects. Gobbo writes it to a JSON
//! file before the launch; the old three environment variables remain the fallback for `gobbo run` and for tests.

use gobbo_overlay_core::{ENV_MOUNTS, Layers, MountConfig, MountSpec};

use crate::paths::long_path;

/// The most mounts a session accepts: the game and seven user-data sets. The pass-through path checks every mount, so
/// the list stays small on purpose.
const MAX_MOUNTS: usize = 8;

/// Reads the mount set from `GOBBO_OVERLAY_MOUNTS`, or builds the single game mount from the legacy variables.
pub(crate) fn load() -> Option<MountConfig> {
    if let Some(path) = std::env::var_os(ENV_MOUNTS) {
        let text = std::fs::read_to_string(&path).ok()?;
        let config: MountConfig = serde_json::from_str(&text).ok()?;
        return valid(config).map(long_names);
    }

    let layers = Layers::from_env(|name| std::env::var(name).ok())?;
    Some(long_names(MountConfig {
        version: MountConfig::VERSION,
        mounts: vec![MountSpec {
            id: MountSpec::GAME_ID.to_string(),
            guest: layers.root.clone(),
            root: layers.root.clone(),
            staging: layers.staging.clone(),
            upper: layers.upper.clone(),
        }],
    }))
}

/// Stores mount paths in their long form, the form the handle APIs answer. A program may spell a folder with 8.3 names:
/// handle APIs answer long, while the loader reports a module path in the spelling the process used. The hook compares
/// both spellings against these prefixes and always answers with the long guest path.
fn long_names(config: MountConfig) -> MountConfig {
    MountConfig {
        version: config.version,
        mounts: config
            .mounts
            .into_iter()
            .map(|mount| MountSpec {
                id: mount.id,
                guest: long_path(&mount.guest),
                root: long_path(&mount.root),
                staging: mount.staging.as_deref().map(long_path),
                upper: long_path(&mount.upper),
            })
            .collect(),
    }
}

/// Checks the shape the hook relies on: a known version, the game first, a bounded number of absolute paths and unique
/// ids. A bad set fails the launch instead of running the game against the real folder.
fn valid(config: MountConfig) -> Option<MountConfig> {
    if config.version != MountConfig::VERSION || config.mounts.len() > MAX_MOUNTS {
        return None;
    }

    config.game()?;
    let mut ids = std::collections::BTreeSet::new();

    for mount in &config.mounts {
        if mount.id.is_empty()
            || !ids.insert(mount.id.as_str())
            || !mount.guest.is_absolute()
            || !mount.root.is_absolute()
            || !mount.upper.is_absolute()
            || mount
                .staging
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
        {
            return None;
        }
    }

    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec(id: &str) -> MountSpec {
        MountSpec {
            id: id.to_string(),
            guest: PathBuf::from(r"C:\Game"),
            root: PathBuf::from(r"C:\Game"),
            staging: None,
            upper: PathBuf::from(r"C:\State\upper"),
        }
    }

    #[test]
    fn the_game_must_be_first_and_the_ids_are_unique() {
        let mut config = MountConfig {
            version: MountConfig::VERSION,
            mounts: vec![spec(MountSpec::GAME_ID), spec("documents")],
        };
        assert!(valid(config.clone()).is_some());
        config.mounts[1].id = MountSpec::GAME_ID.into();
        assert!(valid(config).is_none());
    }

    #[test]
    fn a_relative_path_or_an_unknown_version_is_refused() {
        let mut config = MountConfig {
            version: MountConfig::VERSION,
            mounts: vec![spec(MountSpec::GAME_ID)],
        };
        assert!(valid(config.clone()).is_some());
        config.version = 99;
        assert!(valid(config).is_none());
        let mut config = MountConfig {
            version: MountConfig::VERSION,
            mounts: vec![spec(MountSpec::GAME_ID)],
        };
        config.mounts[0].upper = PathBuf::from("relative");
        assert!(valid(config).is_none());
    }
}
