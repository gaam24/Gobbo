//! Linux: overlayfs in a namespace. `unshare` starts the program in a new user and mount namespace, where the overlay is
//! mounted on the game folder. The lower layers are the mod layer and the game folder; the game's writes in the session go
//! to `overwrite/`, so the game folder is never written to.

use std::fs;
use std::io::PipeWriter;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;

use super::Session;
use crate::deploy::error::DeployError;
use crate::error::FileError;

/// `unshare` exits 125 when it cannot create the namespace, before the probe command ever runs. A user namespace that
/// the kernel refuses is the common cause; reporting it as a generic "the probe exited" hides the reason.
const UNSHARE_FAILURE: i32 = 125;

/// The detail for a failed probe. Exit 125 names an `unshare` failure; any other status is the probe command's own
/// refusal, which the caller can reproduce with a different program.
fn probe_failure(program: &str, status: ExitStatus) -> String {
    match status.code() {
        Some(UNSHARE_FAILURE) => format!(
            "{program} could not create a user namespace (exit {UNSHARE_FAILURE}); unprivileged user namespaces may be disabled"
        ),
        _ => format!("the probe exited with {status}"),
    }
}

/// Mounts the view on `$2` with the options `$1`, enters the game folder `$3` and replaces the process with the command
/// from the remaining arguments. The folder is resolved again after the mount: the folder the process opened before the
/// mount is the plain one, so relative paths would bypass every mod. The deployment root and the game folder differ for
/// a game that keeps its mods outside the game folder.
const SCRIPT: &str = r#"mount -t overlay overlay -o "$1" "$2" || exit 1; game=$(pwd -P) || exit 1; cd / && cd "$game" || exit 1; shift 3; exec "$@""#;

/// The longest layer list (`lowerdir`) that `mount` accepts on this kernel. Measured: 279 characters work,
/// 297 no longer does (`wrong fs type` error). The limit has a margin, because the paths depend on the system.
const MAX_LOWERDIR_LEN: usize = 250;

impl Session {
    /// Layers from the top: the mod layer, then the game.
    fn lowerdir(&self) -> String {
        format!("{}:{}", self.mods.display(), self.view_root().display())
    }

    /// The `mount -t overlay` options. The mod layer lies above the game, so its files win.
    fn options(&self) -> String {
        format!(
            "lowerdir={},upperdir={},workdir={}",
            self.lowerdir(),
            self.overwrite.display(),
            self.work.display()
        )
    }
}

/// Checks what the mount needs, before anything is written: the paths must not contain the separators of the `mount`
/// options, the layer list must fit in what the kernel accepts, and a tiny overlay must really mount in a user namespace.
/// The last check catches a kernel with unprivileged user namespaces disabled, which otherwise looks like a game crash.
pub fn check(session: &Session) -> Result<(), DeployError> {
    check_mount_path(session.view_root())?;
    check_mount_path(&session.state)?;
    let lowerdir_len = session.lowerdir().len();

    if lowerdir_len > MAX_LOWERDIR_LEN {
        return Err(DeployError::PathTooLong { len: lowerdir_len });
    }

    if !cfg!(test) {
        probe().map_err(|detail| DeployError::OverlayUnavailable { detail })?;
    }

    Ok(())
}

/// The result of the one probe per process. The kernel and the user-namespace setting do not change while Gobbo runs.
static PROBE: OnceLock<Result<(), String>> = OnceLock::new();

/// Mounts a tiny overlay in a user namespace once. A missing `unshare`, a refused user namespace or a missing
/// `overlay` module is reported as overlay being unavailable, not as a game that crashed on start.
fn probe() -> Result<(), String> {
    PROBE.get_or_init(|| probe_with("unshare")).clone()
}

/// `probe` with the program to run. Tests pass a script that succeeds or fails without touching the kernel.
fn probe_with(program: &str) -> Result<(), String> {
    let base = std::env::temp_dir().join(format!(
        "gobbo-overlay-probe-{}",
        crate::infra::store::unique_suffix()
    ));
    let lower = base.join("lower");
    let upper = base.join("upper");
    let work = base.join("work");
    let merged = base.join("merged");

    for dir in [&lower, &upper, &work, &merged] {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    let script =
        r#"mount -t overlay overlay -o "lowerdir=$1,upperdir=$2,workdir=$3" "$4" && umount "$4""#;
    let status = Command::new(program)
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--propagation",
            "private",
            "--",
        ])
        .arg("sh")
        .arg("-c")
        .arg(script)
        .arg("gobbo-probe")
        .arg(&lower)
        .arg(&upper)
        .arg(&work)
        .arg(&merged)
        .status();
    let _ = fs::remove_dir_all(&base);

    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(probe_failure(program, status)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("{program} is missing"))
        }
        Err(error) => Err(error.to_string()),
    }
}

/// The command that, in the namespace, mounts the view on the game folder and runs `program` with `args`.
/// The exit code of the command is the exit code of the game.
pub fn command(session: &Session, program: &str, args: &[String]) -> Command {
    let mut command = Command::new("unshare");
    command
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--propagation",
            "private",
            "--",
        ])
        .args(["sh", "-c", SCRIPT, "mm-overlay"])
        .arg(session.options())
        .arg(session.view_root())
        .arg(&session.game_root)
        .arg(program)
        .args(args)
        .current_dir(&session.game_root);
    command
}

/// A started program in the namespace. `wait` returns its exit code.
pub struct Running {
    child: Child,
}

impl Running {
    /// The launcher's steps; overlayfs starts through `unshare`, with no hook installer to time.
    #[expect(
        clippy::unused_self,
        reason = "the Windows Running::timings reads the steps from self, and the caller uses the same shape"
    )]
    pub const fn timings(&self) -> Option<super::LaunchTimings> {
        None
    }

    /// Waits until the program exits, and returns its exit code. A program killed by a signal answers `128 + signal`,
    /// as a shell reports it, so the launch report tells a crash from an ordinary exit code.
    pub fn wait(&mut self) -> Result<i32, DeployError> {
        use std::os::unix::process::ExitStatusExt;

        let status = self
            .child
            .wait()
            .map_err(|e| FileError::new(Path::new("unshare"), e))?;
        Ok(status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .unwrap_or(1))
    }
}

/// Starts `program` in the namespace, with `env` added to the environment it inherits. `output` takes its standard output
/// and error when it is given.
pub fn start(
    session: &Session,
    program: &str,
    args: &[String],
    env: &[(String, String)],
    output: Option<PipeWriter>,
) -> Result<Running, DeployError> {
    // The writable layer and its work folder must exist before `mount` names them; a state folder that lost either still
    // starts, and an existing one is left as it is.
    for dir in [&session.overwrite, &session.work] {
        fs::create_dir_all(dir).map_err(|e| FileError::new(dir, e))?;
    }

    let mut command = command(session, program, args);
    command.envs(env.iter().map(|(name, value)| (name, value)));

    if let Some(writer) = output {
        let stderr = writer
            .try_clone()
            .map_err(|e| FileError::new(Path::new("unshare"), e))?;
        command
            .stdout(Stdio::from(writer))
            .stderr(Stdio::from(stderr));
    }

    let child = command
        .spawn()
        .map_err(|e| FileError::new(Path::new("unshare"), e))?;
    Ok(Running { child })
}

/// The paths go into the `mount` options, where `,` and `:` are separators.
fn check_mount_path(path: &Path) -> Result<(), DeployError> {
    let text = path.to_string_lossy();

    if text.contains(',') || text.contains(':') {
        return Err(DeployError::UnsafePath {
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_probe_reports_a_success_a_refusal_and_a_missing_program() {
        assert!(probe_with("/bin/true").is_ok());
        assert!(probe_with("/bin/false").is_err());
        assert!(probe_with("/no/such/gobbo-probe").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_refused_user_namespace_names_the_exit_instead_of_a_generic_failure() {
        use std::os::unix::process::ExitStatusExt;

        let refused = ExitStatus::from_raw(UNSHARE_FAILURE << 8);
        let detail = probe_failure("unshare", refused);
        assert!(
            detail.contains("could not create a user namespace") && detail.contains("125"),
            "{detail}"
        );
        let other = probe_failure("unshare", ExitStatus::from_raw(1 << 8));
        assert!(other.contains("the probe exited"), "{other}");
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_exits_125_is_reported_as_a_namespace_failure() {
        use std::os::unix::fs::PermissionsExt;

        let base = crate::test_support::scratch!("overlay-probe-125");
        let script = base.join("exit125.sh");
        fs::write(&script, "#!/bin/sh\nexit 125\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let detail = probe_with(script.to_str().unwrap()).unwrap_err();
        assert!(
            detail.contains("could not create a user namespace"),
            "{detail}"
        );
    }
}
