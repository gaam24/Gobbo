//! Texts and errors the frontend shows. The backend sends a stable `code` with raw `params`, never a
//! sentence. The frontend turns the code into text from its locale files (`errors.<code>` for errors, and
//! the matching section for other texts), so a translation never has to be built in Rust.
//!
//! Codes are grouped by where a failure happens and how specific it is: `profile.*` (singular) names a failure of one
//! item in a list (`profile.duplicate_mod`, `profile.load_order_full`, `profile.separator_name`), while
//! `profiles.*` (plural) names the profile set or a command on it (`profiles.name_taken`, `profiles.not_found`,
//! `profiles.copy_same`). `source.*` is a generic provider failure (`source.unknown`, `source.bad_link`), where
//! `nexus.*` and `thunderstore.*` name an adapter's failure and `relations.*` names one in the mod-relations context —
//! the same idea at different specificity. Keep the strings as they are; the frontend translates them.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A text for the interface, identified by a code such as `file.no_conflict`, with the values it needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Message {
    pub code: String,
    pub params: BTreeMap<String, String>,
}

impl Message {
    /// A message with `code` and no parameters.
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_string(),
            params: BTreeMap::new(),
        }
    }

    /// Adds an interpolation value. The key is a stable name the locale message uses, so it is part of the translation
    /// contract, not a local label.
    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }
}

/// How the interface shows an error: a user action, a temporary failure, a permanent one, or a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The user can fix it, for example by saving a key or choosing a folder. The fix is shown next to the error.
    UserAction,
    /// Temporary: the same request may work later, for example after a network failure.
    Retryable,
    /// Fails again with the same input.
    Permanent,
    /// A Gobbo bug or an unexpected failure of the system. Shown with a pointer to the log.
    Bug,
}

/// What the user can do about an error. The frontend turns an action into a button next to the message, so a failure is
/// never only text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    SetGameFolder,
    SetSettingsFolder,
    SetModsFolder,
    SaveProviderKey,
    CloseGame,
    OpenLog,
}

/// A concrete recovery step: the action and, when it needs one, the object it applies to (a provider id, a game id).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
pub struct Recovery {
    pub action: RecoveryAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Recovery {
    /// A recovery step with no target, for an action that applies to the error itself.
    pub const fn new(action: RecoveryAction) -> Self {
        Self {
            action,
            target: None,
        }
    }

    /// Names the object the action applies to, such as a provider or game id.
    pub fn target(mut self, target: impl ToString) -> Self {
        self.target = Some(target.to_string());
        self
    }
}

/// An error returned by a command.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
pub struct AppError {
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub kind: ErrorKind,
    /// The step the user can take next. Omitted when the message is enough.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
}

/// The stop code for a download the user stopped, whatever it was downloading. `pack.cancelled`,
/// `collection.cancelled` and `import.cancelled` are deliberate siblings: each names the context a stop happened in
/// so the window can answer in that context, not a duplicate of this code. Keep the strings as they are; the frontend
/// translates them.
pub const DOWNLOAD_CANCELLED: &str = "download.cancelled";
/// An enabled mod needs the game's loader package, and it is not installed, when an apply is attempted anyway. The
/// status names the same need with `status::stage::LOADER_MISSING`.
pub const LOADER_MISSING_DEPLOY: &str = "deploy.loader_missing";

/// A saved account without direct-download entitlement. This is the one code for that meaning: a known free account, a
/// refused paid download and a browser route all answer with it, and the window opens the mod's files page.
pub const PAYMENT_REQUIRED: &str = "source.payment_required";

/// A file the provider refuses to serve for moderation or quarantine reasons. This is *not* an entitlement failure:
/// use `PAYMENT_REQUIRED` when the account tier is the cause.
pub const DOWNLOAD_UNAVAILABLE: &str = "nexus.download_unavailable";

// The credential family: a missing saved key, a game definition without a provider key, and a key the provider
// rejected. These describe the key, not the account's tier, so they stay separate from PAYMENT_REQUIRED. Each one
// carries a `provider` param; saving the key is the fix for the first and last of them.

/// The saved key has not been entered yet.
pub const KEY_MISSING: &str = "source.key_missing";
/// The game definition does not name a key for this provider.
pub const NO_GAME_KEY: &str = "source.no_game_key";
/// The provider rejected the key it was given.
pub const KEY_REJECTED: &str = "nexus.key_rejected";

/// A mod that is not in the library, however the caller names it: a family id, a provider's mod id or a required mod.
/// The `folder` param names the mod; a source-related failure also passes `provider`.
pub const MOD_MISSING: &str = "library.mod_missing";

/// A provider 404. The `kind` param says what was not found (`mod`, `file` or `collection`), so one code per provider
/// family replaces a code per endpoint.
pub const NEXUS_NOT_FOUND: &str = "nexus.not_found";
/// The Thunderstore equivalent of [`NEXUS_NOT_FOUND`].
pub const THUNDERSTORE_NOT_FOUND: &str = "thunderstore.not_found";

impl AppError {
    /// Whether the user stopped the work. The codes are per-context siblings (`download.cancelled`,
    /// `pack.cancelled`, `collection.cancelled`, `import.cancelled`) that all end in `cancelled`, so a loop asks this
    /// instead of naming each feature's code. The context is kept for the frontend's message.
    pub fn is_cancelled(&self) -> bool {
        self.code.ends_with("cancelled")
    }

    /// An error with `code`, no parameters or recovery, and a permanent kind.
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_string(),
            params: BTreeMap::new(),
            kind: ErrorKind::Permanent,
            recovery: None,
        }
    }

    /// Adds the concrete step the interface offers next to the message.
    pub fn recovery(mut self, recovery: Recovery) -> Self {
        self.recovery = Some(recovery);
        self
    }

    /// A failure the user can fix, for example by saving a key or choosing a folder.
    pub fn user(code: &str) -> Self {
        Self::new(code).kind(ErrorKind::UserAction)
    }

    /// A temporary failure: the same request may work later.
    pub fn retryable(code: &str) -> Self {
        Self::new(code).kind(ErrorKind::Retryable)
    }

    /// Adds an interpolation value. The key is a stable name the locale message uses, so it is part of the translation
    /// contract, not a local label.
    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    /// Sets what the user can do about the failure.
    pub const fn kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }
}

/// The error shown when a change needs a stopped game.
pub fn game_running() -> AppError {
    AppError::user("deploy.game_running").recovery(Recovery::new(RecoveryAction::CloseGame))
}

/// The code of a game folder that is not set. The game settings is where it is set, so the error offers that step.
pub const GAME_FOLDER_MISSING: &str = "settings.game_folder_missing";

/// The error shown when the game has no folder yet. The interface offers to set it in the game settings.
pub fn game_folder_missing() -> AppError {
    AppError::user(GAME_FOLDER_MISSING).recovery(Recovery::new(RecoveryAction::SetGameFolder))
}

/// The error shown when a deployment is already active and must be undone first.
pub fn deployment_active() -> AppError {
    AppError::user("deploy.active")
}

/// A zip, 7z or rar archive that cannot be read: damaged, encrypted or an unsupported shape.
pub fn bad_archive(detail: impl ToString) -> AppError {
    AppError::new("import.bad_archive")
        .with("detail", detail)
        .kind(ErrorKind::Permanent)
}

/// Logs and the CLI show the code with its params. The interface translates the code instead.
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)?;

        for (key, value) in &self.params {
            write!(f, " {key}={value:?}")?;
        }

        Ok(())
    }
}

impl std::error::Error for AppError {}

impl AppError {
    /// A failure with no code of its own. The detail is English text for the log; the interface shows a generic message.
    pub fn unexpected(detail: impl ToString) -> Self {
        Self::new("unexpected")
            .with("detail", detail)
            .kind(ErrorKind::Bug)
    }
}

impl From<String> for AppError {
    fn from(detail: String) -> Self {
        Self::unexpected(detail)
    }
}

/// A status message keeps the code and params of an error. The error kind is not needed there.
impl From<AppError> for Message {
    fn from(error: AppError) -> Self {
        Self {
            code: error.code,
            params: error.params,
        }
    }
}

/// A file the app could not read or write: the path and the system's own reason. Every module that touches the disk
/// uses this one type, so the message is the same everywhere. It becomes an `AppError` at the command boundary.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source}", path.display())]
pub struct FileError {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

impl FileError {
    /// A file failure at `path`, with the system's own `source` error.
    pub fn new(path: &Path, source: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }
}

impl From<FileError> for AppError {
    fn from(error: FileError) -> Self {
        Self::new("io.failed")
            .with("path", error.path.display())
            .with("detail", &error.source)
            .kind(ErrorKind::Permanent)
    }
}

/// For `map_err` on a file operation that returns an `AppError`: `fs::read(p).map_err(io_at(p))`.
pub fn io_at(path: &Path) -> impl Fn(io::Error) -> AppError + '_ {
    move |source| FileError::new(path, source).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_code_reads_as_cancelled_by_its_suffix() {
        // Two unrelated contexts prove the suffix rule; listing every feature's code here would duplicate the rule.
        assert!(AppError::user("import.cancelled").is_cancelled());
        assert!(AppError::user(DOWNLOAD_CANCELLED).is_cancelled());
        assert!(!AppError::user("import.failed").is_cancelled());
        assert!(!AppError::user("cancelled.import").is_cancelled());
    }

    #[test]
    fn plain_text_becomes_an_unexpected_bug() {
        let error = AppError::from("disk is full".to_string());
        assert_eq!(error.code, "unexpected");
        assert_eq!(error.params["detail"], "disk is full");
        assert_eq!(error.kind, ErrorKind::Bug);
    }
}
