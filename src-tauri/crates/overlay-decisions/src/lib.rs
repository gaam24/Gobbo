//! A small file-backed ring of overlay decisions, shared by the Windows hook (the writer) and Gobbo (the reader).
//!
//! The file has a fixed header and a fixed number of fixed-size records. The writer appends a record and then moves the
//! header's write index, so a reader only sees a record once it is complete. Old records are overwritten when the ring
//! wraps; `read_last` reads at most the newest records and skips a record that was overwritten while it was read.
//!
//! Each process writes its own file, `<base>.<pid>.ring`, so a game and its children never contend for one write index.
//! `read_base` merges the files of all processes and `clear` removes the files of an earlier launch.
//!
//! The crate is plain `std`, with no Windows calls, so the app can read the files on every platform.

#![forbid(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;

/// The environment variable that names the base path of the ring. The hook appends its process id and `.ring`.
pub const ENV_RING: &str = "GOBBO_OVERLAY_RING";

/// The stem of the ring base both Gobbo's app and the hook's developer default build. The app joins it to the session
/// state folder; a dev build of the hook joins it to the developer folder.
pub const BASE_STEM: &str = "overlay-decisions";

/// The file that names the newest out-of-process minidump of a session. The dumper writes it and the launch report reads
/// it.
pub const LAST_DUMP_FILE: &str = "last_dump";

/// The in-process `DbgHelp` dump of a session, used when no out-of-process monitor is available.
pub const CRASH_DUMP_FILE: &str = "crash.dmp";

/// The environment variable that names the x64 minidumper socket. Gobbo starts the x64 server and exports this name.
pub const ENV_DUMPER: &str = "GOBBO_OVERLAY_DUMPER";

/// The environment variable that names the 32-bit minidumper socket. Gobbo starts the helper and exports this name.
pub const ENV_DUMPER32: &str = "GOBBO_OVERLAY_DUMPER32";

/// The environment variable that names the in-process `DbgHelp` dump. Gobbo passes it to the game; the hook reads it.
pub const ENV_IN_PROCESS_DUMP: &str = "GOBBO_OVERLAY_DUMP";

/// The environment variable that names the hook's trace file.
pub const ENV_LOG: &str = "GOBBO_OVERLAY_LOG";
/// The environment variable that names the hook's call-count report.
pub const ENV_DIAG: &str = "GOBBO_OVERLAY_DIAG";
/// The environment variable that names the hook's crash report.
pub const ENV_CRASH: &str = "GOBBO_OVERLAY_CRASH";
/// The environment variable that names the hook's session report.
pub const ENV_SESSION: &str = "GOBBO_OVERLAY_SESSION";
/// The environment variable naming the detours the hook leaves off.
pub const ENV_DISABLE: &str = "GOBBO_OVERLAY_DISABLE";
/// The environment variable that turns on first-chance exception reports.
pub const ENV_CRASH_FIRST: &str = "GOBBO_OVERLAY_CRASH_FIRST";
/// The environment variable that turns on per-call tracing and counting.
pub const ENV_CALLS: &str = "GOBBO_OVERLAY_CALLS";
/// The environment variable that traces writes, deletes and renames outside the layers.
pub const ENV_PASS: &str = "GOBBO_OVERLAY_PASS";
/// The environment variable that makes every hook call pass through.
pub const ENV_TEST_PASSTHROUGH: &str = "GOBBO_OVERLAY_TEST_PASSTHROUGH";
/// The environment variable that allows a child that could not be hooked to run.
pub const ENV_ALLOW_UNHOOKED_CHILDREN: &str = "GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN";
/// The environment variable that restores thread counting in the enable transaction.
pub const ENV_SAFE_THREADS: &str = "GOBBO_OVERLAY_SAFE_THREADS";

/// The magic and version at the start of the header. `Ring::open` recreates a file carrying other bytes, so a changed
/// layout is never read with the old record offsets.
const MAGIC: &[u8; 8] = b"GBODEC01";
const VERSION: u32 = 1;
const HEADER: usize = 64;
const RECORD: usize = 256;
/// The number of fixed-size records in a ring file. The crate owns this number; the hook, the app and benchmarks share it.
pub const CAPACITY: u32 = 4096;
const PATH_AT: usize = 32;
const PATH_MAX: usize = RECORD - PATH_AT;
const WRITE_AT: usize = 24;

/// Generates the stable lowercase `name` and the `from_u8` decoder of a decision enum.
macro_rules! enum_code {
    ($name:ident, $($variant:ident => $code:literal, $text:literal),* $(,)?) => {
        impl $name {
            /// The stable lowercase name, for the app and the log.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)*
                }
            }

            const fn from_u8(value: u8) -> Option<Self> {
                match value {
                    $($code => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

/// What a decision is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// A write went to the upper layer.
    Write = 1,
    /// A lower file was copied to the upper layer before a write.
    CopyUp = 2,
    /// A lower entry was hidden by a whiteout.
    Whiteout = 3,
    /// No layer shows the file.
    Missing = 4,
    /// The call was refused, with the error in `error`.
    Refuse = 5,
    /// A mutation passed through because its path is outside the game folder.
    PassThrough = 6,
    /// A directory listing was merged from the layers.
    List = 7,
}

enum_code!(Kind,
    Write => 1, "write",
    CopyUp => 2, "copy_up",
    Whiteout => 3, "whiteout",
    Missing => 4, "missing",
    Refuse => 5, "refuse",
    PassThrough => 6, "pass_through",
    List => 7, "list",
);

/// The layer a decision resolved to.
///
/// `None` is a record-only value: a refusal or a pass-through has no layer. The hook translates its `overlay-core` layer
/// into this enum, so the two enumerations must stay in step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Layer {
    None = 0,
    Upper = 1,
    Staging = 2,
    Root = 3,
}

enum_code!(Layer,
    None => 0, "none",
    Upper => 1, "upper",
    Staging => 2, "staging",
    Root => 3, "root",
);

/// One decision read back from the ring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub seq: u64,
    pub time_ms: u64,
    pub pid: u32,
    pub thread: u32,
    pub kind: Kind,
    pub layer: Layer,
    pub error: u32,
    pub path: String,
}

/// The writer side: a file kept open for the run.
pub struct Ring {
    file: File,
    write: u64,
    pid: u32,
}

impl Ring {
    /// Opens `path`, creating or truncating the ring when it is missing or has another format or version. A new ring
    /// has no decisions; the app removes old ring files before a launch.
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let length = file.metadata()?.len();
        let mut stored = [0u8; HEADER];

        if length >= HEADER as u64 {
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut stored)?;
        }

        let version = u32::from_le_bytes([stored[8], stored[9], stored[10], stored[11]]);
        if length != ring_length() || stored[..8] != MAGIC[..] || version != VERSION {
            file.set_len(ring_length())?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&header(0))?;
            return Ok(Self {
                file,
                write: 0,
                pid: std::process::id(),
            });
        }

        file.seek(SeekFrom::Start(WRITE_AT as u64))?;
        let mut bytes = [0u8; 8];
        file.read_exact(&mut bytes)?;
        Ok(Self {
            file,
            write: u64::from_le_bytes(bytes),
            pid: std::process::id(),
        })
    }

    /// Appends one decision. The record is written before the header index, so a reader never sees a partial record as
    /// the newest one. A path longer than the record is shortened at a byte boundary; the reader still decodes it.
    pub fn record(
        &mut self,
        kind: Kind,
        layer: Layer,
        error: u32,
        path: &str,
        thread: u32,
    ) -> io::Result<()> {
        let seq = self.write;
        let slot = (seq % u64::from(CAPACITY)) as usize;
        let mut record = [0u8; RECORD];
        record[..8].copy_from_slice(&seq.to_le_bytes());
        record[8..16].copy_from_slice(&now_millis().to_le_bytes());
        record[16..20].copy_from_slice(&thread.to_le_bytes());
        record[20..24].copy_from_slice(&self.pid.to_le_bytes());
        record[24] = kind as u8;
        record[25] = layer as u8;
        record[28..32].copy_from_slice(&error.to_le_bytes());
        let bytes = path.as_bytes();
        let mut length = bytes.len().min(PATH_MAX);
        // A cut inside a multi-byte character would make the reader show a replacement character. Keep the prefix
        // that is valid UTF-8; the record still says how many bytes it holds.
        while length > 0 && !path.is_char_boundary(length) {
            length -= 1;
        }

        record[26..28].copy_from_slice(&(length as u16).to_le_bytes());
        record[PATH_AT..PATH_AT + length].copy_from_slice(&bytes[..length]);
        write_at(&self.file, &record, record_offset(slot))?;
        let next = seq.wrapping_add(1);
        write_at(&self.file, &next.to_le_bytes(), WRITE_AT as u64)?;
        self.write = next;
        Ok(())
    }
}

/// The file of this process under `base`: `<name>.<pid>.ring`.
///
/// Returns `None` when `base` has no file name, the same rule the readers use, so a ring is never written where
/// nothing can read it back.
pub fn process_file(base: &Path) -> Option<PathBuf> {
    let name = base.file_name()?;
    Some(base.with_file_name(format!(
        "{}.{}.ring",
        name.to_string_lossy(),
        std::process::id()
    )))
}

/// Reads the newest `limit` decisions of all processes whose files start with `base`, and the total number recorded.
/// A missing folder or file gives an empty list; an unreadable file is skipped.
pub fn read_base(base: &Path, limit: usize) -> io::Result<(Vec<Decision>, u64)> {
    let Some((dir, prefix)) = split_base(base) else {
        return Ok((Vec::new(), 0));
    };
    let mut all = Vec::new();
    let mut total = 0u64;

    for path in ring_files(dir, &prefix)? {
        let Ok((mut decisions, written)) = read_last(&path, limit) else {
            continue;
        };
        total = total.saturating_add(written);
        all.append(&mut decisions);
    }

    all.sort_by_key(|decision| (decision.time_ms, decision.pid, decision.seq));

    if all.len() > limit {
        all.drain(..all.len() - limit);
    }

    Ok((all, total))
}

/// Removes the ring files of an earlier launch of `base`. Missing files are ignored.
pub fn clear(base: &Path) -> io::Result<()> {
    let Some((dir, prefix)) = split_base(base) else {
        return Ok(());
    };

    for path in ring_files(dir, &prefix)? {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

/// The paths of the ring files of `base` in `dir`, matching the ring name rule.
fn ring_files(dir: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };

        if !matches_ring(name, prefix) {
            continue;
        }

        files.push(entry.path());
    }

    Ok(files)
}

/// Reads the newest `limit` decisions from `path`, and the total number recorded in this ring. A file that does not
/// exist, or has another format, gives an empty list.
pub fn read_last(path: &Path, limit: usize) -> io::Result<(Vec<Decision>, u64)> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(error) => return Err(error),
    };
    let length = file.metadata()?.len();

    if length != ring_length() {
        return Ok((Vec::new(), 0));
    }

    let mut header = [0u8; HEADER];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut header)?;

    if &header[..8] != MAGIC {
        return Ok((Vec::new(), 0));
    }

    let write = u64::from_le_bytes(header[WRITE_AT..WRITE_AT + 8].try_into().unwrap_or([0; 8]));
    let oldest = write.saturating_sub(u64::from(CAPACITY));
    let start = write.saturating_sub(limit as u64).max(oldest);
    let count = usize::try_from(write - start).unwrap_or(0);

    if count == 0 {
        return Ok((Vec::new(), write));
    }
    // Read the requested slots in one contiguous run (two when the ring wraps), then decode from memory. The old
    // loop paid one seek and one read per record.
    let mut raw = vec![0u8; count * RECORD];
    let first_slot = (start % u64::from(CAPACITY)) as usize;
    let first_count = count.min(CAPACITY as usize - first_slot);
    file.seek(SeekFrom::Start(record_offset(first_slot)))?;
    file.read_exact(&mut raw[..first_count * RECORD])?;

    if first_count < count {
        file.seek(SeekFrom::Start(record_offset(0)))?;
        file.read_exact(&mut raw[first_count * RECORD..])?;
    }

    let mut decisions = Vec::with_capacity(count);

    for index in 0..count {
        let record = &raw[index * RECORD..(index + 1) * RECORD];
        let seq = start + index as u64;

        if u64::from_le_bytes(record[..8].try_into().unwrap_or([0; 8])) != seq {
            // The writer overwrote this slot while we read it; skip that decision.
            continue;
        }

        let Some(kind) = Kind::from_u8(record[24]) else {
            continue;
        };
        let Some(layer) = Layer::from_u8(record[25]) else {
            continue;
        };
        let path_length = usize::from(u16::from_le_bytes([record[26], record[27]])).min(PATH_MAX);
        let path = String::from_utf8_lossy(&record[PATH_AT..PATH_AT + path_length]).into_owned();
        decisions.push(Decision {
            seq,
            time_ms: u64::from_le_bytes(record[8..16].try_into().unwrap_or([0; 8])),
            thread: u32::from_le_bytes(record[16..20].try_into().unwrap_or([0; 4])),
            pid: u32::from_le_bytes(record[20..24].try_into().unwrap_or([0; 4])),
            kind,
            layer,
            error: u32::from_le_bytes(record[28..32].try_into().unwrap_or([0; 4])),
            path,
        });
    }

    Ok((decisions, write))
}

/// The size of a ring file: the header and every record slot.
const fn ring_length() -> u64 {
    (HEADER + RECORD * CAPACITY as usize) as u64
}

/// The folder and the file-name prefix shared by the ring files of `base`.
fn split_base(base: &Path) -> Option<(&Path, String)> {
    let dir = base.parent()?;
    let stem = base.file_name()?.to_str()?;
    Some((dir, format!("{stem}.")))
}

/// Whether `name` is `<prefix><pid>.ring`.
fn matches_ring(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    let Some(pid) = rest.strip_suffix(".ring") else {
        return false;
    };
    !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit())
}

/// Writes at an offset without moving the file cursor.
trait WriteAt {
    fn write_at_offset(&self, buffer: &[u8], offset: u64) -> io::Result<usize>;
}

#[cfg(windows)]
impl WriteAt for File {
    fn write_at_offset(&self, buffer: &[u8], offset: u64) -> io::Result<usize> {
        self.seek_write(buffer, offset)
    }
}

#[cfg(unix)]
impl WriteAt for File {
    fn write_at_offset(&self, buffer: &[u8], offset: u64) -> io::Result<usize> {
        FileExt::write_at(self, buffer, offset)
    }
}

/// Writes all of `buffer` at `offset` without moving the file cursor. On Windows this is one `WriteFile` with an
/// `OVERLAPPED` offset; the old `seek` + `write_all` pair paid two calls for each of the record and the header.
fn write_at(file: &File, buffer: &[u8], offset: u64) -> io::Result<()> {
    #[cfg(any(windows, unix))]
    {
        let mut written = 0;

        while written < buffer.len() {
            let count = file.write_at_offset(&buffer[written..], offset + written as u64)?;

            if count == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }

            written += count;
        }

        Ok(())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let mut file = file;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(buffer)
    }
}

/// The offset of the record in `slot`.
const fn record_offset(slot: usize) -> u64 {
    (HEADER + slot * RECORD) as u64
}

/// The header of a new ring with the write index `write`.
fn header(write: u64) -> [u8; HEADER] {
    let mut header = [0u8; HEADER];
    header[..8].copy_from_slice(MAGIC);
    header[8..12].copy_from_slice(&VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&(RECORD as u32).to_le_bytes());
    header[16..20].copy_from_slice(&CAPACITY.to_le_bytes());
    header[WRITE_AT..WRITE_AT + 8].copy_from_slice(&write.to_le_bytes());
    header
}

/// The current wall-clock time in milliseconds, for the ring's records and for file names the writer shares.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// A value paired with its modification time, for pruning.
type Dated<T> = (T, SystemTime);

/// Splits `files` into the `keep` newest and the rest.
///
/// An equal modification time is broken by path, greater first, so every consumer prunes the same files. Any further
/// rule, such as an age limit, is applied by the caller to the returned kept files.
pub fn split_newest<T: Ord>(
    mut files: Vec<Dated<T>>,
    keep: usize,
) -> (Vec<Dated<T>>, Vec<Dated<T>>) {
    files.sort_by(|(left_path, left_time), (right_path, right_time)| {
        right_time
            .cmp(left_time)
            .then_with(|| right_path.cmp(left_path))
    });
    let removed = files.split_off(keep.min(files.len()));
    (files, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gobbo-decisions-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_record_survives_the_round_trip() {
        let dir = scratch("round-trip");
        let path = dir.join("ring");
        let mut ring = Ring::open(&path).unwrap();
        ring.record(Kind::Write, Layer::Upper, 0, r"data\a.txt", 7)
            .unwrap();
        let (decisions, total) = read_last(&path, 10).unwrap();
        assert_eq!(total, 1);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].kind, Kind::Write);
        assert_eq!(decisions[0].layer, Layer::Upper);
        assert_eq!(decisions[0].path, r"data\a.txt");
        assert_eq!(decisions[0].thread, 7);
        assert_eq!(decisions[0].pid, std::process::id());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_long_path_is_cut_on_a_character_boundary() {
        let dir = scratch("utf8-cut");
        let path = dir.join("ring");
        let mut ring = Ring::open(&path).unwrap();
        let text = format!("{}Ś", "a".repeat(PATH_MAX - 1));
        ring.record(Kind::Write, Layer::Upper, 0, &text, 1).unwrap();
        let (decisions, _) = read_last(&path, 1).unwrap();
        assert_eq!(decisions[0].path.len(), PATH_MAX - 1);
        assert!(!decisions[0].path.contains('\u{FFFD}'));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn read_last_returns_the_newest_decisions_and_the_total() {
        let dir = scratch("newest");
        let path = dir.join("ring");
        let mut ring = Ring::open(&path).unwrap();

        for number in 0..3 {
            ring.record(Kind::Write, Layer::Upper, 0, &number.to_string(), 1)
                .unwrap();
        }

        let (decisions, total) = read_last(&path, 2).unwrap();
        assert_eq!(total, 3);
        assert_eq!(
            decisions
                .iter()
                .map(|decision| decision.path.as_str())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_ring_wraps_and_keeps_the_newest_capacity_records() {
        let dir = scratch("wrap");
        let path = dir.join("ring");
        let mut ring = Ring::open(&path).unwrap();

        for number in 0..CAPACITY + 3 {
            ring.record(Kind::Write, Layer::Upper, 0, &number.to_string(), 1)
                .unwrap();
        }

        let (decisions, total) = read_last(&path, usize::MAX).unwrap();
        assert_eq!(total, u64::from(CAPACITY) + 3);
        assert_eq!(decisions.len(), CAPACITY as usize);
        assert_eq!(decisions[0].path, "3");
        assert_eq!(decisions.last().unwrap().path, (CAPACITY + 2).to_string());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_with_another_format_is_recreated_by_open() {
        let dir = scratch("format");
        let path = dir.join("ring");
        std::fs::write(&path, b"not a ring").unwrap();
        let mut ring = Ring::open(&path).unwrap();
        ring.record(Kind::Missing, Layer::None, 2, "missing", 1)
            .unwrap();
        let (decisions, _) = read_last(&path, 10).unwrap();
        assert_eq!(decisions.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_files_of_all_processes_are_merged_in_time_order() {
        let dir = scratch("merge");
        let base = dir.join("overlay-decisions");
        let first = dir.join("overlay-decisions.111.ring");
        let second = dir.join("overlay-decisions.222.ring");
        Ring::open(&first)
            .unwrap()
            .record(Kind::Write, Layer::Upper, 0, "first", 1)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        Ring::open(&second)
            .unwrap()
            .record(Kind::CopyUp, Layer::Staging, 0, "second", 2)
            .unwrap();
        let (decisions, total) = read_base(&base, 10).unwrap();
        assert_eq!(total, 2);
        assert_eq!(
            decisions
                .iter()
                .map(|decision| decision.path.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn clear_removes_only_the_ring_files_of_the_base() {
        let dir = scratch("clear");
        let base = dir.join("overlay-decisions");
        let ring = dir.join("overlay-decisions.111.ring");
        let other = dir.join("other.111.ring");
        let not_a_ring = dir.join("overlay-decisions.notes.txt");

        for path in [&ring, &other, &not_a_ring] {
            std::fs::write(path, b"x").unwrap();
        }

        clear(&base).unwrap();
        assert!(!ring.exists());
        assert!(other.exists());
        assert!(not_a_ring.exists());
        let own = dir.join(format!("overlay-decisions.{}.ring", std::process::id()));
        assert_eq!(process_file(&base), Some(own));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_stem_less_base_is_refused_by_both_the_write_and_read_paths() {
        let base = Path::new("");
        let from_writer = process_file(base);
        let from_reader = split_base(base);

        assert!(from_writer.is_none());
        assert!(from_reader.is_none());
        let (decisions, total) = read_base(base, 10).unwrap();
        assert!(decisions.is_empty());
        assert_eq!(total, 0);
        clear(base).unwrap();
    }

    #[test]
    fn a_ring_with_a_different_version_is_recreated_by_open() {
        let dir = scratch("version");
        let path = dir.join("ring");
        let mut ring = Ring::open(&path).unwrap();
        ring.record(Kind::Write, Layer::Upper, 0, "before", 1)
            .unwrap();
        drop(ring);

        // Keep the length and magic intact so only the stored version can force a recreate.
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(8)).unwrap();
        file.write_all(&(VERSION + 1).to_le_bytes()).unwrap();
        drop(file);

        Ring::open(&path).unwrap();
        let (decisions, total) = read_last(&path, 10).unwrap();
        assert_eq!(total, 0);
        assert!(decisions.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_name_that_is_not_a_pid_is_not_read() {
        assert!(matches_ring("base.123.ring", "base."));
        assert!(!matches_ring("base.abc.ring", "base."));
        assert!(!matches_ring("base.123.tmp", "base."));
        assert!(!matches_ring("other.123.ring", "base."));
    }
}
