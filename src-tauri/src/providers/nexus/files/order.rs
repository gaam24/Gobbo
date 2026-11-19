//! The order of the versions inside one file group: position, then upload time, then file id.

use std::cmp::Ordering;

use super::ModFile;

/// Whether Nexus still lists this version as a download. Old, archived and removed versions stay in the version list
/// but must not be reported as an update.
pub(super) fn is_active_version(file: &ModFile) -> bool {
    file.is_active
        && !file.version_category.as_deref().is_some_and(|category| {
            let category = category.trim();
            category.eq_ignore_ascii_case("old_version")
                || category.eq_ignore_ascii_case("archived")
                || category.eq_ignore_ascii_case("removed")
        })
}

/// Orders two versions of a file group: position first, then upload time, then file id.
pub(super) fn newest_order(left: &ModFile, right: &ModFile) -> Ordering {
    let by_position = match (left.position.as_deref(), right.position.as_deref()) {
        (Some(left), Some(right)) => decimal_cmp(left, right),
        _ => None,
    };
    if let Some(order) = by_position
        && order != Ordering::Equal
    {
        return order;
    }
    match (left.uploaded_at, right.uploaded_at) {
        (Some(left), Some(right)) if left != right => return left.cmp(&right),
        _ => {}
    }
    left.file_id.cmp(&right.file_id)
}

/// Compares two non-negative decimal strings exactly, without parsing them into a float. `None` when either is not a
/// decimal number, so the caller can use the next signal.
pub(super) fn decimal_cmp(left: &str, right: &str) -> Option<Ordering> {
    let left = left.trim();
    let right = right.trim();
    let (left_integer, left_fraction) = split_decimal(left)?;
    let (right_integer, right_fraction) = split_decimal(right)?;
    let integer = left_integer
        .len()
        .cmp(&right_integer.len())
        .then_with(|| left_integer.as_bytes().cmp(right_integer.as_bytes()));
    if integer != Ordering::Equal {
        return Some(integer);
    }
    let width = left_fraction.len().max(right_fraction.len());
    for at in 0..width {
        let left = left_fraction.as_bytes().get(at).copied().unwrap_or(b'0');
        let right = right_fraction.as_bytes().get(at).copied().unwrap_or(b'0');
        let order = left.cmp(&right);
        if order != Ordering::Equal {
            return Some(order);
        }
    }
    Some(Ordering::Equal)
}

/// Splits a decimal into its integer and fraction parts, both without leading or trailing zeros. Every character must
/// be an ASCII digit and at most one dot may appear.
pub(super) fn split_decimal(text: &str) -> Option<(&str, &str)> {
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    if integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((
        integer.trim_start_matches('0'),
        fraction.trim_end_matches('0'),
    ))
}
