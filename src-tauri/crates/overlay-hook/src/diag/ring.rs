//! The decision ring: a small file-backed log of the mutations the hook decides about. The app reads the same files;
//! the writer is per process, so a game and its children never contend for one write index.
//!
//! The ring is off unless `GOBBO_OVERLAY_RING` names a base path (a dev build defaults to
//! `%LOCALAPPDATA%\Gobbo\overlay\overlay-decisions`). When it is off, a call site only reads a `OnceLock`; the path
//! string is built only when the ring is on, so the pass-through path keeps its allocation budget.

use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use gobbo_overlay_decisions::{Kind, Layer, Ring};

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

/// Appends one decision. `path` is built only when the ring is on.
pub(crate) fn record(kind: Kind, layer: Layer, error: u32, path: impl FnOnce() -> String) {
    if crate::diag::crashing() {
        return;
    }

    let Some(Some(lock)) = RING.get() else {
        return;
    };
    // Build the path before taking the ring lock: the decision path may call `to_string_lossy` or `format!`, and the
    // writer must not hold a process-wide lock during that work.
    let path = path();
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
    fn a_core_layer_maps_to_the_record_layer() {
        assert_eq!(layer(gobbo_overlay_core::Layer::Upper), Layer::Upper);
        assert_eq!(layer(gobbo_overlay_core::Layer::Staging), Layer::Staging);
        assert_eq!(layer(gobbo_overlay_core::Layer::Root), Layer::Root);
    }
}
