//! The app's own log: one file in the data root, rotated at a size cap, with request secrets redacted. It records the
//! failures that happen before the window can show them and the ones a user report needs. The game's output has its own
//! file in `launch/output.rs`; this one is Gobbo's.

use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::infra::http::redact;

/// The file in the data root. Visible in the crate so the console and the support bundle read the same file this module
/// writes.
pub const FILE_NAME: &str = "gobbo.log";
/// The one backup of the log. A file over the cap is moved here, replacing the earlier backup.
const BACKUP_FILE: &str = "gobbo.log.1";
/// The largest log kept. An older file is rotated at startup, never while the app writes.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The open log file. The app and the CLI share one process, so the first `init` wins.
static LOG: OnceLock<Mutex<Option<File>>> = OnceLock::new();

/// Starts the log in the data root. A file over the cap is moved to `gobbo.log.1`. A second call does nothing.
pub fn init(root: &Path) {
    let _ = fs::create_dir_all(root);
    let path = root.join(FILE_NAME);
    rotate(&path);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let _ = LOG.set(Mutex::new(file));
}

/// Writes a panic before the process aborts. `panic = "abort"` still runs the hook; the log is the only place a
/// release panic can leave evidence.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map_or_else(
            || "unknown".to_string(),
            |location| format!("{}:{}", location.file(), location.line()),
        );
        error(format!("panic at {location}: {info}"));
    }));
}

/// Records a normal event, for example a migration that changed a file.
pub fn info(message: impl Display) {
    write("INFO", message);
}

/// Records something that went wrong but did not stop the app.
pub fn warn(message: impl Display) {
    write("WARN", message);
}

/// Records a failure that stopped an action. The window may show a translated error for the same failure.
pub fn error(message: impl Display) {
    write("ERROR", message);
}

/// Writes one line to the file and to stderr. A missing file or a full disk only loses the line: logging never fails
/// the caller.
fn write(level: &str, message: impl Display) {
    let line = format_line(level, &message.to_string());

    if let Some(log) = LOG.get() {
        // A poisoned log still holds a usable file: a panic during an earlier write must not lose this line too.
        let mut guard = log
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }

    eprintln!("gobbo: {line}");
}

/// One line with an RFC 3339 time, the level and the redacted text.
fn format_line(level: &str, message: &str) -> String {
    let at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| crate::infra::clock::now_seconds().to_string());
    format!("{at} {level} {}", redact(message))
}

/// Moves a log at or over the cap to the backup, replacing the earlier one.
fn rotate(path: &Path) {
    rotate_at(path, MAX_BYTES);
}

/// `rotate` with the cap as a parameter, so a test does not write four megabytes.
fn rotate_at(path: &Path, max_bytes: u64) {
    if path.metadata().map_or(0, |meta| meta.len()) < max_bytes {
        return;
    }

    let backup = path.with_file_name(BACKUP_FILE);
    let _ = fs::remove_file(&backup);
    let _ = fs::rename(path, backup);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_line_carries_the_level_and_a_redacted_request() {
        let line = format_line(
            "WARN",
            "GET https://example.invalid/a?key=secret&expires=1 failed",
        );
        assert!(line.contains(" WARN GET https://example.invalid/a?key=***&expires=*** failed"));
    }

    #[test]
    fn a_rotated_log_keeps_the_old_backup_only() {
        let dir = scratch!("log-rotate");
        let path = dir.join(FILE_NAME);
        fs::write(&path, "newer").unwrap();
        fs::write(dir.join(BACKUP_FILE), "older").unwrap();
        rotate_at(&path, 1);
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(dir.join(BACKUP_FILE)).unwrap(), "newer");
    }

    #[test]
    fn a_small_log_is_not_rotated() {
        let dir = scratch!("log-small");
        let path = dir.join(FILE_NAME);
        fs::write(&path, "small").unwrap();
        rotate_at(&path, MAX_BYTES);
        assert_eq!(fs::read_to_string(&path).unwrap(), "small");
        assert!(!dir.join(BACKUP_FILE).exists());
    }
}
