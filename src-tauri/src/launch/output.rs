//! The output of the game: what it writes to its standard output and its standard error. A launch sends both into one
//! pipe, and a thread copies the bytes into a file of the game's data folder. The console reads the end of that file.
//! The file holds the last launch only: a new launch replaces it.

use std::fs::File;
use std::io::{self, PipeReader, PipeWriter, Read, Write};
use std::path::{Path, PathBuf};

use super::Data;

/// The file in the game's data folder that holds the output of the last launch.
const OUTPUT_FILE: &str = "last_output.log";

/// The output is copied to the file up to this size. Past it the pipe is still drained, so the game never blocks on it.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// How many lines at the end of the file the console shows.
const TAIL_LINES: usize = 500;

/// A ceiling for one tail read: a binary log without newlines must not be walked to its start on every refresh.
const TAIL_BYTES: u64 = 256 * 1024;

/// The output file of a game.
pub fn file(data: &Data) -> PathBuf {
    data.dir.join(OUTPUT_FILE)
}

/// Starts the output of a launch of the game: the file is emptied, and the returned pipe takes what the game writes.
pub fn begin(data: &Data) -> io::Result<PipeWriter> {
    begin_at(&file(data))
}

/// Starts the output at `path`, replacing what it held. A thread copies from the pipe into the file until the game and
/// every process it started have closed the pipe.
pub fn begin_at(path: &Path) -> io::Result<PipeWriter> {
    let file = File::create(path)?;
    let (reader, writer) = io::pipe()?;
    std::thread::spawn(move || copy(reader, file));
    Ok(writer)
}

/// Copies the pipe into the file, up to `MAX_BYTES`. The rest is read and dropped, so the writer never waits for it.
fn copy(mut reader: PipeReader, mut file: File) {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written: u64 = 0;
    let mut full = false;

    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };

        if full {
            continue;
        }

        let room = usize::try_from(MAX_BYTES - written).unwrap_or(usize::MAX);
        let take = read.min(room);

        if file.write_all(&buffer[..take]).is_err() {
            full = true;
            continue;
        }

        written += take as u64;

        if written >= MAX_BYTES {
            full = true;
            let _ = file.write_all(b"\n[output stopped at 64 MB]\n");
        }
    }
}

/// The last lines of the last launch's output, for the console. Empty before the first launch.
pub fn tail(data: &Data) -> Vec<String> {
    tail_at(&file(data))
}

/// The last `TAIL_LINES` lines of the file at `path`, without carriage returns.
pub fn tail_at(path: &Path) -> Vec<String> {
    // The same tail reader as the log viewer in `infra::text`; it already drops a first line cut by the read.
    crate::infra::text::tail_lines_with_limit(path, TAIL_LINES, TAIL_BYTES).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::process::{Command, Stdio};

    /// Polls the file until the copy thread has written what the test waits for, for two seconds at most.
    fn wait_for(path: &Path, done: impl Fn(&[String]) -> bool) -> Vec<String> {
        for _ in 0..20 {
            let lines = tail_at(path);

            if done(&lines) {
                return lines;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        tail_at(path)
    }

    #[cfg(windows)]
    fn echo_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/c", "echo from the game & echo oops 1>&2"]);
        command
    }

    #[cfg(not(windows))]
    fn echo_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "echo from the game; echo oops 1>&2"]);
        command
    }

    #[test]
    fn a_program_writes_its_output_and_errors_into_the_file() {
        let dir = scratch!("program");
        let path = dir.join(OUTPUT_FILE);
        let writer = begin_at(&path).unwrap();
        let errors = writer.try_clone().unwrap();
        let mut command = echo_command();
        command
            .stdout(Stdio::from(writer))
            .stderr(Stdio::from(errors));
        let mut child = command.spawn().unwrap();
        // The command holds the write ends: it is dropped before waiting, so the reader sees the end of the output.
        drop(command);
        child.wait().unwrap();

        let lines = wait_for(&path, |lines| lines.len() >= 2);
        let trimmed: Vec<&str> = lines.iter().map(|line| line.trim()).collect();
        assert_eq!(trimmed, ["from the game", "oops"]);
    }

    #[test]
    fn only_the_end_of_a_long_output_is_shown() {
        let dir = scratch!("tail");
        let path = dir.join(OUTPUT_FILE);
        let mut writer = begin_at(&path).unwrap();

        for number in 0..600 {
            writeln!(writer, "line {number}").unwrap();
        }

        drop(writer);

        let lines = wait_for(&path, |lines| {
            lines.last().is_some_and(|line| line == "line 599")
        });
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("line 100"));
        assert_eq!(lines.last().map(String::as_str), Some("line 599"));
    }

    /// The game is started the way a launch starts it: suspended, with the hook, and with the pipe for its output.
    #[cfg(windows)]
    #[test]
    fn a_program_started_in_the_layers_writes_into_the_file() {
        let dir = scratch!("hooked");
        let path = dir.join(OUTPUT_FILE);
        let writer = begin_at(&path).unwrap();
        let game = dir.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let session = crate::deploy::overlay::Session::at(&dir.join("state"), &game);
        let mut running = crate::deploy::overlay::start(
            &session,
            r"C:\Windows\System32\cmd.exe",
            // `cmd /c` takes the command as one argument; quoting `echo` on its own turns it into an external command.
            &["/c".to_string(), "echo hooked-output".to_string()],
            &[],
            Some(writer),
        )
        .unwrap();
        assert_eq!(running.wait().unwrap(), 0);

        let lines = wait_for(&path, |lines| {
            lines.iter().any(|line| line.contains("hooked-output"))
        });
        assert!(lines.iter().any(|line| line.contains("hooked-output")));
    }

    #[test]
    fn a_missing_output_file_is_empty() {
        assert!(tail_at(Path::new("no-such-dir/last_output.log")).is_empty());
    }

    #[test]
    fn a_launch_writes_the_output_file_of_its_game_and_the_console_reads_it() {
        let dir = scratch!("file-of-game");
        let data = Data::new(dir.clone());
        assert_eq!(file(&data), dir.join(OUTPUT_FILE));

        let mut writer = begin(&data).unwrap();
        writeln!(writer, "first").unwrap();
        writeln!(writer, "second").unwrap();
        drop(writer);

        let lines = wait_for(&file(&data), |lines| {
            lines.last().is_some_and(|line| line == "second")
        });
        assert_eq!(lines, ["first", "second"]);
        assert_eq!(tail(&data), ["first", "second"]);
    }

    #[test]
    fn two_megabytes_of_output_are_all_kept() {
        let dir = scratch!("two-megabytes");
        let path = dir.join(OUTPUT_FILE);
        let mut writer = begin_at(&path).unwrap();
        let line = "x".repeat(1023);

        for _ in 0..2048 {
            writeln!(writer, "{line}").unwrap();
        }

        writeln!(writer, "end").unwrap();
        drop(writer);

        let lines = wait_for(&path, |lines| {
            lines.last().is_some_and(|line| line == "end")
        });
        assert_eq!(lines.last().map(String::as_str), Some("end"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("output stopped"));
    }

    #[test]
    fn output_stops_at_the_size_limit_and_says_so() {
        let dir = scratch!("size-limit");
        let path = dir.join(OUTPUT_FILE);
        let mut writer = begin_at(&path).unwrap();
        let chunk = vec![b'x'; 1024 * 1024 - 1];

        for _ in 0..65 {
            writer.write_all(&chunk).unwrap();
            writer.write_all(b"\n").unwrap();
        }

        drop(writer);

        let lines = wait_for(&path, |lines| {
            lines.iter().any(|line| line.contains("output stopped"))
        });
        assert!(lines.iter().any(|line| line == "[output stopped at 64 MB]"));
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size <= MAX_BYTES + 64 * 1024, "{size}");
    }

    #[test]
    fn a_tail_cut_from_the_middle_of_a_long_file_starts_at_a_whole_line() {
        let dir = scratch!("tail-whole-line");
        let path = dir.join(OUTPUT_FILE);
        let line = format!("line {}", "x".repeat(1000));
        std::fs::write(&path, format!("{line}\n").repeat(600)).unwrap();

        let lines = tail_at(&path);
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|line| line.starts_with("line ")));
    }
}
