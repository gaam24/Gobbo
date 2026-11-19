//! A small cache of values parsed from files or folders. An entry is keyed by the path's length and modification time,
//! so any write, including one from another process, makes the next read parse it again. A directory's modification
//! time changes exactly when an entry is added or removed, so the same key serves a listing. The cache keeps the entries
//! of several games, so switching between two games does not re-parse both state files every time.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use crate::error::AppError;

/// The most recent values read from different paths. Four games fit comfortably; a fifth evicts the oldest.
const CAPACITY: usize = 8;

/// The last values read from a handful of paths. Cloning the `Data` handle clones the `Arc` around the entries, so every
/// command shares the same cache.
pub struct FileCache<T> {
    entries: Mutex<Vec<(Key, Arc<T>)>>,
}

impl<T> Default for FileCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Key {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}

impl Key {
    fn of(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            len: meta.len(),
            modified: meta.modified().ok()?,
        })
    }
}

impl<T> FileCache<T> {
    /// The cached value when `path` still has the same key; otherwise `parse` reads it and the cache keeps it. The key
    /// is taken before `parse`, so a write during the parse makes the next read miss. A missing path is not cached.
    pub fn load(
        &self,
        path: &Path,
        parse: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<Arc<T>, AppError> {
        let key = Key::of(path);
        if let Some(hit) = key.as_ref().and_then(|key| self.hit(key)) {
            return Ok(hit);
        }
        let value = Arc::new(parse()?);
        if let Some(key) = key {
            self.put(key, Arc::clone(&value));
        }
        Ok(value)
    }

    /// Remembers a value that was just written to `path`, so the next read does not parse the file again.
    pub fn store(&self, path: &Path, value: Arc<T>) {
        if let Some(key) = Key::of(path) {
            self.put(key, value);
        }
    }

    /// Drops the entry for `path`, for a caller that just wrote the file and knows the old value is stale.
    pub fn invalidate(&self, path: &Path) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|(cached, _)| cached.path != path);
    }

    fn hit(&self, key: &Key) -> Option<Arc<T>> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let at = entries.iter().position(|(cached, _)| cached == key)?;
        let entry = entries.remove(at);
        let value = Arc::clone(&entry.1);
        entries.insert(0, entry);
        drop(entries);
        Some(value)
    }

    fn put(&self, key: Key, value: Arc<T>) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|(cached, _)| cached.path != key.path);
        entries.insert(0, (key, value));
        entries.truncate(CAPACITY);
        drop(entries);
    }
}

impl<T: Clone> FileCache<T> {
    /// A copy of the value for a caller that will change it. A hit avoids parsing the file and keeps the entry for the
    /// next reader; the caller's write replaces it through `store`.
    pub fn load_cloned(
        &self,
        path: &Path,
        parse: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        self.load(path, parse).map(|value| (*value).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_hit_is_reused_and_a_changed_file_is_parsed_again() {
        let dir = scratch!("file-cache");
        let path = dir.join("library.json");
        fs::write(&path, "one").unwrap();
        let cache = FileCache::<u32>::default();
        let mut parses = 0;

        let first = cache
            .load(&path, || {
                parses += 1;
                Ok(1)
            })
            .unwrap();
        let second = cache
            .load(&path, || panic!("a cache hit must not parse"))
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        // A longer file has a different key even when the clock has not moved.
        fs::write(&path, "one and more").unwrap();
        let third = cache
            .load(&path, || {
                parses += 1;
                Ok(2)
            })
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(parses, 2);
    }

    #[test]
    fn a_stored_value_is_reused() {
        let dir = scratch!("file-cache-store");
        let path = dir.join("journal.jsonl");
        fs::write(&path, "one").unwrap();
        let cache = FileCache::default();
        let stored = Arc::new(vec![dir.join("Mods/modA")]);
        cache.store(&path, Arc::clone(&stored));
        let loaded = cache
            .load(&path, || panic!("a stored value must not parse the file"))
            .unwrap();
        assert!(Arc::ptr_eq(&stored, &loaded));
    }

    #[test]
    fn a_folder_listing_is_reused_until_the_folder_changes() {
        let dir = scratch!("file-cache-folder");
        fs::create_dir_all(&dir).unwrap();
        let cache = FileCache::default();
        let first = cache.load(&dir, || Ok(vec!["one"])).unwrap();
        let second = cache
            .load(&dir, || panic!("a cache hit must not list the folder"))
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        // A new entry changes the folder's modification time.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join("two"), "x").unwrap();
        let third = cache.load(&dir, || Ok(vec!["one", "two"])).unwrap();
        assert_eq!(third.len(), 2);
    }

    #[test]
    fn a_missing_path_is_not_cached() {
        let dir = scratch!("file-cache-missing");
        let cache = FileCache::default();
        let mut parses = 0;
        for _ in 0..2 {
            cache
                .load(&dir.join("library.json"), || {
                    parses += 1;
                    Ok(0)
                })
                .unwrap();
        }
        assert_eq!(parses, 2);
    }
}

#[cfg(test)]
mod lru_tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn two_games_do_not_evict_each_other() {
        let dir = scratch!("file-cache-lru");
        let first = dir.join("first.json");
        let second = dir.join("second.json");
        fs::write(&first, "first").unwrap();
        fs::write(&second, "second").unwrap();
        let cache = FileCache::<u32>::default();

        cache.load(&first, || Ok(1)).unwrap();
        cache.load(&second, || Ok(2)).unwrap();
        assert_eq!(
            *cache
                .load(&first, || panic!("a hit must not parse"))
                .unwrap(),
            1
        );
        assert_eq!(
            *cache
                .load(&second, || panic!("a hit must not parse"))
                .unwrap(),
            2
        );
    }
}
