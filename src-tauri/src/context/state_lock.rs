//! One writer for the game state, across threads and processes. The frontend already queues profile edits, but a
//! Steam launch (`gobbo run`) is a second process and a background import or an apply can overlap a delete. This lock
//! covers the file-moving operations so two writers cannot interleave on the same data root.
//!
//! The lock is advisory and reentrant on one thread: a public operation can call another one that locks again. Another
//! thread or another process gets a typed `state.busy` instead of waiting forever.

use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::{AppError, io_at};

/// The lock file in the data root. It is created on first use and kept; the OS lock, not the file's contents, is the
/// state.
const LOCK_FILE: &str = "gobbo.lock";
/// A child process forked by another thread inherits the lock's open file description, and the lock stays taken until
/// the child execs and `O_CLOEXEC` closes it. A short retry keeps that millisecond window from looking like a busy
/// apply; a real writer elsewhere still gets `state.busy` after this wait.
const RETRIES: u32 = 25;
const RETRY_DELAY: Duration = Duration::from_millis(2);
/// How often `lock_state_wait` tries again while another thread or process holds the lock.
const WAIT_RETRY: Duration = Duration::from_millis(25);

thread_local! {
    /// How many locks this thread holds. The first call takes the OS lock; a nested call only increments this.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// A held state lock. Dropping the outermost guard releases the OS lock; a nested guard only decrements the depth.
pub struct StateLock {
    _file: Option<File>,
}

impl Drop for StateLock {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Takes the one writer lock for `root`. `root` is the data root, not a game folder: the lock covers every game in this
/// library, because the file-moving operations share the same profiles, library index and running-game table.
pub fn lock_state(root: &Path) -> Result<StateLock, AppError> {
    let nested = DEPTH.with(|depth| {
        let held = depth.get();
        depth.set(held + 1);
        held > 0
    });

    if nested {
        return Ok(StateLock { _file: None });
    }

    let path = root.join(LOCK_FILE);
    let file = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) => {
            release_depth();
            return Err(io_at(&path)(error));
        }
    };
    let mut result = file.try_lock();
    let mut attempt = 1;

    while attempt < RETRIES && matches!(&result, Err(std::fs::TryLockError::WouldBlock)) {
        std::thread::sleep(RETRY_DELAY);
        result = file.try_lock();
        attempt += 1;
    }

    match result {
        Ok(()) => Ok(StateLock { _file: Some(file) }),
        Err(std::fs::TryLockError::WouldBlock) => {
            release_depth();
            Err(busy())
        }
        Err(std::fs::TryLockError::Error(error)) => {
            release_depth();
            Err(io_at(&path)(error))
        }
    }
}

/// Takes the state lock, retrying a busy lock until `timeout` expires. A thread that already holds the lock re-enters
/// it at once, as with [`lock_state`]; a caller that never gets it keeps the final `state.busy`.
pub fn lock_state_wait(root: &Path, timeout: Duration) -> Result<StateLock, AppError> {
    let deadline = Instant::now() + timeout;
    loop {
        match lock_state(root) {
            Ok(guard) => return Ok(guard),
            Err(error) if error.code == "state.busy" && Instant::now() < deadline => {
                std::thread::sleep(WAIT_RETRY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn release_depth() {
    DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
}

fn busy() -> AppError {
    AppError::retryable("state.busy")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::sync::mpsc;

    #[test]
    fn the_lock_is_reentrant_on_one_thread_and_busy_on_another() {
        let root = scratch!("state-lock");
        let outer = lock_state(&root).unwrap();
        let inner = lock_state(&root).unwrap();
        drop(inner);

        let (sender, receiver) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let result = lock_state(&root);
                sender.send(result.is_err()).unwrap();
            });
        });
        assert!(receiver.recv().unwrap(), "a second thread must get busy");

        drop(outer);
        assert!(lock_state(&root).is_ok());
    }

    #[test]
    fn waiting_for_the_lock_outlives_a_short_busy_period() {
        let root = scratch!("state-lock-wait");
        let (held_tx, held_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let shared = &root;
        std::thread::scope(|scope| {
            scope.spawn(move || {
                let guard = lock_state(shared).unwrap();
                held_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                drop(guard);
            });
            held_rx.recv().unwrap();
            assert_eq!(
                lock_state_wait(&root, Duration::from_millis(20))
                    .err()
                    .map(|error| error.code),
                Some("state.busy".to_owned())
            );
            release_tx.send(()).unwrap();
            assert!(lock_state_wait(&root, Duration::from_secs(1)).is_ok());
        });
    }

    #[test]
    fn the_lock_file_is_locked_for_other_handles() {
        let root = scratch!("state-lock-file");
        let held = lock_state(&root).unwrap();
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(LOCK_FILE))
            .unwrap();
        assert!(matches!(
            other.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(held);
        let again = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(LOCK_FILE))
            .unwrap();
        assert!(again.try_lock().is_ok());
    }
}
