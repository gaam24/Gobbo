//! Starting the game needs the game folder. The folder with `mods.settings` is needed only for deployment.

use crate::context::Settings;
use crate::context::is_dir;
use crate::error::{AppError, game_folder_missing};
use crate::games::Executable;
use crate::games::GameDef;

/// Checks that the game folder exists before a launch; the deployment-only folders are not required.
pub fn check_start(settings: &Settings) -> Result<(), AppError> {
    if !is_dir(&settings.game_root) {
        return Err(game_folder_missing());
    }

    Ok(())
}

/// The error when the program file to start is not on the disk.
pub fn executable_missing(path: impl ToString) -> AppError {
    AppError::user("launch.exe_missing").with("path", path)
}

/// The executable the user named, or an error when the definition has no such file.
fn named_executable<'a>(game: &'a GameDef, name: &str) -> Result<&'a Executable, AppError> {
    game.executables
        .iter()
        .find(|exe| exe.name == name)
        .ok_or_else(|| AppError::user("games.executable_missing").with("name", name))
}

/// The default executable: the first one the definition lists.
pub fn default_executable(game: &GameDef) -> Result<&Executable, AppError> {
    game.default_executable()
        .ok_or_else(|| AppError::unexpected("game definition has no executable"))
}

/// Launch file: by name, or the default when there is no name.
pub fn pick_executable<'a>(
    game: &'a GameDef,
    name: Option<&str>,
) -> Result<&'a Executable, AppError> {
    name.filter(|name| !name.is_empty()).map_or_else(
        || default_executable(game),
        |name| named_executable(game, name),
    )
}

/// Arguments for the game: the executable's arguments from the definition, then the user's arguments from the settings.
/// The user goes last so they can override a default.
pub fn launch_args(executable: &Executable, settings: &Settings) -> Vec<String> {
    executable
        .args
        .iter()
        .cloned()
        .chain(crate::domain::arguments::parse_args(&settings.launch_args))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;

    #[test]
    fn predefined_arguments_come_before_the_user_arguments() {
        let game = test_games::external();
        let executable = default_executable(&game).unwrap();
        let settings = Settings {
            launch_args: "-logfile test.log".to_string(),
            ..Settings::default()
        };

        assert_eq!(
            launch_args(executable, &settings),
            ["-flag", "-logfile", "test.log"]
        );
    }

    #[test]
    fn a_game_without_predefined_arguments_keeps_the_user_arguments() {
        let game = test_games::slot();
        let executable = default_executable(&game).unwrap();
        let settings = Settings {
            launch_args: "--windowed".to_string(),
            ..Settings::default()
        };

        assert_eq!(launch_args(executable, &settings), ["--windowed"]);
    }

    #[test]
    fn the_game_folder_must_exist_to_start() {
        let missing = Settings {
            game_root: "no-such-game-folder".to_string(),
            ..Settings::default()
        };
        assert!(check_start(&missing).is_err());

        let base = crate::test_support::scratch_in("executable", "game-folder");
        let present = Settings {
            game_root: base.display().to_string(),
            ..Settings::default()
        };
        assert!(check_start(&present).is_ok());
    }

    #[test]
    fn the_platform_argument_selects_the_native_program() {
        let executable = Executable {
            name: "Game".to_string(),
            path: "Game.exe".to_string(),
            path_linux: Some("Game.x86_64".to_string()),
            args: Vec::new(),
        };
        assert_eq!(executable.path_for_platform(true), "Game.exe");
        assert_eq!(executable.path_for_platform(false), "Game.x86_64");

        let fallback = Executable {
            name: "Game".to_string(),
            path: "Game.exe".to_string(),
            path_linux: Some("  ".to_string()),
            args: Vec::new(),
        };
        assert_eq!(fallback.path_for_platform(true), "Game.exe");
        assert_eq!(fallback.path_for_platform(false), "Game.exe");
    }

    #[test]
    fn an_executable_is_found_by_its_name_and_an_empty_name_is_the_default() {
        let game = test_games::slot();
        let first = &game.executables[0];

        assert_eq!(
            named_executable(&game, &first.name).unwrap().name,
            first.name
        );
        assert!(named_executable(&game, "no such executable").is_err());
        assert_eq!(pick_executable(&game, Some("")).unwrap().name, first.name);
        assert_eq!(pick_executable(&game, None).unwrap().name, first.name);
    }
}
