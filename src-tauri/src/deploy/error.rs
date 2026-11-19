//! The deployment errors and their stable interface codes.

use std::path::PathBuf;

use crate::domain::resolver::Problem;
use crate::error::{AppError, ErrorKind, FileError};

/// A deployment failed. The message is English text for the log; the interface translates the code that
/// `From<DeployError>` for `AppError` gives.
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error(transparent)]
    Io(#[from] FileError),
    #[error("journal: {0}")]
    Json(#[from] serde_json::Error),
    /// A journal entry names a path outside the roots this deployment may touch. Undo stops before it changes anything.
    #[error("journal entry is outside the deployment roots: {}", path.display())]
    JournalUnsafe { path: PathBuf },
    /// The plan has problems. Nothing was saved.
    #[error("the plan has problems, nothing was written: {0:?}")]
    Blocked(Vec<Problem>),
    #[error("missing file {}", path.display())]
    MissingFile { path: PathBuf },
    #[error("file is not UTF-8: {}", path.display())]
    NotUtf8 { path: PathBuf },
    /// The path contains `,` or `:`, which are the separators of `mount` options.
    #[error("path {} contains ',' or ':', which cannot be mounted", path.display())]
    #[cfg_attr(
        not(target_os = "linux"),
        allow(
            dead_code,
            reason = "only the Linux overlay mounts paths through comma- and colon-separated options"
        )
    )]
    UnsafePath { path: PathBuf },
    /// Overlay works only on Linux (namespaces and overlayfs) and Windows (the hook DLL).
    #[error("overlay works only on Linux and Windows")]
    OverlayUnsupported,
    /// The overlay hook DLL is not next to Gobbo. Windows needs it to start a game in an overlay deployment.
    #[error("the overlay hook {} is missing", path.display())]
    #[cfg_attr(
        not(windows),
        allow(
            dead_code,
            reason = "the overlay hook DLL is Windows-only; Linux overlays use namespaces and overlayfs"
        )
    )]
    OverlayHookMissing { path: PathBuf },
    /// A game step (a program the game definition names) could not run or failed.
    #[error("a game step failed: {detail}")]
    StepFailed { detail: String },
    /// The game and layer paths are too long for `mount` (overlayfs rejects a long list of layers).
    #[error(
        "the game and layer paths are {len} characters together, and overlayfs accepts fewer; move the game or the library closer to the root"
    )]
    #[cfg_attr(
        not(target_os = "linux"),
        allow(
            dead_code,
            reason = "only the Linux overlay checks the length of the mount layer paths"
        )
    )]
    PathTooLong { len: usize },
    /// Starting the game through overlay without an earlier overlay deployment.
    #[error("there is no deployment; apply the mods first")]
    NoOverlaySession,
    /// The game folder is still used by an overlay, so the layer of its mods cannot be rebuilt under the game.
    #[error("the game in {} still runs in its overlay; stop it before the mods change", path.display())]
    OverlayInUse { path: PathBuf },
    /// The kernel or the user namespaces do not support the overlay mount.
    #[error("overlay is not available: {detail}")]
    #[cfg_attr(
        not(target_os = "linux"),
        allow(
            dead_code,
            reason = "only the Linux overlay probes the kernel; Windows uses the hook DLL"
        )
    )]
    OverlayUnavailable { detail: String },
    #[error("a deployment already exists; undo it first")]
    AlreadyApplied,
    #[error("nothing to undo")]
    NothingToUndo,
}

impl From<DeployError> for AppError {
    fn from(error: DeployError) -> Self {
        match error {
            DeployError::Io(file) => file.into(),
            DeployError::Json(e) => Self::new("deploy.journal_failed")
                .with("detail", e)
                .kind(ErrorKind::Bug),
            DeployError::JournalUnsafe { path } => Self::new("deploy.journal_unsafe")
                .with("path", path.display())
                .kind(ErrorKind::Permanent),
            DeployError::Blocked(problems) => Self::new("deploy.blocked")
                .with("count", problems.len())
                .kind(ErrorKind::UserAction),
            DeployError::MissingFile { path } => Self::new("deploy.missing_file")
                .with("path", path.display())
                .kind(ErrorKind::UserAction),
            DeployError::NotUtf8 { path } => Self::new("deploy.not_utf8")
                .with("path", path.display())
                .kind(ErrorKind::Permanent),
            DeployError::UnsafePath { path } => Self::new("deploy.unsafe_path")
                .with("path", path.display())
                .kind(ErrorKind::UserAction),
            DeployError::OverlayUnsupported => {
                Self::new("deploy.overlay_unsupported").kind(ErrorKind::Permanent)
            }
            DeployError::OverlayHookMissing { path } => Self::new("deploy.overlay_hook_missing")
                .with("path", path.display())
                .kind(ErrorKind::Permanent)
                .recovery(crate::error::Recovery::new(
                    crate::error::RecoveryAction::OpenLog,
                )),
            DeployError::StepFailed { detail } => Self::new("deploy.step_failed")
                .with("detail", detail)
                .kind(ErrorKind::UserAction),
            DeployError::PathTooLong { len } => Self::new("deploy.path_too_long")
                .with("length", len)
                .kind(ErrorKind::UserAction),
            DeployError::NoOverlaySession => {
                Self::new("deploy.no_overlay_session").kind(ErrorKind::UserAction)
            }
            DeployError::OverlayInUse { path } => Self::new("deploy.overlay_in_use")
                .with("path", path.display())
                .kind(ErrorKind::UserAction),
            DeployError::OverlayUnavailable { detail } => Self::new("deploy.overlay_unavailable")
                .with("detail", detail)
                .kind(ErrorKind::UserAction),
            DeployError::AlreadyApplied => {
                Self::new("deploy.already_applied").kind(ErrorKind::UserAction)
            }
            DeployError::NothingToUndo => {
                Self::new("deploy.nothing_to_undo").kind(ErrorKind::Permanent)
            }
        }
    }
}
