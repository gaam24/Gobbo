//! The wall clock of the state files. One function, so every timestamp in a save file counts the same seconds.

/// Unix time in seconds. It fits in `u32` until 2106.
pub fn now_seconds() -> u32 {
    u32::try_from(seconds_since(std::time::SystemTime::now())).unwrap_or(u32::MAX)
}

/// Seconds since the Unix epoch at `time`. A time before the epoch counts as 0.
pub fn seconds_since(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// A timestamp written as RFC 3339, as seconds since the Unix epoch. Text in any other form is `None`: a date the app
/// does not read is left out rather than guessed.
pub fn unix_time(text: &str) -> Option<u32> {
    let parsed =
        time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()?;
    u32::try_from(parsed.unix_timestamp()).ok()
}
