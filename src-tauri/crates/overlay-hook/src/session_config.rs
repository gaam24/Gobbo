//! The mount set of one session: the game folder plus the profile's user-data redirects. Gobbo writes it to a JSON
//! file before the launch; the old three environment variables remain the fallback for `gobbo run` and for tests.

use std::io::Read as _;
use std::path::Path;

use gobbo_overlay_core::{ENV_MOUNTS, Layers, MountConfig, MountSpec};

use crate::paths::long_path;

/// The largest mount file the hook reads. The file name comes from the environment, so a runaway file must not exhaust
/// the game's memory before validation can refuse it. Eight mounts of four maximum-length paths fit well below it.
const MAX_CONFIG_BYTES: u64 = 8 * 1024 * 1024;

/// Reads the mount set from `GOBBO_OVERLAY_MOUNTS`, or builds the single game mount from the legacy variables.
pub(crate) fn load() -> Option<MountConfig> {
    if let Some(path) = std::env::var_os(ENV_MOUNTS) {
        let config = read_config(Path::new(&path))?;
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

/// Reads one mount file with a hard size limit, then parses it. Reading one byte beyond the limit is enough to refuse a
/// larger file without letting it into the allocator.
fn read_config(path: &Path) -> Option<MountConfig> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if u64::try_from(text.len()).ok()? > MAX_CONFIG_BYTES {
        return None;
    }
    serde_json::from_str(&text).ok()
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

    #[test]
    fn a_valid_mount_file_is_read() {
        let path =
            std::env::temp_dir().join(format!("gobbo-mounts-valid-{}.json", std::process::id()));
        let config =
            MountConfig::try_new(MountConfig::VERSION, vec![spec(MountSpec::GAME_ID)]).unwrap();
        std::fs::write(&path, serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(read_config(&path), Some(config));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_mount_file_beyond_the_read_limit_is_refused() {
        let path = std::env::temp_dir().join(format!(
            "gobbo-mounts-too-large-{}.json",
            std::process::id()
        ));
        let text = format!(
            "{{\"version\":1,\"mounts\":[],\"padding\":\"{}\"}}",
            "x".repeat(MAX_CONFIG_BYTES as usize + 1)
        );
        std::fs::write(&path, text).unwrap();
        assert!(read_config(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
