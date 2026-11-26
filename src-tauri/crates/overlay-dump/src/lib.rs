//! The out-of-process minidumper server, shared by the Gobbo process (x64) and the 32-bit helper.
//!
//! The hook connects to a short `AF_UNIX` path and sends its crash context; this server writes the dump while the game
//! is still suspended. The app starts a server in its own process for x64 games and a `gobbo_overlay_dump32` child for
//! x86 games, because the minidumper protocol carries pointer-sized fields.

#![cfg(windows)]

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

use minidumper::{LoopAction, MinidumpBinary, Server, ServerHandler, SocketName};

// The dump markers, the clock and the pruning rule are owned by the decisions crate, the shared vocabulary of the
// session files. This crate writes them; the app and the hook import them from there too.
use gobbo_overlay_decisions::{CRASH_DUMP_FILE, LAST_DUMP_FILE, now_millis, split_newest};

/// The file the 32-bit helper writes once its socket is bound. The app's spawner and the helper use this name instead
/// of each spelling it.
pub const READY_FILE: &str = "dump32.ready";

/// The bytes the 32-bit helper writes to [`READY_FILE`] when its socket is listening.
const READY_CONTENT: &str = "ready";

/// The protocol's fixed buffer for an `AF_UNIX` socket path. A path must be shorter so the terminator fits.
const SOCKET_PATH_MAX: usize = 108;

/// The bound `socket_path` uses, below [`SOCKET_PATH_MAX`] so a generated path never reaches the terminator byte.
const SOCKET_PATH_LIMIT: usize = SOCKET_PATH_MAX - 8;

/// The folder of the session dumps.
const DUMPS: &str = "dumps";

/// How many dumps one session keeps.
const KEEP_DUMPS: usize = 10;

/// A running minidumper in this process.
pub struct Monitor {
    path: PathBuf,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Monitor {
    /// The socket the hook connects to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stops the server and waits for it.
    pub fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        self.shutdown.store(true, Ordering::Release);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // The x86 helper removes its own socket when it stops; do the same for this in-process server, now that its
        // thread has stopped listening. A stale socket would make the next launch refuse to bind.
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

/// Starts a server in a thread of this process. A failure is not fatal; the game can use the in-process fallback.
pub fn start(state: &Path) -> Option<Monitor> {
    let dir = dumps_dir(state);
    fs::create_dir_all(&dir).ok()?;
    let path = socket_path("g")?;
    let server = match Server::with_name(SocketName::Path(&path)) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "gobbo: could not start the overlay minidumper at {}: {error}",
                path.display()
            );
            return None;
        }
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&shutdown);
    let state = state.to_path_buf();
    let started = thread::Builder::new()
        .name("overlay-dump".to_string())
        .spawn(move || {
            let mut server = server;

            if let Err(error) = server.run(Box::new(Handler::new(&state)), &flag, None) {
                eprintln!("gobbo: the overlay minidumper stopped: {error}");
            }
        });
    match started {
        Ok(thread) => Some(Monitor {
            path,
            shutdown,
            thread: Some(thread),
        }),
        Err(error) => {
            eprintln!("gobbo: could not start the overlay dump thread: {error}");
            None
        }
    }
}

/// Writes [`READY_CONTENT`] to `ready` once the socket is bound, then runs a server in the current process until it
/// is killed.
///
/// The helper is called as `<state> <socket> <ready>`: the state folder owns the dumps, the socket carries the
/// pointer-sized protocol, and the ready file exists only after the socket is bound, so the spawner can pass the socket
/// to the hook without a race.
pub fn serve(state: &Path, socket: &Path, ready: &Path) -> Result<(), String> {
    fs::create_dir_all(dumps_dir(state)).map_err(|error| error.to_string())?;
    let server = Server::with_name(SocketName::Path(socket)).map_err(|error| error.to_string())?;
    fs::write(ready, READY_CONTENT.as_bytes()).map_err(|error| error.to_string())?;
    let shutdown = AtomicBool::new(false);
    let mut server = server;
    server
        .run(Box::new(Handler::new(state)), &shutdown, None)
        .map_err(|error| error.to_string())
}

/// A short path for the `AF_UNIX` socket. [`SOCKET_PATH_LIMIT`] keeps it inside the protocol's [`SOCKET_PATH_MAX`]
/// buffer, leaving the last byte for the terminator.
pub fn socket_path(prefix: &str) -> Option<PathBuf> {
    static SOCKETS: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        "{prefix}{}-{}.sock",
        std::process::id(),
        SOCKETS.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(name);
    (path.to_str()?.len() < SOCKET_PATH_LIMIT).then_some(path)
}

/// The folder of the session dumps.
fn dumps_dir(state: &Path) -> PathBuf {
    state.join(DUMPS)
}

/// Removes the marker and the in-process dump of the previous launch.
pub fn clear_previous(state: &Path) {
    let _ = fs::remove_file(state.join(LAST_DUMP_FILE));
    let _ = fs::remove_file(state.join(CRASH_DUMP_FILE));
}

/// The server handler: creates the dump file, remembers it, and names it in `last_dump`.
struct Handler {
    state: PathBuf,
    dir: PathBuf,
    sequence: AtomicU64,
}

impl Handler {
    fn new(state: &Path) -> Self {
        Self {
            state: state.to_path_buf(),
            dir: dumps_dir(state),
            sequence: AtomicU64::new(0),
        }
    }
}

impl ServerHandler for Handler {
    fn create_minidump_file(&self) -> io::Result<(File, PathBuf)> {
        fs::create_dir_all(&self.dir)?;
        prune(&self.dir);
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let path = self
            .dir
            .join(format!("crash-{}-{sequence}.dmp", now_millis()));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok((file, path))
    }

    fn on_minidump_created(&self, result: Result<MinidumpBinary, minidumper::Error>) -> LoopAction {
        match result {
            Ok(dump) => {
                eprintln!(
                    "gobbo: wrote the overlay crash dump {}",
                    dump.path.display()
                );
                let _ = fs::write(
                    self.state.join(LAST_DUMP_FILE),
                    dump.path.display().to_string(),
                );
            }
            Err(error) => {
                eprintln!("gobbo: could not write the overlay crash dump: {error}");
            }
        }

        LoopAction::Continue
    }

    fn on_message(&self, kind: u32, _buffer: Vec<u8>) {
        eprintln!("gobbo: overlay dumper message {kind}");
    }
}

/// Keeps the newest `KEEP_DUMPS` files in `dir`.
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let dumps: Vec<(PathBuf, SystemTime)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "dmp"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((entry.path(), modified))
        })
        .collect();

    for (path, _) in split_newest(dumps, KEEP_DUMPS).1 {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_short_socket_path_fits_the_protocol() {
        let path = socket_path("g").expect("the temporary folder gives a short path");
        assert!(path.to_string_lossy().len() < SOCKET_PATH_MAX);
        const { assert!(SOCKET_PATH_LIMIT < SOCKET_PATH_MAX) };
    }

    #[test]
    fn a_socket_path_beyond_the_protocol_is_refused() {
        assert!(socket_path(&"x".repeat(SOCKET_PATH_MAX)).is_none());
    }

    #[test]
    fn a_finished_dump_is_named_for_the_console() {
        let dir = std::env::temp_dir().join(format!("gobbo-dump-server-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let handler = Handler::new(&dir);
        let (file, path) = handler.create_minidump_file().expect("a dump file");
        drop(file);
        fs::write(&path, b"dump").unwrap();
        let created = MinidumpBinary {
            file: File::open(&path).unwrap(),
            path: path.clone(),
            contents: None,
        };
        assert!(matches!(
            handler.on_minidump_created(Ok(created)),
            LoopAction::Continue
        ));
        // The app reads the marker through `deploy::overlay::latest_dump`.
        let marker = fs::read_to_string(dir.join(LAST_DUMP_FILE)).unwrap();
        assert_eq!(marker.trim(), path.display().to_string());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_keeps_the_newest_dumps() {
        let dir = std::env::temp_dir().join(format!("gobbo-dumps-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let now = SystemTime::now();
        for number in 0..KEEP_DUMPS + 3 {
            let path = dir.join(format!("crash-{number}.dmp"));
            let file = fs::File::create(&path).unwrap();
            let age = Duration::from_secs((KEEP_DUMPS + 3 - number) as u64);
            file.set_modified(now - age).unwrap();
        }

        prune(&dir);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), KEEP_DUMPS);
        assert!(!dir.join("crash-0.dmp").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_breaks_an_mtime_tie_by_path() {
        let dir = std::env::temp_dir().join(format!("gobbo-dumps-tie-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let modified = SystemTime::now() - Duration::from_secs(1);
        for number in 0..=KEEP_DUMPS {
            let path = dir.join(format!("crash-{number:02}.dmp"));
            let file = fs::File::create(&path).unwrap();
            file.set_modified(modified).unwrap();
        }

        prune(&dir);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), KEEP_DUMPS);
        // Equal times keep the greater path, so the smallest one is pruned.
        assert!(!dir.join("crash-00.dmp").exists());
        assert!(dir.join(format!("crash-{KEEP_DUMPS:02}.dmp")).exists());
        let _ = fs::remove_dir_all(dir);
    }
}
