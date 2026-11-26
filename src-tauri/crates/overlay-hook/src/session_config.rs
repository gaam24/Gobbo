//! The mount set of one session: the game folder plus the profile's user-data redirects. Gobbo writes it to a JSON
//! file before the launch; the old three environment variables remain the fallback for `gobbo run` and for tests.

use gobbo_overlay_core::{ENV_MOUNTS, Layers, MountConfig, MountSpec};

use crate::paths::long_path;

/// Reads the mount set from `GOBBO_OVERLAY_MOUNTS`, or builds the single game mount from the legacy variables.
pub(crate) fn load() -> Option<MountConfig> {
    if let Some(path) = std::env::var_os(ENV_MOUNTS) {
        let text = std::fs::read_to_string(&path).ok()?;
        let config: MountConfig = serde_json::from_str(&text).ok()?;
        return long_names(config.validate().ok()?);
    }

    let layers = Layers::from_env(|name| std::env::var(name).ok())?;
    let config = MountConfig::try_new(
        MountConfig::VERSION,
        vec![MountSpec {
            id: MountSpec::GAME_ID.to_string(),
            guest: layers.root.clone(),
            root: layers.root.clone(),
            staging: layers.staging.clone(),
            upper: layers.upper.clone(),
        }],
    )
    .ok()?;
    long_names(config)
}

/// Stores mount paths in their long form, the form the handle APIs answer. A program may spell a folder with 8.3 names:
/// handle APIs answer long, while the loader reports a module path in the spelling the process used. The hook compares
/// both spellings against these prefixes and always answers with the long guest path.
fn long_names(config: MountConfig) -> Option<MountConfig> {
    let mounts = config
        .mounts()
        .iter()
        .map(|mount| MountSpec {
            id: mount.id.clone(),
            guest: long_path(&mount.guest),
            root: long_path(&mount.root),
            staging: mount.staging.as_deref().map(long_path),
            upper: long_path(&mount.upper),
        })
        .collect();
    // A long name is still absolute, but the set is checked again before the hook can use it.
    MountConfig::try_new(MountConfig::VERSION, mounts).ok()
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
        let config = MountConfig::try_new(
            MountConfig::VERSION,
            vec![spec(MountSpec::GAME_ID), spec("documents")],
        )
        .unwrap();
        let long = long_names(config).unwrap();
        assert_eq!(
            long.mounts()
                .iter()
                .map(|mount| mount.id.as_str())
                .collect::<Vec<_>>(),
            [MountSpec::GAME_ID, "documents"]
        );
        // Validation lives in `gobbo_overlay_core`, so a bad set is refused before the hook can use it.
        assert!(
            MountConfig::try_new(
                MountConfig::VERSION,
                vec![spec("documents"), spec(MountSpec::GAME_ID)],
            )
            .is_err()
        );
        assert!(
            MountConfig::try_new(
                MountConfig::VERSION,
                vec![spec(MountSpec::GAME_ID), spec(MountSpec::GAME_ID)],
            )
            .is_err()
        );
    }

    #[test]
    fn a_relative_path_or_an_unknown_version_is_refused() {
        assert!(
            MountConfig::try_new(MountConfig::VERSION, vec![spec(MountSpec::GAME_ID)],).is_ok()
        );
        assert!(MountConfig::try_new(99, vec![spec(MountSpec::GAME_ID)]).is_err());
        let mut relative = spec(MountSpec::GAME_ID);
        relative.upper = PathBuf::from("relative");
        assert!(MountConfig::try_new(MountConfig::VERSION, vec![relative]).is_err());
    }
}
