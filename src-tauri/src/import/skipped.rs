//! The one rule for recording files no layout rule took.

use crate::domain::library::{SkipReason, SkippedFile};

/// The entries no rule took, capped for the index, and their full count. The list is capped so a bad archive cannot
/// grow `library.json` without limit; the total keeps the real number. Every path that records an archive uses this.
pub fn archive_skipped(skipped: &[SkippedFile]) -> (Vec<SkippedFile>, u32) {
    let total = u32::try_from(
        skipped
            .iter()
            .filter(|file| file.reason == SkipReason::NoRule)
            .count(),
    )
    .unwrap_or(u32::MAX);
    let list = skipped
        .iter()
        .filter(|file| file.reason == SkipReason::NoRule)
        .take(1000)
        .cloned()
        .collect();
    (list, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_archive_keeps_a_capped_list_and_the_full_count() {
        let file = SkippedFile {
            path: "readme.txt".to_string(),
            reason: SkipReason::NoRule,
        };
        let documented = SkippedFile {
            path: "docs/index.html".to_string(),
            reason: SkipReason::Noise,
        };
        let skipped: Vec<SkippedFile> = (0..1001)
            .map(|index| SkippedFile {
                path: format!("readme-{index}.txt"),
                ..file.clone()
            })
            .chain([documented])
            .collect();
        let (list, total) = archive_skipped(&skipped);
        assert_eq!(list.len(), 1000);
        assert_eq!(total, 1001, "documented entries are not counted");
    }
}
