//! The one percentage rule every progress bar uses: downloads, imports, deployments and updates.

/// `done` of `total` as a whole percentage from 0 to 100. `None` when the total is unknown (zero): each caller decides
/// whether an unknown total reads as not started or as done.
pub fn percent(done: u64, total: u64) -> Option<u32> {
    // Wide math: `done * 100` of a multi-exabyte total must not saturate into a wrong, tiny percentage.
    let part = (u128::from(done.min(total)) * 100).checked_div(u128::from(total))?;
    Some(u32::try_from(part).unwrap_or(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentage_is_clamped_and_an_unknown_total_has_none() {
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent(1, 4), Some(25));
        assert_eq!(percent(9, 4), Some(100));
        assert_eq!(percent(u64::MAX, u64::MAX), Some(100));
    }
}
