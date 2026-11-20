//! Removes the developer files of earlier runs. Each run writes files of its own, so the folder grows with every run. A new
//! process clears out the files older than `KEEP_FOR`, and all but the `KEEP_FILES` newest. A file that another process
//! still has open cannot be removed, and then it is left alone.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use super::settings::developer_dir;

/// How many developer files are kept, the newest first.
const KEEP_FILES: usize = 200;
/// How long a developer file is kept.
const KEEP_FOR: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Removes the old developer files. A release build writes none, so it has none to remove.
pub(crate) fn prune_old_files() {
    if !cfg!(debug_assertions) {
        return;
    }

    let Some(dir) = developer_dir() else {
        return;
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let files: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;

            if !meta.is_file() {
                return None;
            }

            Some((entry.path(), meta.modified().ok()?))
        })
        .collect();
    for path in prune_plan(files, SystemTime::now(), KEEP_FILES, KEEP_FOR) {
        let _ = fs::remove_file(path);
    }
}

/// The files to remove: those older than `keep_for`, and every file after the `keep` newest. A file dated after `now` is
/// not old.
fn prune_plan(
    mut files: Vec<(PathBuf, SystemTime)>,
    now: SystemTime,
    keep: usize,
    keep_for: Duration,
) -> Vec<PathBuf> {
    files.sort_by_key(|file| std::cmp::Reverse(file.1));
    files
        .into_iter()
        .enumerate()
        .filter(|(index, (_, modified))| {
            *index >= keep
                || now
                    .duration_since(*modified)
                    .is_ok_and(|age| age > keep_for)
        })
        .map(|(_, (path, _))| path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A time that is `seconds` before `now`.
    fn ago(seconds: u64, now: SystemTime) -> SystemTime {
        now - Duration::from_secs(seconds)
    }

    #[test]
    fn the_newest_files_are_kept_up_to_the_limit() {
        let now = SystemTime::now();
        let files = (0..5)
            .map(|number| (PathBuf::from(format!("f{number}")), ago(number, now)))
            .collect();
        let removed = prune_plan(files, now, 3, Duration::from_secs(3600));
        assert_eq!(removed, [PathBuf::from("f3"), PathBuf::from("f4")]);
    }

    #[test]
    fn a_file_older_than_the_limit_goes_even_among_the_newest() {
        let now = SystemTime::now();
        let files = vec![
            (PathBuf::from("new"), ago(10, now)),
            (PathBuf::from("old"), ago(7200, now)),
        ];
        let removed = prune_plan(files, now, 10, Duration::from_secs(3600));
        assert_eq!(removed, [PathBuf::from("old")]);
    }

    #[test]
    fn a_file_dated_after_now_is_kept() {
        let now = SystemTime::now();
        let files = vec![(PathBuf::from("later"), now + Duration::from_secs(60))];
        assert!(prune_plan(files, now, 10, Duration::from_secs(3600)).is_empty());
    }
}
