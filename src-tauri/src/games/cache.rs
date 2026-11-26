//! The parsed-definition cache: one shared value per unchanged `game.toml`, invalidated when a definition is written.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::error::AppError;

use super::definition::GameDef;
use super::dirs::loaders_dir;
use super::load::read_definition;

/// Parsed definitions by path, keyed by length and modification time. The cache keeps one shared `Arc` per unchanged
/// `game.toml`, so the hot read paths hand the same value to every caller.
static DEFINITIONS: OnceLock<crate::infra::file_cache::FileCache<GameDef>> = OnceLock::new();

/// Drops the cached parse of a definition that was just written, so the next read sees the new file even when the
/// clock resolution makes its length and modification time look unchanged.
pub fn forget_definition(path: &Path) {
    if let Some(cache) = DEFINITIONS.get() {
        cache.invalidate(path);
    }
}

/// Drops every parsed definition. A user loader preset is merged into each game that names it, and the cache is keyed
/// by `game.toml` only, so a written preset must reach every game that uses it.
pub fn forget_all_definitions() {
    if let Some(cache) = DEFINITIONS.get() {
        cache.clear();
    }
}

/// Reads one definition through the process cache when it uses the app's loader folders.
pub(super) fn read_cached(path: &Path, loaders: &Path) -> Result<Arc<GameDef>, AppError> {
    read_through(
        DEFINITIONS.get_or_init(crate::infra::file_cache::FileCache::default),
        path,
        loaders,
    )
}

/// Reads one definition through `cache` when it uses the app's loader folders; a caller that passes its own loader
/// folder reads directly, so a temporary loader cannot be served from another test's cache. The cache is passed in so a
/// test can use one of its own instead of the process-wide entries other tests share.
pub(super) fn read_through(
    cache: &crate::infra::file_cache::FileCache<GameDef>,
    path: &Path,
    loaders: &Path,
) -> Result<Arc<GameDef>, AppError> {
    if loaders != loaders_dir() {
        return read_definition(path, loaders).map(Arc::new);
    }

    cache.load(path, || read_definition(path, loaders))
}
