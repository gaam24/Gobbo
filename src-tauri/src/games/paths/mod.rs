//! Path templates from game definitions. A definition names a directory with `{game}`, `{prefix}`,
//! `{appdata}`, `{local_appdata}`, `{documents}`, `{home}` or `{settings}`; the core resolves it for
//! the current installation. In a Proton prefix the user folders are the ones Windows programs see there.
//! Templates never leave the chosen root through `..`.

mod template;

#[cfg(test)]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
mod tests;

use std::path::PathBuf;

use crate::error::{AppError, ErrorKind};

pub use template::Context;

/// The template variable for the Proton prefix.
pub const PREFIX_VARIABLE: &str = "{prefix}";

/// The error when the game's Proton prefix does not exist yet: the game has to start once to create it.
pub fn prefix_missing() -> AppError {
    AppError::user("settings.prefix_missing")
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
