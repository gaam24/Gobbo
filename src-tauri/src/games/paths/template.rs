//! The template context and the path resolution rules that back it.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, ErrorKind};

use super::{env_path, home, missing_appdata, prefix_missing, unsafe_template};

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
