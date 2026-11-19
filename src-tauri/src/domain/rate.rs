//! Pure pacing data for downloads. No clock, no I/O and no provider names here: the provider layer decides which
//! rule applies, and the infrastructure applies it. Keeping the decision out of the download code makes it testable
//! with plain values and keeps arbitrary delays out of the hot path.

use std::time::Duration;

/// The account a download is made with. Only one provider has paid tiers; the others need no tier to be paced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountTier {
    Anonymous,
    Free,
    Premium,
}

/// How a provider's downloads are paced on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadPolicy {
    /// No client-side wait. The server enforces its own limits; a 429 is handled when it arrives.
    Unlimited,
    /// At most `max` starts inside `window`. A safety net for a burst that is known to trigger a temporary ban,
    /// not a normal pace: a user well below the ceiling never waits.
    WindowBudget { max: u32, window: Duration },
}
