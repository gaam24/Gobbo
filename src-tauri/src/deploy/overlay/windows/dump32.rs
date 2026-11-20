//! The 32-bit minidumper helper. The x64 Gobbo process cannot serve the 32-bit crash context; it starts
//! `gobbo_overlay_dump32.exe` before the game, waits for its socket, and kills it after the game exits.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use gobbo_overlay_dump::{ENV_DUMPER32, socket_path};

/// How long the helper may take to bind its socket.
const READY_TIMEOUT: Duration = Duration::from_secs(3);

/// The ready file of one launch.
const READY_FILE: &str = "dump32.ready";

/// A running 32-bit minidumper.
pub(super) struct Monitor {
    child: Child,
    path: PathBuf,
    ready: PathBuf,
}

impl Monitor {
    /// The socket the 32-bit hook connects to.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Stops the helper and removes its socket and ready file.
    pub(super) fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.ready);
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

/// A helper that has been started but may not have bound its socket yet. Its startup overlaps the rest of the launch
/// preparation; `wait` is called only just before the game resumes.
pub(super) struct Pending {
    child: Option<Child>,
    path: PathBuf,
    ready: PathBuf,
}

impl Pending {
    /// Waits for the helper's socket. A failure leaves the 32-bit hook with the in-process `DbgHelp` fallback.
    pub(super) fn wait(mut self) -> Option<Monitor> {
        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            if self.ready.is_file() {
                let child = self.child.take()?;
                return Some(Monitor {
                    child,
                    path: std::mem::take(&mut self.path),
                    ready: std::mem::take(&mut self.ready),
                });
            }
            let child = self.child.as_mut()?;
            if child.try_wait().ok()?.is_some() {
                self.child.take();
                return None;
            }
            thread::sleep(Duration::from_millis(10));
        }
        // The `Drop` stops a helper that never became ready.
        None
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_file(&self.ready);
        let _ = fs::remove_file(&self.path);
    }
}

/// Starts the helper next to Gobbo without waiting for its socket.
pub(super) fn spawn(state: &Path) -> Option<Pending> {
    let helper = helper()?;
    let path = socket_path("g32")?;
    let ready = state.join(READY_FILE);
    let _ = fs::remove_file(&ready);
    let _ = fs::remove_file(&path);
    let child = Command::new(helper)
        .arg(state)
        .arg(&path)
        .arg(&ready)
        // A GUI app must not flash a console for its helper.
        .creation_flags(0x0800_0000)
        .spawn()
        .ok()?;
    Some(Pending {
        child: Some(child),
        path,
        ready,
    })
}

/// The helper next to the Gobbo executable, or in the Cargo dev layout above it.
fn helper() -> Option<PathBuf> {
    crate::infra::binaries::adjacent("gobbo_overlay_dump32.exe")
}

/// The environment name is re-exported for the launch code.
pub(super) const ENV: &str = ENV_DUMPER32;
