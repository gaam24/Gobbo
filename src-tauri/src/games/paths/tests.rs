//! Tests of the path templates and the folders they resolve to.

use std::path::{Path, PathBuf};

use super::*;

fn context(prefix: Option<&Path>, home: &Path) -> Context {
    Context {
        game_root: PathBuf::from("/games/Test"),
        prefix: prefix.map(Path::to_path_buf),
        under_proton: false,
        home: home.to_path_buf(),
        settings: None,
    }
}

#[test]
fn under_proton_a_missing_prefix_never_falls_back_to_the_home_folders() {
    let home = Path::new("/home/user");
    let ctx = Context {
        under_proton: true,
        ..context(None, home)
    };
    for template in [
        "{appdata}/Game",
        "{local_appdata}/Game",
        "{documents}/Game",
        "{local_low}/Game",
    ] {
        assert_eq!(
            ctx.resolve(template).unwrap_err().code,
            "settings.prefix_missing",
            "{template}"
        );
    }
    assert_eq!(
        ctx.resolve("{game}/Mods").unwrap(),
        Path::new("/games/Test/Mods")
    );

    let prefix = Path::new("/prefix/pfx");
    let ctx = Context {
        under_proton: true,
        ..context(Some(prefix), home)
    };
    assert_eq!(
        ctx.resolve("{appdata}/Game").unwrap(),
        prefix.join("drive_c/users/steamuser/AppData/Roaming/Game")
    );
}

#[test]
fn game_root_is_used_directly() {
    let home = Path::new("/home/user");
    let path = context(None, home).resolve("{game}/Mods").unwrap();
    assert_eq!(path, Path::new("/games/Test/Mods"));
}

#[test]
fn the_settings_folder_comes_from_the_game_settings() {
    let home = Path::new("/home/user");
    let settings = Path::new("/data/settings");
    let ctx = Context {
        settings: Some(settings.to_path_buf()),
        ..context(None, home)
    };
    assert_eq!(
        ctx.resolve("{settings}/mods.settings").unwrap(),
        settings.join("mods.settings")
    );
}

#[test]
fn a_settings_variable_without_a_folder_is_an_error() {
    let err = context(None, Path::new("/home/user"))
        .resolve("{settings}/mods.settings")
        .unwrap_err();
    assert_eq!(err.code, "settings.settings_folder_missing");
}

#[test]
fn a_proton_prefix_gives_windows_user_folders() {
    let home = Path::new("/home/user");
    let prefix = Path::new("/steam/compatdata/1/pfx");
    let ctx = context(Some(prefix), home);
    assert_eq!(
        ctx.resolve("{appdata}/ExampleGame").unwrap(),
        prefix.join("drive_c/users/steamuser/AppData/Roaming/ExampleGame")
    );
    assert_eq!(
        ctx.resolve("{local_appdata}/Example Studio").unwrap(),
        prefix.join("drive_c/users/steamuser/AppData/Local/Example Studio")
    );
    assert_eq!(
        ctx.resolve("{documents}/Example Game").unwrap(),
        prefix.join("drive_c/users/steamuser/Documents/Example Game")
    );
}

#[test]
fn without_a_prefix_the_xdg_folders_are_used() {
    let home = Path::new("/home/user");
    let ctx = context(None, home);
    // The environment can set XDG variables; the fallback is what matters here.
    let appdata = ctx.resolve("{appdata}/Game").unwrap();
    assert!(appdata.ends_with("Game"));
    assert!(appdata.is_absolute());
}

#[test]
fn a_prefix_variable_without_a_prefix_is_an_error() {
    let err = context(None, Path::new("/home/user"))
        .resolve("{prefix}/drive_c")
        .unwrap_err();
    assert_eq!(err.code, "settings.prefix_missing");
}

#[test]
fn a_missing_home_is_an_error_for_user_folders() {
    let ctx = context(None, Path::new(""));
    assert_eq!(
        ctx.resolve("{home}/Game").unwrap_err().code,
        "system.no_home"
    );
}

#[cfg(windows)]
#[test]
fn a_windows_variable_in_the_middle_keeps_the_path_before_it() {
    let ctx = Context {
        game_root: PathBuf::from(r"C:\Games\Test"),
        prefix: None,
        under_proton: false,
        home: PathBuf::from(r"C:\Users\Player"),
        settings: None,
    };
    assert_eq!(
        ctx.resolve("{game}/{documents}/save").unwrap(),
        Path::new(r"C:\Games\Test\Users\Player\Documents\save")
    );
}

#[test]
fn parent_segments_and_unknown_variables_are_refused() {
    let ctx = context(None, Path::new("/home/user"));
    assert_eq!(
        ctx.resolve("{game}/../outside").unwrap_err().code,
        "settings.unsafe_path_template"
    );
    assert_eq!(
        ctx.resolve("{game}/{unknown}").unwrap_err().code,
        "settings.unknown_path_variable"
    );
}

mod prefix_user_tests {
    use std::fs;

    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_prefix_user_other_than_steamuser_is_found() {
        let base = scratch!("prefix-user");
        let prefix = base.join("pfx");
        fs::create_dir_all(prefix.join("drive_c/users/alice/AppData/Roaming")).unwrap();
        fs::create_dir_all(prefix.join("drive_c/users/alice/AppData/LocalLow")).unwrap();
        let ctx = Context {
            prefix: Some(prefix.clone()),
            under_proton: false,
            home: "/home/user".into(),
            game_root: "/games/Test".into(),
            settings: None,
        };
        assert_eq!(
            ctx.resolve("{appdata}/Game").unwrap(),
            prefix.join("drive_c/users/alice/AppData/Roaming/Game")
        );
        assert_eq!(
            ctx.resolve("{local_low}/Game").unwrap(),
            prefix.join("drive_c/users/alice/AppData/LocalLow/Game")
        );
        assert_eq!(
            ctx.resolve("{documents}/Game").unwrap(),
            prefix.join("drive_c/users/alice/Documents/Game")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn local_low_without_a_prefix_is_an_error_on_native_linux() {
        let err = Context {
            prefix: None,
            under_proton: false,
            home: "/home/user".into(),
            game_root: "/games/Test".into(),
            settings: None,
        }
        .resolve("{local_low}/Game")
        .unwrap_err();
        assert_eq!(err.code, "settings.local_low_unavailable");
    }
}
