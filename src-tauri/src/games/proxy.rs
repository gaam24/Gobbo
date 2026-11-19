//! The DLLs that a game loads from its program folder and that a mod can provide: a mod loader's proxy such as
//! `winhttp.dll` or `version.dll`. Windows loads such a file from the program folder before its own copy, but Wine uses
//! its own copy unless a DLL override asks for the file. So under Proton Gobbo asks for each proxy that is there.

use crate::games::GameDef;

/// The loader DLLs that mod loaders and mods put next to a game's program. A game with another proxy lists it in its
/// definition, as a root file or as a mirrored `.dll` (`bin/x64/version.dll`).
const LOADER_DLLS: &[&str] = &[
    "winhttp.dll",
    "winmm.dll",
    "version.dll",
    "dinput8.dll",
    "dsound.dll",
    "xinput1_3.dll",
    "xinput1_4.dll",
    "xinput9_1_0.dll",
    "d3d9.dll",
    "d3d11.dll",
    "dxgi.dll",
    "ddraw.dll",
];

/// The folder of a program, relative to the game folder: `bin/x64` for `bin/x64/game.exe`, empty for a program at the
/// top of the game folder.
pub fn program_dir(program: &str) -> &str {
    program.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The DLLs of a game that Proton loads native for a launch: the loader list and the definition's own proxies, each one
/// only when `has_file` finds it at `program_dir/<name>`. `has_file` looks at the launched view of the game (the overlay
/// layers, or the game folder) and takes a path relative to the game folder with `/` separators.
pub fn native_dlls(
    game: &GameDef,
    program_dir: &str,
    has_file: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let listed = LOADER_DLLS.iter().map(|name| (*name).to_string());
    for name in listed.chain(game.deploy.proxy_dlls()) {
        if !candidates
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&name))
        {
            candidates.push(name);
        }
    }
    candidates
        .into_iter()
        .filter(|name| {
            let rel = if program_dir.is_empty() {
                name.clone()
            } else {
                format!("{program_dir}/{name}")
            };
            has_file(&rel)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;

    #[test]
    fn the_program_folder_is_the_part_before_the_file_name() {
        assert_eq!(program_dir("bin/x64/Game.exe"), "bin/x64");
        assert_eq!(program_dir("Game.exe"), "");
    }

    #[test]
    fn a_root_file_of_the_definition_counts_at_the_top_of_the_game() {
        let mut game = test_games::load("plugin");
        game.deploy.root_files.push("Hook.dll".to_string());
        let native = native_dlls(&game, "", |rel| rel == "Hook.dll");
        assert_eq!(native, ["Hook.dll"]);
    }

    #[test]
    fn nothing_is_native_when_no_loader_is_there() {
        let game = test_games::load("plugin");
        assert!(native_dlls(&game, "bin/x64", |_| false).is_empty());
    }
}
