//! How an install starts on this computer. The files in the game folder decide, not the definition alone: the same game
//! can be a native Linux build from one store and a Windows build under Proton from another. Launch, tools, steps,
//! prerequisites and the deployment root all ask [`of`], so they never disagree.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use super::GameDef;
use super::prefix::effective_source;
use super::stores::Store;

/// How an install starts on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    /// Through Proton, in the Steam prefix of a Steam install.
    SteamProton,
    /// Through Proton, in the prefix of another store or one Gobbo owns: a Windows program that is not from Steam.
    OwnedProton,
    /// Directly, as a program of this system.
    Native,
}

/// The runtime of the install in `game_root`, whose saved source is `source`. An empty or unknown source resolves
/// through [`effective_source`], so an old install with a Steam app id still uses Steam's prefix.
pub fn of(game: &GameDef, game_root: &Path, source: &str) -> Runtime {
    of_for(
        crate::platform::is_windows(),
        game,
        game_root,
        effective_source(source, &game.detect.steam_app_ids),
    )
}

/// [`of`] with the platform given, so both sides can be tested on one machine. Windows always starts the program
/// directly. On Linux a native install starts directly wherever it comes from; a Windows program goes through Proton —
/// the Steam prefix for a Steam install, another prefix for everything else.
pub fn of_for(is_windows: bool, game: &GameDef, game_root: &Path, store: Store) -> Runtime {
    if is_windows || is_native_install(game, game_root) {
        return Runtime::Native;
    }

    if store == Store::Steam {
        Runtime::SteamProton
    } else {
        Runtime::OwnedProton
    }
}

/// Whether the folder holds a build of this system: the files of `linux_markers`, the default program's `path_linux`,
/// or a default program that is not a Windows program. When neither program is in the folder, a definition that names a
/// Linux program counts as native, so the launch reports the missing Linux file. A definition without programs has
/// nothing to start through Proton, so it counts as native.
fn is_native_install(game: &GameDef, game_root: &Path) -> bool {
    let linux = &game.detect.linux_markers;
    let linux_markers = !linux.is_empty()
        && if game.detect.any_marker {
            linux.iter().any(|marker| game_root.join(marker).exists())
        } else {
            linux.iter().all(|marker| game_root.join(marker).exists())
        };

    if linux_markers {
        return true;
    }

    let Some(exe) = game.default_executable() else {
        return true;
    };

    let linux = exe
        .path_linux
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty());

    if linux.is_some_and(|path| game_root.join(path).is_file()) {
        return true;
    }

    let windows = game_root.join(&exe.path);

    if !windows.is_file() && linux.is_some() {
        return true;
    }

    !is_windows_program(&windows)
}

/// Whether a file is a Windows program: a script or executable by extension, or a PE file (`MZ`) whatever its name.
/// A file that is not there is judged by its extension alone.
fn is_windows_program(path: &Path) -> bool {
    let by_extension = path.extension().is_some_and(|extension| {
        ["exe", "bat", "cmd", "msi"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    });
    by_extension || {
        let mut magic = [0_u8; 2];
        File::open(path)
            .and_then(|mut file| file.read_exact(&mut magic))
            .is_ok_and(|()| &magic == b"MZ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::{scratch, write};

    /// A fixture game with a Windows program and a native Linux program.
    fn two_builds() -> GameDef {
        let mut game = test_games::slot();
        game.detect.markers = vec!["Game.exe".into()];
        game.detect.linux_markers = vec!["Game.x86_64".into()];
        game.executables[0].path = "Game.exe".into();
        game.executables[0].path_linux = Some("Game.x86_64".into());
        game
    }

    #[test]
    fn windows_always_starts_a_program_directly() {
        let base = scratch!("runtime-windows");
        write(&base.join("Game.exe"), "MZ");
        for store in [
            Store::Steam,
            Store::Gog,
            Store::Epic,
            Store::Heroic,
            Store::Lutris,
            Store::Manual,
        ] {
            assert_eq!(
                of_for(true, &two_builds(), &base, store),
                Runtime::Native,
                "{store:?}"
            );
        }
    }

    /// The store of a Windows build decides which Proton prefix it uses, for every store Gobbo knows.
    #[test]
    fn every_store_sends_a_windows_build_to_the_right_proton_prefix() {
        let base = scratch!("runtime-every-store");
        write(&base.join("Game.exe"), "MZ");
        let mut game = two_builds();
        game.detect.linux_markers.clear();

        for &store in Store::ALL {
            let expected = if store == Store::Steam {
                Runtime::SteamProton
            } else {
                Runtime::OwnedProton
            };
            assert_eq!(
                of_for(false, &game, &base, store),
                expected,
                "{}",
                store.display_name()
            );
        }
    }

    #[test]
    fn a_windows_build_of_a_game_with_a_linux_program_still_goes_through_proton() {
        // The definition names a Linux program, but this folder only holds the Windows build.
        let base = scratch!("runtime-windows-build");
        write(&base.join("Game.exe"), "MZ");
        let game = two_builds();
        assert_eq!(
            of_for(false, &game, &base, Store::Steam),
            Runtime::SteamProton
        );
        assert_eq!(
            of_for(false, &game, &base, Store::Gog),
            Runtime::OwnedProton
        );
    }

    #[test]
    fn a_native_build_starts_directly_wherever_it_comes_from() {
        let base = scratch!("runtime-native-build");
        write(&base.join("Game.x86_64"), "\x7fELF");
        let game = two_builds();
        for store in [Store::Steam, Store::Gog, Store::Heroic, Store::Manual] {
            assert_eq!(
                of_for(false, &game, &base, store),
                Runtime::Native,
                "{store:?}"
            );
        }
    }

    #[test]
    fn the_linux_program_alone_marks_a_native_build() {
        let base = scratch!("runtime-native-program");
        write(&base.join("Game.x86_64"), "\x7fELF");
        let mut game = two_builds();
        game.detect.linux_markers.clear();
        assert_eq!(of_for(false, &game, &base, Store::Steam), Runtime::Native);
    }

    #[test]
    fn a_default_program_that_is_not_a_windows_program_is_native() {
        let base = scratch!("runtime-plain-program");
        let mut game = test_games::slot();
        game.detect.linux_markers.clear();
        game.executables[0].path = "bin/game".into();
        game.executables[0].path_linux = None;
        write(&base.join("bin/game"), "#!/bin/sh\n");
        assert_eq!(of_for(false, &game, &base, Store::Manual), Runtime::Native);

        // A PE file is a Windows program even without the usual extension.
        write(&base.join("bin/game"), "MZ\x00\x00");
        assert_eq!(
            of_for(false, &game, &base, Store::Manual),
            Runtime::OwnedProton
        );
    }

    #[test]
    fn a_missing_program_is_judged_by_its_extension() {
        let base = scratch!("runtime-missing-program");
        std::fs::create_dir_all(&base).unwrap();
        let mut game = two_builds();
        game.executables[0].path_linux = None;
        assert_eq!(
            of_for(false, &game, &base, Store::Epic),
            Runtime::OwnedProton
        );
    }

    #[test]
    fn with_neither_program_there_a_named_linux_program_makes_it_native() {
        let base = scratch!("runtime-no-program");
        std::fs::create_dir_all(&base).unwrap();
        let mut game = two_builds();
        game.detect.linux_markers.clear();
        assert_eq!(of_for(false, &game, &base, Store::Steam), Runtime::Native);
    }
}
