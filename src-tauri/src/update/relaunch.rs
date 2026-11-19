//! Starts the updated copy. The old copy exits first and the new one waits for it to end: the single-instance plugin
//! would otherwise hand the start to the copy that is leaving, and the new copy would quit at once.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::error::{AppError, io_at};

/// The environment variable that names the process a restarted copy waits for.
pub const PARENT_ENV: &str = "GOBBO_UPDATE_PARENT";
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

/// Called before the app starts. A copy that an update started waits here until its parent has ended.
pub fn wait_for_parent() {
    let Some(pid) = std::env::var(PARENT_ENV)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return;
    };
    wait_until_gone(pid, MAX_WAIT);
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
}
