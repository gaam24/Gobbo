//! Path templates from game definitions. A definition names a directory with `{game}`, `{prefix}`,
//! `{appdata}`, `{local_appdata}`, `{documents}`, `{home}` or `{settings}`; the core resolves it for
//! the current installation. In a Proton prefix the user folders are the ones Windows programs see there.
//! Templates never leave the chosen root through `..`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, ErrorKind};

/// The template variable for the Proton prefix.
pub const PREFIX_VARIABLE: &str = "{prefix}";

/// The error when the game's Proton prefix does not exist yet: the game has to start once to create it.
pub fn prefix_missing() -> AppError {
    AppError::user("settings.prefix_missing")
}

/// The values a template may use. `prefix` is the Proton prefix of the install, when it exists; `under_proton` says the
/// install runs under Proton, so its Windows user folders exist only inside that prefix and a missing prefix is an
/// error, never the Linux home folders. `settings` is the folder of the game's order file, as entered in the game
/// settings.
pub struct Context {
    pub game_root: PathBuf,
    pub prefix: Option<PathBuf>,
    pub under_proton: bool,
    pub home: PathBuf,
    pub settings: Option<PathBuf>,
}

impl Context {
    /// Resolves a template to an absolute path. Unknown variables and `..` are errors.
    pub fn resolve(&self, template: &str) -> Result<PathBuf, AppError> {
        let mut path = PathBuf::new();

        for part in template
            .split(['/', '\\'])
            .filter(|part| !part.is_empty() && *part != ".")
        {
            if part == ".." {
                return Err(unsafe_template(template));
            }

            let value = match part {
                "{game}" => Some(self.game_root.clone()),
                "{home}" => Some(self.home_path()?.to_path_buf()),
                "{prefix}" => Some(self.prefix.clone().ok_or_else(prefix_missing)?),
                "{appdata}" => Some(self.appdata()?),
                "{local_appdata}" => Some(self.local_appdata()?),
                "{documents}" => Some(self.documents()?),
                "{local_low}" => Some(self.local_low()?),
                "{settings}" => Some(self.settings.clone().ok_or_else(|| {
                    AppError::user("settings.settings_folder_missing").recovery(
                        crate::error::Recovery::new(
                            crate::error::RecoveryAction::SetSettingsFolder,
                        ),
                    )
                })?),
                _ if part.starts_with('{') => {
                    return Err(AppError::new("settings.unknown_path_variable")
                        .with("variable", part)
                        .kind(ErrorKind::Permanent));
                }
                _ => None,
            };

            match value {
                Some(value) => {
                    if value.is_absolute() && !path.as_os_str().is_empty() {
                        // A variable in the middle of a path must not reset the path built so far. Drop the root
                        // component (and a Windows drive or UNC prefix) before appending it.
                        let relative: PathBuf = value
                            .components()
                            .skip_while(|component| {
                                matches!(component, Component::Prefix(_) | Component::RootDir)
                            })
                            .collect();
                        path.push(relative);
                    } else {
                        path.push(value);
                    }
                }
                None => path.push(part),
            }
        }

        if path.as_os_str().is_empty() {
            return Err(unsafe_template(template));
        }

        Ok(path)
    }

    /// The prefix the Windows user folders live in. An install under Proton without its prefix yet is an error, so a
    /// path never resolves to the Linux home folder first and into the prefix after the first run.
    fn windows_prefix(&self) -> Result<Option<&Path>, AppError> {
        match &self.prefix {
            Some(prefix) => Ok(Some(prefix)),
            None if self.under_proton => Err(prefix_missing()),
            None => Ok(None),
        }
    }

    fn appdata(&self) -> Result<PathBuf, AppError> {
        self.known_folder("AppData/Roaming", "APPDATA", "XDG_CONFIG_HOME", ".config")
    }

    fn local_appdata(&self) -> Result<PathBuf, AppError> {
        self.known_folder(
            "AppData/Local",
            "LOCALAPPDATA",
            "XDG_DATA_HOME",
            ".local/share",
        )
    }

    /// Windows `LocalLow`, the folder Unity games use for saves and settings. Proton has the same folder under its
    /// prefix user; native Linux has no `LocalLow`, so a definition uses `path_linux` for a native build.
    fn local_low(&self) -> Result<PathBuf, AppError> {
        if let Some(prefix) = self.windows_prefix()? {
            return Ok(proton_user(prefix).join("AppData/LocalLow"));
        }

        if cfg!(windows) {
            return Ok(self.home_path()?.join("AppData/LocalLow"));
        }

        Err(AppError::user("settings.local_low_unavailable"))
    }

    /// A Windows known folder: its Proton path, its Windows environment variable, or its XDG fallback.
    fn known_folder(
        &self,
        proton: &str,
        windows_env: &str,
        xdg_env: &str,
        xdg_fallback: &str,
    ) -> Result<PathBuf, AppError> {
        if let Some(prefix) = self.windows_prefix()? {
            return Ok(proton_user(prefix).join(proton));
        }

        if cfg!(windows) {
            return env_path(windows_env).ok_or_else(missing_appdata);
        }

        let home = self.home_path()?;
        Ok(env_path(xdg_env).unwrap_or_else(|| home.join(xdg_fallback)))
    }

    fn documents(&self) -> Result<PathBuf, AppError> {
        if let Some(prefix) = self.windows_prefix()? {
            return Ok(proton_user(prefix).join("Documents"));
        }
        // The OS known folder follows a Documents folder redirected to OneDrive. The environment fallback keeps
        // tests and unusual setups working when the known folder is not available.
        if self.home == home()
            && let Some(documents) = dirs::document_dir()
        {
            return Ok(documents);
        }

        if cfg!(windows) {
            return Ok(self.home_path()?.join("Documents"));
        }

        let home = self.home_path()?;
        Ok(env_path("XDG_DOCUMENTS_DIR").unwrap_or_else(|| home.join("Documents")))
    }

    fn home_path(&self) -> Result<&Path, AppError> {
        if self.home.as_os_str().is_empty() {
            Err(AppError::user("system.no_home"))
        } else {
            Ok(&self.home)
        }
    }
}

/// The prefix user folder. Almost every prefix has `steamuser`; an old or custom prefix may name the Windows user
/// differently, so the first real user folder is used instead of creating an empty `steamuser`.
fn proton_user(prefix: &Path) -> PathBuf {
    let users = prefix.join("drive_c/users");

    if users.join("steamuser").is_dir() {
        return users.join("steamuser");
    }

    if let Ok(entries) = fs::read_dir(&users) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if entry.path().is_dir() && name != "Public" && name != "Default" && name != "All Users"
            {
                return entry.path();
            }
        }
    }

    users.join("steamuser")
}

/// The user's home folder, or an empty path when the system does not tell us. Windows sets `USERPROFILE`, Linux `HOME`.
pub fn home() -> PathBuf {
    let names = if cfg!(windows) {
        ["USERPROFILE", "HOME"]
    } else {
        ["HOME", "USERPROFILE"]
    };
    names.into_iter().find_map(env_path).unwrap_or_default()
}

/// The user's home folder. Steam and Proton paths are built from it.
pub fn home_dir() -> Result<PathBuf, AppError> {
    let home = home();

    if home.as_os_str().is_empty() {
        return Err(AppError::user("system.no_home").kind(ErrorKind::Permanent));
    }

    Ok(home)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn missing_appdata() -> AppError {
    AppError::new("settings.appdata_missing")
}

fn unsafe_template(template: &str) -> AppError {
    AppError::new("settings.unsafe_path_template")
        .with("template", template)
        .kind(ErrorKind::Permanent)
}

#[cfg(test)]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
mod tests {
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
}

#[cfg(test)]
mod prefix_user_tests {
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
