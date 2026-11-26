//! The decision ring: a small file-backed log of the mutations the hook decides about. The app reads the same files;
//! the writer is per process, so a game and its children never contend for one write index.
//!
//! The ring is off unless `GOBBO_OVERLAY_RING` names a base path (a dev build defaults to
//! `%LOCALAPPDATA%\Gobbo\overlay\overlay-decisions`). When it is off, a call site only reads a `OnceLock`; the path
//! string is built only when the ring is on, so the pass-through path keeps its allocation budget.

use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use gobbo_overlay_decisions::{Kind, Layer, Ring};

use super::writer::reporting;

/// The ring of this process, or `None` when the switch is off.
static RING: OnceLock<Option<Mutex<Ring>>> = OnceLock::new();

/// Opens the ring of this process under `base`: `<base>.<pid>.ring`. The app removes the old files before a launch.
pub(crate) fn open(base: Option<&Path>) {
    let ring = base.and_then(|base| {
        let Some(path) = gobbo_overlay_decisions::process_file(base) else {
            crate::trace::log("the decision ring base has no file name");
            return None;
        };

        match Ring::open(&path) {
            Ok(ring) => Some(Mutex::new(ring)),
            Err(error) => {
                crate::trace::log(&format!(
                    "could not open the decision ring {}: {error}",
                    path.display()
                ));
                None
            }
        }
    });
    let _ = RING.set(ring);
}

/// Appends one decision. `path` is built only when the ring is on. Decisions pause only while a crash report or a
/// trace batch is being appended: that writer may hold a lock the ring path must not contend with. A handled
/// first-chance exception must not stop the ring for the rest of the process.
pub(crate) fn record(kind: Kind, layer: Layer, error: u32, path: impl FnOnce() -> String) {
    if reporting() {
        return;
    }

    let Some(Some(lock)) = RING.get() else {
        return;
    };
    // Build the path before taking the ring lock: the decision path may call `to_string_lossy` or `format!`, and the
    // writer must not hold a process-wide lock during that work.
    let path = path();
    // This diagnostics-only lock covers two file writes and serializes the writers of one ring file. It is a documented
    // deviation from the no-mutex-across-I/O rule; see the deviation records in docs/diagnostics.md.
    let mut ring = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = ring.record(kind, layer, error, &path, crate::win32::thread_id());
}

/// The decision layer of a core layer.
pub(crate) fn layer(layer: gobbo_overlay_core::Layer) -> Layer {
    match layer {
        gobbo_overlay_core::Layer::Upper => Layer::Upper,
        gobbo_overlay_core::Layer::Staging => Layer::Staging,
        gobbo_overlay_core::Layer::Root => Layer::Root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_core_layer_maps_to_the_same_decision_layer() {
        assert_eq!(
            layer(gobbo_overlay_core::Layer::Upper),
            gobbo_overlay_decisions::Layer::Upper
        );
        assert_eq!(
            layer(gobbo_overlay_core::Layer::Staging),
            gobbo_overlay_decisions::Layer::Staging
        );
        assert_eq!(
            layer(gobbo_overlay_core::Layer::Root),
            gobbo_overlay_decisions::Layer::Root
        );
    }

    /// Decisions must resume after an append finishes, so a first-chance exception the program handles does not stop
    /// the ring for the rest of the process. This test is the only one that opens the global ring, but other tests
    /// append to it and can briefly hold the same append suppression, so a wanted record is retried until it appears.
    /// The assertions still require the record inside the held window to be absent and the later one to be present and
    /// ordered after the first; unrelated paths from other tests are ignored.
    #[test]
    fn decisions_resume_after_an_append_finishes() {
        let dir = std::env::temp_dir().join(format!("gobbo-ring-suppress-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("ring");
        open(Some(&base));
        let file = gobbo_overlay_decisions::process_file(&base).unwrap();

        let paths = || {
            let (decisions, _) = gobbo_overlay_decisions::read_last(&file, usize::MAX).unwrap();
            decisions
                .iter()
                .map(|decision| decision.path.clone())
                .collect::<Vec<_>>()
        };
        let record_until_seen = |name: &str| {
            for _ in 0..100 {
                record(Kind::Write, Layer::Upper, 0, || name.to_string());
                if paths().iter().any(|path| path == name) {
                    return;
                }
            }
            panic!("{name} was never recorded after the append finished");
        };

        record_until_seen("kept.txt");
        crate::diag::writer::hold_append(true);
        record(Kind::Write, Layer::Upper, 0, || "skipped.txt".into());
        crate::diag::writer::hold_append(false);
        record_until_seen("after.txt");

        let written = paths();
        let position = |name: &str| written.iter().position(|path| path == name);
        let kept = position("kept.txt");
        let after = position("after.txt");
        assert!(kept.is_some(), "kept.txt was not recorded");
        assert!(after.is_some(), "after.txt was not recorded");
        assert!(
            position("skipped.txt").is_none(),
            "a decision was recorded while an append was in progress"
        );
        assert!(kept < after, "after.txt was recorded before kept.txt");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_record_layer_survives_the_ring() {
        let dir = std::env::temp_dir().join(format!("gobbo-ring-layer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ring");
        let mut ring = Ring::open(&path).unwrap();
        ring.record(Kind::Write, Layer::Staging, 0, r"data\a.txt", 1)
            .unwrap();
        let (decisions, _) = gobbo_overlay_decisions::read_last(&path, 1).unwrap();
        assert_eq!(decisions[0].kind, Kind::Write);
        assert_eq!(decisions[0].layer, Layer::Staging);
        assert_eq!(decisions[0].path, r"data\a.txt");
        let _ = std::fs::remove_dir_all(dir);
    }
}
