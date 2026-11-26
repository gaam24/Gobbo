//! The whiteouts of a session: what the game deleted or renamed away from a lower layer. Every mount has its own set,
//! saved next to that mount's upper layer, so a deletion in one profile never hides a file in another.
//!
//! A change updates the in-memory snapshot and appends one line to `upper.whiteouts.journal`; it does not rewrite the
//! whole set. Every `COMPACT_AFTER` changes the journal is folded into `upper.whiteouts` with temp + rename, and the
//! journal is truncated. A reader takes a snapshot under a short read lock and probes the disk after the lock is gone.
//! A journal line is complete only when its newline is there, so a crash in the middle of a write leaves the previous
//! state, not a half path that hides an unrelated file.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use gobbo_overlay_core::{Layers, Whiteouts};
use gobbo_overlay_decisions::{Kind, Layer};

use crate::diag::record_decision;
use crate::state::Mount;
use crate::trace::{log, trace};

/// How many appended lines are folded into the snapshot at once. Small enough that the journal never grows far beyond
/// the set, large enough that a session with many deletions does not rewrite the whole set for each one.
const COMPACT_AFTER: usize = 512;

/// The append-only journal of one mount, opened once at install.
struct Journal {
    /// The open append handle.
    handle: File,
    /// The compacted whiteouts file this journal is folded into.
    snapshot: PathBuf,
    /// The journal file itself, reopened to truncate it after a compaction.
    path: PathBuf,
    /// The number of complete lines appended since the last compaction.
    lines: usize,
}

impl Journal {
    /// Appends one path. A failure disables the journal for the rest of the session; the in-memory snapshot keeps
    /// working, so the game still sees the deletion.
    fn write(&mut self, rel: &str) -> io::Result<()> {
        self.handle.write_all(rel.as_bytes())?;
        self.handle.write_all(b"\n")?;
        self.lines = self.lines.saturating_add(1);
        Ok(())
    }

    /// Finishes a compaction whose snapshot was written outside the journal lock. The captured line count is the one
    /// the snapshot covers; a later append changed it, so its line must stay in the journal for the next fold.
    fn finish_compact(&mut self, lines: usize) -> io::Result<()> {
        if self.lines != lines {
            return Ok(());
        }
        // The append handle has `FILE_APPEND_DATA` but not necessarily `FILE_WRITE_DATA`, and `set_len` wants the
        // latter. Reopen the journal for truncation instead of changing the open append handle.
        File::create(&self.path)?;
        self.lines = 0;
        Ok(())
    }
}

/// Writes `whiteouts` to `snapshot` through a temporary file, so a crash leaves either the old snapshot or the new one,
/// never a partial file.
fn write_snapshot(snapshot: &Path, whiteouts: &Whiteouts) -> io::Result<()> {
    let temporary = snapshot.with_extension("whiteouts.tmp");
    let result =
        fs::write(&temporary, whiteouts.to_text()).and_then(|()| fs::rename(&temporary, snapshot));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// The journal of each mount, keyed by the mount id. `None` inside means the journal could not be opened or failed;
/// the snapshot still works.
static JOURNALS: OnceLock<Mutex<HashMap<String, Option<Journal>>>> = OnceLock::new();

fn journals() -> &'static Mutex<HashMap<String, Option<Journal>>> {
    JOURNALS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Hides `rel` in the lower layers of `mount`, after a deletion or a rename. The in-memory set changes under a short
/// write lock; the journal append and the occasional compaction run after it.
pub(crate) fn hide(mount: &Mount, rel: &str) {
    trace(|| format!("hide {rel} in {}", mount.id));
    mount.update_whiteouts(|whiteouts| whiteouts.hide(rel));
    record_decision(Kind::Whiteout, Layer::None, 0, || mount.decision_path(rel));
    append(mount, rel);
}

/// The whiteouts saved for this mount: the snapshot plus the complete lines of its journal. A new journal is opened
/// for the next changes. A journal that was replayed is folded into the snapshot, so the next session starts compact.
pub(crate) fn load(id: &str, layers: &Layers) -> Whiteouts {
    let mut whiteouts = fs::read_to_string(layers.whiteouts_file())
        .map(|text| Whiteouts::parse(&text))
        .unwrap_or_default();
    let journal_path = layers.whiteouts_journal_file();
    let replayed = match fs::read(&journal_path) {
        Ok(bytes) => {
            // Only complete lines count. A final line without a newline is a write that a crash interrupted; cut it
            // off before the journal is opened for appending, or the next line would join the half path.
            let complete = bytes
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |at| at + 1);
            let text = String::from_utf8_lossy(&bytes[..complete]);
            let replayed = replay(&mut whiteouts, &text);

            if complete < bytes.len() {
                truncate_to(&journal_path, complete as u64);
            }

            replayed
        }
        Err(_) => 0,
    };

    if replayed > 0 {
        match write_snapshot(&layers.whiteouts_file(), &whiteouts) {
            Ok(()) => truncate(&journal_path),
            Err(error) => log(&format!("could not fold the whiteout journal: {error}")),
        }
    }

    let journal = open_journal(layers);
    let mut all = journals().lock().unwrap_or_else(PoisonError::into_inner);
    all.insert(id.to_string(), journal);
    whiteouts
}

/// Applies the complete lines of a journal to `whiteouts`, and returns how many it applied. A final line without a
/// newline is ignored: it is a write that a crash interrupted.
fn replay(whiteouts: &mut Whiteouts, text: &str) -> usize {
    let mut applied = 0;

    for line in text.split_inclusive('\n') {
        let Some(line) = line.strip_suffix('\n') else {
            continue;
        };
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        whiteouts.hide(line);
        applied += 1;
    }

    applied
}

/// Appends a path to the mount's journal and folds it into the snapshot when the journal is long enough. The append
/// holds the journal lock; the snapshot write and rename run after it is released, and truncation re-checks the line
/// count so a concurrent append is never dropped.
fn append(mount: &Mount, rel: &str) {
    let compaction = {
        let mut guard = journals().lock().unwrap_or_else(PoisonError::into_inner);
        let Some(journal) = guard.get_mut(&mount.id).and_then(Option::as_mut) else {
            return;
        };

        if let Err(error) = journal.write(rel) {
            log(&format!(
                "could not append to the whiteout journal: {error}"
            ));

            if let Some(entry) = guard.get_mut(&mount.id) {
                *entry = None;
            }

            return;
        }

        if journal.lines >= COMPACT_AFTER {
            // Read the published snapshot while the journal lock is held: every earlier append has already published
            // its change, so this snapshot covers all the lines the compaction is about to drop. A snapshot captured
            // by a caller before it took this lock can be older than those lines (another thread may have hidden a
            // path after it), and writing it would lose that path from disk while the journal is truncated.
            Some((
                journal.snapshot.clone(),
                mount.whiteouts_snapshot(),
                journal.lines,
            ))
        } else {
            None
        }
    };

    let Some((snapshot_path, snapshot, lines)) = compaction else {
        return;
    };

    // No global lock across the disk write: other threads keep appending while the snapshot is folded in.
    if let Err(error) = write_snapshot(&snapshot_path, &snapshot) {
        log(&format!("could not compact the whiteout journal: {error}"));
        return;
    }

    let mut guard = journals().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(journal) = guard.get_mut(&mount.id).and_then(Option::as_mut)
        && let Err(error) = journal.finish_compact(lines)
    {
        log(&format!("could not compact the whiteout journal: {error}"));
    }
}

/// Opens the journal for appending. A failure is logged and leaves the snapshot-only mode for this mount.
fn open_journal(layers: &Layers) -> Option<Journal> {
    let path = layers.whiteouts_journal_file();

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(handle) => Some(Journal {
            handle,
            snapshot: layers.whiteouts_file(),
            path,
            lines: 0,
        }),
        Err(error) => {
            log(&format!(
                "could not open the whiteout journal {}: {error}",
                path.display()
            ));
            None
        }
    }
}

/// Empties the journal after a successful fold. A failure is logged; the old lines are replayed again next time, which
/// is harmless because the set ignores duplicates.
fn truncate(path: &Path) {
    truncate_to(path, 0);
}

/// Sets the journal length, used to drop a final partial line and to empty it after a fold.
fn truncate_to(path: &Path, length: u64) {
    match OpenOptions::new().write(true).open(path) {
        Ok(file) => {
            if let Err(error) = file.set_len(length) {
                log(&format!(
                    "could not truncate the whiteout journal {}: {error}",
                    path.display()
                ));
            }
        }
        Err(error) => log(&format!(
            "could not open the whiteout journal {}: {error}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gobbo-whiteouts-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replay_ignores_an_interrupted_last_line() {
        let mut whiteouts = Whiteouts::default();
        assert_eq!(replay(&mut whiteouts, "a\\b\nc\\d\ninterrupted"), 2);
        assert!(whiteouts.hides(r"a\b"));
        assert!(whiteouts.hides(r"c\d"));
        assert!(!whiteouts.hides("interrupted"));
    }

    #[test]
    fn a_compaction_writes_the_current_snapshot_not_a_stale_one() {
        let dir = scratch("stale-compaction");
        let layers = Layers::new(dir.join("game"), None, dir.join("upper"));
        let mount = Mount::new(
            "stale-compaction".to_string(),
            layers.root.clone(),
            layers.clone(),
            Whiteouts::default(),
        );
        load(&mount.id, &layers);

        // A reader may hold an older snapshot while another thread publishes two new paths. A compaction triggered by
        // the later append must still write both paths: they are in the journal it is about to truncate.
        let _older = mount.whiteouts_snapshot();
        mount.update_whiteouts(|whiteouts| whiteouts.hide(r"first\file.txt"));
        mount.update_whiteouts(|whiteouts| whiteouts.hide(r"second\file.txt"));
        {
            let mut guard = journals().lock().unwrap();
            let journal = guard
                .get_mut(&mount.id)
                .and_then(Option::as_mut)
                .expect("the journal was opened by load");
            journal.lines = COMPACT_AFTER - 1;
        }
        append(&mount, r"second\file.txt");

        let saved = fs::read_to_string(layers.whiteouts_file()).unwrap();
        assert!(saved.contains(r"first\file.txt"), "{saved}");
        assert!(saved.contains(r"second\file.txt"), "{saved}");
        assert_eq!(
            fs::metadata(layers.whiteouts_journal_file()).unwrap().len(),
            0
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_compaction_writes_the_snapshot_and_empties_the_journal() {
        let dir = scratch("compact");
        let snapshot = dir.join("upper.whiteouts");
        let path = dir.join("upper.whiteouts.journal");
        let handle = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let mut journal = Journal {
            handle,
            snapshot: snapshot.clone(),
            path: path.clone(),
            lines: 0,
        };
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(r"data\a.txt");
        journal.write(r"data\a.txt").unwrap();
        let lines = journal.lines;
        write_snapshot(&snapshot, &whiteouts).unwrap();
        journal.finish_compact(lines).unwrap();
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), "data\\a.txt\n");
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_append_during_compaction_keeps_its_line_for_the_next_fold() {
        let dir = scratch("compact-ordering");
        let layers = Layers::new(dir.join("game"), None, dir.join("upper"));
        let mount = Mount::new(
            "compact-ordering".to_string(),
            layers.root.clone(),
            layers.clone(),
            Whiteouts::default(),
        );
        load(&mount.id, &layers);
        mount.update_whiteouts(|whiteouts| whiteouts.hide(r"early.txt"));

        // Capture a compaction at the threshold, then let another append land while the snapshot is off the lock.
        // The second append is below the threshold itself, so only the first one tries to fold the journal.
        let (snapshot_path, snapshot, captured_lines) = {
            let mut guard = journals().lock().unwrap();
            let journal = guard
                .get_mut(&mount.id)
                .and_then(Option::as_mut)
                .expect("the journal was opened by load");
            journal.lines = COMPACT_AFTER - 2;
            (
                journal.snapshot.clone(),
                mount.whiteouts_snapshot(),
                COMPACT_AFTER,
            )
        };
        append(&mount, r"late.txt");

        write_snapshot(&snapshot_path, &snapshot).unwrap();
        {
            let mut guard = journals().lock().unwrap();
            let journal = guard
                .get_mut(&mount.id)
                .and_then(Option::as_mut)
                .expect("the journal was opened by load");
            journal.finish_compact(captured_lines).unwrap();
            assert_eq!(journal.lines, COMPACT_AFTER - 1);
        }

        let saved = fs::read_to_string(layers.whiteouts_file()).unwrap();
        assert!(saved.contains(r"early.txt"), "{saved}");
        assert!(!saved.contains(r"late.txt"), "{saved}");
        let journal_text = fs::read_to_string(layers.whiteouts_journal_file()).unwrap();
        assert!(journal_text.contains(r"late.txt"), "{journal_text}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
