//! The counts of the hooked calls, and the report they are written to every `REPORT_EVERY` events. A report that shows one
//! call as more than a quarter of all calls says that a loop is likely.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Mutex, PoisonError};

use super::settings::settings;
use super::writer::write_file;
use crate::guard::Guard;

/// Events counted under one key each, and the total of all of them.
#[derive(Default)]
struct Counts {
    by_key: BTreeMap<String, u64>,
    total: u64,
}

/// The report is written after this many counted events.
const REPORT_EVERY: u64 = 1000;
/// How many of the most frequent keys the report lists.
const REPORT_TOP: usize = 60;

static COUNTS: Mutex<Counts> = Mutex::new(Counts {
    by_key: BTreeMap::new(),
    total: 0,
});

/// Counts one event under `operation` and the detail that `detail` builds. Nothing is built or counted unless the report
/// is on. The report file is written under the guard, so its own open is not counted again.
pub(crate) fn count(operation: &str, detail: impl FnOnce() -> String) {
    let settings = settings();
    let Some(path) = &settings.diag else {
        return;
    };

    if !settings.calls || super::crashing() {
        return;
    }

    let key = format!("{operation} {}", detail());
    let report = {
        let mut counts = COUNTS.lock().unwrap_or_else(PoisonError::into_inner);
        increment(&mut counts, key);

        if !counts.total.is_multiple_of(REPORT_EVERY) {
            return;
        }

        report(&counts)
    };

    if let Some(_guard) = Guard::enter()
        && let Err(error) = write_file(path, &report)
    {
        crate::trace::log(&format!("could not write the count report: {error}"));
    }
}

/// Counts one event, saturating instead of overflowing after a lifetime of calls.
fn increment(counts: &mut Counts, key: String) {
    let entry = counts.by_key.entry(key).or_insert(0);
    *entry = entry.saturating_add(1);
    counts.total = counts.total.saturating_add(1);
}

/// The most frequent keys, and whether one of them looks like a loop.
fn report(counts: &Counts) -> String {
    let mut ranked: Vec<(&String, &u64)> = counts.by_key.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut out = format!("total events: {}\n", counts.total);

    if let Some((_, top)) = ranked.first()
        && (**top).saturating_mul(4) > counts.total
    {
        out.push_str("one event is more than a quarter of all events: a loop is likely\n");
    }

    for (key, count) in ranked.into_iter().take(REPORT_TOP) {
        let _ = writeln!(out, "{count:>9}  {key}");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_ranks_by_count_and_flags_a_loop() {
        let mut counts = Counts::default();

        for _ in 0..30 {
            *counts
                .by_key
                .entry("NtQueryAttributesFile a.dll".into())
                .or_insert(0) += 1;
        }

        *counts.by_key.entry("CreateFileW b.dll".into()).or_insert(0) += 1;
        counts.total = 31;
        let text = report(&counts);
        assert!(text.starts_with("total events: 31\none event"));
        assert!(text.contains("a loop is likely"));
        assert!(text.find("NtQueryAttributesFile").unwrap() < text.find("CreateFileW").unwrap());
    }

    #[test]
    fn a_report_without_a_dominant_call_flags_nothing() {
        let mut counts = Counts::default();

        for (key, count) in [("a", 5u64), ("b", 5), ("c", 5), ("d", 5)] {
            counts.by_key.insert(key.into(), count);
        }

        counts.total = 20;
        assert!(!report(&counts).contains("loop"));
    }

    #[test]
    fn counting_saturates_at_the_limit_instead_of_overflowing() {
        let mut counts = Counts::default();
        counts.by_key.insert("a".into(), u64::MAX);
        counts.total = u64::MAX;

        increment(&mut counts, "a".into());

        assert_eq!(counts.by_key["a"], u64::MAX);
        assert_eq!(counts.total, u64::MAX);
    }

    #[test]
    fn a_report_with_a_huge_top_count_does_not_overflow_the_loop_check() {
        let mut counts = Counts::default();
        counts.by_key.insert("a".into(), u64::MAX / 2);
        counts.total = u64::MAX;

        assert!(report(&counts).starts_with(&format!("total events: {}\n", u64::MAX)));
    }
}
