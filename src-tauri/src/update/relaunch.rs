//! Starts the updated copy. The old copy exits first and the new one waits for it to end: the single-instance plugin
//! would otherwise hand the start to the copy that is leaving, and the new copy would quit at once.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::error::{AppError, io_at};

/// The environment variable that names the process a restarted copy waits for.
pub const PARENT_ENV: &str = "GOBBO_UPDATE_PARENT";
/// The argument that names the process an elevated copy waits for. A `runas` start does not inherit the environment, so
/// the pid travels with the command line instead.
pub const PARENT_ARG: &str = "--wait-for-parent";
/// The longest a restarted copy waits for its parent. A parent that is still alive after this is not waited for any
/// longer, so a stuck old process cannot keep the new one from starting.
const MAX_WAIT: Duration = Duration::from_secs(30);

/// Starts `exe` as a new copy that waits for this process to end.
pub fn spawn_after_exit(exe: &Path) -> Result<(), AppError> {
    Command::new(exe)
        .env(PARENT_ENV, std::process::id().to_string())
        .spawn()
        .map(|_| ())
        .map_err(io_at(exe))
}

/// Called before the app starts. A copy that an update or an elevated restart started waits here until its parent has
/// ended. The elevated restart passes the pid as an argument, because `ShellExecuteEx` does not carry the environment.
pub fn wait_for_parent() {
    let pid = std::env::var(PARENT_ENV)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .or_else(|| parent_from_args(std::env::args_os()));

    if let Some(pid) = pid {
        wait_until_gone(pid, MAX_WAIT);
    }
}

/// The pid after `--wait-for-parent`, when both the switch and a valid number are present. Kept separate from the
/// environment so a test can pass any argument list.
fn parent_from_args(args: impl IntoIterator<Item = OsString>) -> Option<u32> {
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        if arg.to_str() == Some(PARENT_ARG) {
            return args.next()?.to_str()?.parse().ok();
        }
    }

    None
}

/// Waits until the process `pid` is gone, or `limit` has passed. Returns whether it is gone.
pub fn wait_until_gone(pid: u32, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;

    loop {
        if !process_exists(pid) {
            return true;
        }

        if Instant::now() >= deadline {
            return false;
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn process_exists(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).is_some()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn a_process_that_is_not_running_is_gone_at_once() {
        // A pid this large is not assigned on Linux or Windows in practice.
        assert!(wait_until_gone(u32::MAX - 1, Duration::from_millis(50)));
    }

    #[test]
    fn a_running_process_is_not_waited_for_past_the_limit() {
        // This test process is alive, so the wait runs out and reports that.
        assert!(!wait_until_gone(
            std::process::id(),
            Duration::from_millis(200)
        ));
    }

    #[test]
    fn the_parent_pid_comes_from_the_argument() {
        let args = ["gobbo.exe", PARENT_ARG, "4242"].map(OsString::from);

        assert_eq!(parent_from_args(args), Some(4242));
    }

    #[test]
    fn a_missing_or_invalid_parent_pid_is_ignored() {
        assert_eq!(parent_from_args([OsString::from("gobbo.exe")]), None);
        assert_eq!(
            parent_from_args([OsString::from("gobbo.exe"), OsString::from(PARENT_ARG)]),
            None
        );
        assert_eq!(
            parent_from_args([
                OsString::from("gobbo.exe"),
                OsString::from(PARENT_ARG),
                OsString::from("not-a-pid")
            ]),
            None
        );
    }
}
