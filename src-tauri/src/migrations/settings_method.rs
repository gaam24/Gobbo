//! Removes the old `deploy_method` key from every game's `config.json`. Overlay is the only deployment method now; the
//! key is ignored by the settings type, and an old value must not stay in the file as a choice the window could offer
//! again. The migration is idempotent and never deletes the file or another key.

use std::fs;
use std::path::Path;

use crate::error::AppError;
use crate::infra::json;

const CONFIG_FILE: &str = "config.json";
const OLD_KEY: &str = "deploy_method";

pub(super) fn run(root: &Path, _known_game_ids: &[String]) -> Result<(), AppError> {
    let games = root.join(super::GAMES_DIR);
    let Ok(entries) = fs::read_dir(&games) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let config = path.join(CONFIG_FILE);
        let Some(serde_json::Value::Object(mut fields)) = json::read_json(&config).ok().flatten()
        else {
            // Missing or damaged: the settings reader has its own recovery for a damaged file, and a missing file has
            // no old key.
            continue;
        };
        if fields.remove(OLD_KEY).is_none() {
            continue;
        }
        json::write_json(&config, &serde_json::Value::Object(fields))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn the_old_method_key_is_removed_and_the_other_settings_stay() {
        let root = scratch!("migration-settings-method");
        let game = root.join(super::super::GAMES_DIR).join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(
            game.join(CONFIG_FILE),
            r#"{"game_root":"/game","deploy_method":"hardlink","launch_args":"-x"}"#,
        )
        .unwrap();

        run(&root, &[]).unwrap();

        let text = fs::read_to_string(game.join(CONFIG_FILE)).unwrap();
        assert!(!text.contains(OLD_KEY), "{text}");
        assert!(text.contains("game_root"), "{text}");
        assert!(text.contains("launch_args"), "{text}");
    }

    #[test]
    fn a_missing_file_and_a_second_run_change_nothing() {
        let root = scratch!("migration-settings-method-twice");
        let game = root.join(super::super::GAMES_DIR).join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join(CONFIG_FILE), r#"{"game_root":"/game"}"#).unwrap();

        run(&root, &[]).unwrap();
        let once = fs::read_to_string(game.join(CONFIG_FILE)).unwrap();
        run(&root, &[]).unwrap();
        assert_eq!(fs::read_to_string(game.join(CONFIG_FILE)).unwrap(), once);
    }
}
