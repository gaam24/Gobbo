//! Command-line mode: `gobbo run <game> -- <program>` starts a game without a window, so Steam can call it from its
//! launch options.

use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::infra::storage;

/// Returns the exit code when a program was started this way. `None` means a normal app start with a window.
pub fn run_cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(String::as_str) != Some("run") {
        return None;
    }

    Some(match run_command(&args[1..]) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("gobbo: {error}");
            2
        }
    })
}

fn run_command(args: &[String]) -> Result<i32, AppError> {
    let usage = || AppError::user("cli.usage");
    let [game, separator, program, rest @ ..] = args else {
        return Err(usage());
    };

    if separator != "--" {
        return Err(usage());
    }

    let exe_dir = storage::exe_dir()?;

    match definition_dirs_from(&exe_dir) {
        Some((games, loaders)) => crate::games::set_definition_dirs(games, loaders),
        None if Path::new(crate::games::games_dir()).is_dir() => {}
        None => {
            return Err(AppError::new("cli.definitions_missing").with("path", exe_dir.display()));
        }
    }

    let root = storage::root_for_cli()?;
    crate::games::set_user_definition_dirs(root.clone());
    crate::games::set_catalog_definition_dirs(root.clone());
    crate::infra::log::init(&root);
    crate::infra::log::install_panic_hook();
    crate::games::drop_stale_catalog();
    crate::infra::log::info(format!(
        "Gobbo {} run starting in {}",
        env!("CARGO_PKG_VERSION"),
        root.display()
    ));
    let data = crate::context::open(root, &crate::providers::archive_name_from)?;
    crate::launch::tools::run_game(&data, game, program, rest)
}

/// Finds the `games/` and `loaders/` folders of a packaged app next to `exe`. The window uses Tauri's `resource_dir`;
/// the CLI has no `AppHandle`, so it checks the layouts Tauri produces: beside the executable, in `resources/`, and in
/// the Linux `lib/gobbo` and `share/gobbo` folders. `None` means the development tree may still be the source.
fn definition_dirs_from(exe: &Path) -> Option<(PathBuf, PathBuf)> {
    let candidates = [
        exe.to_path_buf(),
        exe.join("resources"),
        exe.join("../lib/gobbo"),
        exe.join("../lib/dev.gobbo.modsorganizer"),
        exe.join("../share/gobbo"),
    ];
    candidates.into_iter().find_map(|base| {
        let games = super::bundled_resource(&base, "games")?;
        let loaders =
            super::bundled_resource(&base, "loaders").unwrap_or_else(|| base.join("loaders"));
        Some((games, loaders))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn the_definition_folders_are_found_beside_the_executable_or_in_resources() {
        let base = scratch!("cli-definitions");
        fs::create_dir_all(base.join("games")).unwrap();
        fs::create_dir_all(base.join("loaders")).unwrap();
        assert_eq!(
            definition_dirs_from(&base),
            Some((base.join("games"), base.join("loaders")))
        );

        let nested = base.join("nested");
        fs::create_dir_all(nested.join("games/games")).unwrap();
        fs::create_dir_all(nested.join("loaders/loaders")).unwrap();
        assert_eq!(
            definition_dirs_from(&nested),
            Some((nested.join("games/games"), nested.join("loaders/loaders")))
        );

        let resources = base.join("resources");
        fs::create_dir_all(resources.join("games")).unwrap();
        fs::create_dir_all(resources.join("loaders")).unwrap();
        assert_eq!(
            definition_dirs_from(&base),
            Some((base.join("games"), base.join("loaders"))),
            "the executable folder wins over resources"
        );
        assert_eq!(
            definition_dirs_from(&resources),
            Some((resources.join("games"), resources.join("loaders")))
        );
    }

    #[test]
    fn a_missing_definition_folder_is_reported() {
        let base = scratch!("cli-no-definitions");
        assert_eq!(definition_dirs_from(&base), None);
    }
}
