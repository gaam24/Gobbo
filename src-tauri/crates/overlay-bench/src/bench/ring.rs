//! A small benchmark of the file-backed decision ring: one record and one read of the newest decisions.

use std::error::Error;
use std::fs;
use std::time::Instant;

use gobbo_overlay_decisions::{Kind, Layer, Ring};
use hdrhistogram::Histogram;

const RECORDS: usize = 4096;
const READS: usize = 100;

/// Runs the ring benchmark and prints p50/p95 for `record` and `read_last`.
pub(super) fn run() -> Result<(), Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("gobbo-ring-bench-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join("ring");
    let mut ring = Ring::open(&path)?;

    let mut writes = Histogram::<u64>::new(3).map_err(|error| error.to_string())?;
    for index in 0..RECORDS {
        let start = Instant::now();
        ring.record(
            Kind::Write,
            Layer::Upper,
            0,
            if index.is_multiple_of(2) {
                r"data\a.txt"
            } else {
                r"content\scripts\gameplay\items\definitions\appearances.xml"
            },
            1,
        )?;
        writes
            .record(
                u64::try_from(start.elapsed().as_nanos())
                    .unwrap_or(u64::MAX)
                    .max(1),
            )
            .map_err(|error| error.to_string())?;
    }

    let mut reads = Histogram::<u64>::new(3).map_err(|error| error.to_string())?;
    for _ in 0..READS {
        let start = Instant::now();
        let (decisions, total) = gobbo_overlay_decisions::read_last(&path, 100)?;
        if total as usize != RECORDS || decisions.len() != 100 {
            return Err("the ring benchmark read the wrong number of decisions".into());
        }
        reads
            .record(
                u64::try_from(start.elapsed().as_nanos())
                    .unwrap_or(u64::MAX)
                    .max(1),
            )
            .map_err(|error| error.to_string())?;
    }

    println!("ring records: {RECORDS}, reads: {READS} x 100");
    println!(
        "record p50 {:.2} µs, p95 {:.2} µs",
        writes.value_at_quantile(0.50) as f64 / 1000.0,
        writes.value_at_quantile(0.95) as f64 / 1000.0
    );
    println!(
        "read_last p50 {:.2} µs, p95 {:.2} µs",
        reads.value_at_quantile(0.50) as f64 / 1000.0,
        reads.value_at_quantile(0.95) as f64 / 1000.0
    );
    let _ = fs::remove_dir_all(dir);
    Ok(())
}
