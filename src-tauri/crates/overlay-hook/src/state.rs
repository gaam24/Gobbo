//! The process-wide state of the hook: the mounts of the session (the game folder and the profile's user-data roots),
//! their whiteouts, and the detours. They are stored once, while the program starts, and every hooked call reads them
//! after that.
//!
//! A mount is a guest path the program uses plus the layer set that serves it. The game mount has a mod staging layer;
//! a user-data redirect has only the profile's upper layer and the real folder as its base. Every mount has its own
//! whiteout snapshot, so a deletion in one profile never hides a file in another.
//!
//! Readers of the whiteouts take a snapshot (`Arc<Whiteouts>`) under a short read lock and then drop the lock; a disk
//! probe never runs under the lock. A change takes the write lock only to insert the path and publishes the same
//! snapshot; the journal write happens after the lock is released.

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock, PoisonError, RwLock};

use gobbo_overlay_core::{Layers, MountSpec, Whiteouts};

use crate::detours::Hooks;
use crate::diag;

/// One mount of the session. `guest` is the path the program sees; `layers` serves it. The game mount's guest is the
/// game folder and its layers include the mod staging layer. A user-data mount's guest is the real user folder and its
/// layers have no staging layer, only the base and the profile's upper.
pub(crate) struct Mount {
    pub(crate) id: String,
    pub(crate) guest: PathBuf,
    pub(crate) layers: Layers,
    /// A change is published by replacing this `Arc`, never by mutating through it. A reader that caches the snapshot
    /// detects a newer one only with `Arc::ptr_eq`: that pointer check is the NT listing's sole staleness signal, so
    /// changing the set behind the same pointer would leave cached listings serving entries built from the old set.
    whiteouts: RwLock<Arc<Whiteouts>>,
}

impl Mount {
    /// A mount with its first whiteout snapshot.
    pub(crate) fn new(id: String, guest: PathBuf, layers: Layers, whiteouts: Whiteouts) -> Self {
        Self {
            id,
            guest,
            layers,
            whiteouts: RwLock::new(Arc::new(whiteouts)),
        }
    }

    /// The published snapshot. The lock is held only for the clone; the returned `Arc`'s pointer identifies the
    /// generation, so a cached snapshot is stale exactly when `Arc::ptr_eq` no longer holds against a fresh one.
    pub(crate) fn whiteouts_snapshot(&self) -> Arc<Whiteouts> {
        crate::stat_lock();
        self.whiteouts
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Reads the whiteouts without holding the lock during `read`.
    pub(crate) fn with_whiteouts<R>(&self, read: impl FnOnce(&Whiteouts) -> R) -> R {
        read(&self.whiteouts_snapshot())
    }

    /// Mutates the snapshot through `Arc::make_mut` and returns the published set. When the set is shared this replaces
    /// the `Arc`, so readers holding the old pointer can tell it is stale. The write lock is held only for the change;
    /// the caller does file I/O after it returns.
    pub(crate) fn update_whiteouts(&self, change: impl FnOnce(&mut Whiteouts)) -> Arc<Whiteouts> {
        let mut snapshot = self
            .whiteouts
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        change(Arc::make_mut(&mut snapshot));
        Arc::clone(&snapshot)
    }

    /// Whether a read from the mount's base can keep the caller's own path. Only the game mount and an inherited
    /// user-data redirect have `guest == root`; a redirect that starts empty has an internal base folder.
    pub(crate) fn guest_is_root(&self) -> bool {
        // This crate only builds for Windows, so the folded comparison is always the one that applies.
        gobbo_overlay_core::same_folded(
            &self.guest.to_string_lossy(),
            &self.layers.root.to_string_lossy(),
        )
    }

    /// The path a decision record carries. The game keeps its game-relative path; a redirect is prefixed with its set
    /// id, so the report can say which profile layer was touched.
    pub(crate) fn decision_path(&self, rel: &str) -> String {
        if self.id == MountSpec::GAME_ID {
            rel.to_string()
        } else {
            format!("@{}\\{rel}", self.id)
        }
    }
}

impl Deref for Mount {
    type Target = Layers;

    fn deref(&self) -> &Self::Target {
        &self.layers
    }
}

/// Every mount of the session. The game mount is always first and always present.
pub(crate) struct Mounts {
    pub(crate) game: Mount,
    pub(crate) redirects: Vec<Mount>,
}

impl Mounts {
    /// The game mount followed by the redirects, in the order they are checked.
    pub(crate) fn all(&self) -> impl Iterator<Item = &Mount> {
        std::iter::once(&self.game).chain(self.redirects.iter())
    }
}

static MOUNTS: OnceLock<Mounts> = OnceLock::new();
/// The empty snapshot used when no session exists; it is never mutated.
static EMPTY_WHITEOUTS: LazyLock<Arc<Whiteouts>> = LazyLock::new(|| Arc::new(Whiteouts::default()));
static HOOKS: OnceLock<Hooks> = OnceLock::new();

/// Stores the mounts of the session. Called once, before the detours are created.
pub(crate) fn store_session(mounts: Mounts) {
    let _ = MOUNTS.set(mounts);
}

/// Stores the detours. Called once, before they are enabled, so a hooked call can always find them.
pub(crate) fn store_hooks(hooks: Hooks) {
    let _ = HOOKS.set(hooks);
}

/// The detours, once `store_hooks` has run. The install stores the table before it enables any detour, so a hooked call
/// cannot arrive with it missing; a caller that gets `None` returns the API's failure instead of panicking.
pub(crate) fn hooks() -> Option<&'static Hooks> {
    HOOKS.get()
}

/// The mounts of the session, or `None` when the environment named none.
pub(crate) fn mounts() -> Option<&'static Mounts> {
    // `GOBBO_OVERLAY_TEST_PASSTHROUGH` (see `diag`) makes every call pass through, with the hook still loaded.
    if diag::settings().passthrough {
        return None;
    }

    MOUNTS.get()
}

/// The game mount, or `None` when the environment named no layers.
fn game() -> Option<&'static Mount> {
    mounts().map(|mounts| &mounts.game)
}

/// The layers of the game mount, for the code that only works inside the game folder.
pub(crate) fn layers() -> Option<&'static Layers> {
    game().map(|mount| &mount.layers)
}

/// The published whiteout snapshot of the game mount.
pub(crate) fn whiteouts_snapshot() -> Arc<Whiteouts> {
    match game() {
        Some(mount) => mount.whiteouts_snapshot(),
        None => Arc::clone(&EMPTY_WHITEOUTS),
    }
}
