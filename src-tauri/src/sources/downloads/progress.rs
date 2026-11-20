//! How a download reports its progress: the cancel message and the meter of bytes per second.

use super::PROGRESS_EVERY;
use crate::context::size_kb;
use std::time::Instant;

/// The error of a download the user stopped.
pub(super) const CANCELLED: &str = crate::error::DOWNLOAD_CANCELLED;

/// Bytes per second since the last report, reported every `PROGRESS_EVERY`.
pub(super) struct Meter {
    pub(super) at: Instant,
    pub(super) bytes: u64,
}

impl Meter {
    pub(super) fn new() -> Self {
        Self {
            at: Instant::now(),
            bytes: 0,
        }
    }

    /// The speed in KB/s, when it is time for a report.
    pub(super) fn tick(&mut self, received: u64) -> Option<u32> {
        let elapsed = self.at.elapsed();

        if elapsed < PROGRESS_EVERY {
            return None;
        }

        let per_second =
            u128::from(received.saturating_sub(self.bytes)) * 1000 / elapsed.as_millis().max(1);
        self.at = Instant::now();
        self.bytes = received;
        Some(size_kb(u64::try_from(per_second).unwrap_or(u64::MAX)))
    }
}
