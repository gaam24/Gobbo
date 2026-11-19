use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::plan::CollectionPlan;

/// Where a member stands against the library and one profile. The interface draws this; the feature computes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionItemStatus {
    /// Not in the library and not waiting.
    Missing,
    /// Waiting in the profile for its download or import.
    Queued,
    /// In the library, but not in the profile.
    InLibrary,
    /// In the profile with the pinned version.
    Installed,
    /// In the profile with a version the collection's `latest` or `prefer` policy accepts.
    Satisfied { current: String },
    /// In the profile with another version or file of the same mod.
    DifferentVersion { current: String },
    /// In the library with another version or file of the same mod.
    InLibraryDifferentVersion { current: String },
    /// The user chose not to install it.
    Skipped,
    /// Completed without a library source, or marked done by the user.
    Manual,
    /// The source cannot be installed automatically.
    Unsupported,
}

impl CollectionItemStatus {
    /// Whether the item is already done and the install loop can skip it.
    pub const fn is_done(&self) -> bool {
        matches!(
            self,
            Self::Installed | Self::Satisfied { .. } | Self::Manual | Self::Skipped
        )
    }
}

/// The counts the preview and the session header show. One item counts once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionCounts {
    pub total: u32,
    pub required: u32,
    pub optional: u32,
    pub installed: u32,
    pub pending: u32,
    pub skipped: u32,
    pub manual: u32,
    pub unsupported: u32,
}

impl CollectionCounts {
    /// Counts the statuses of `plan`.
    pub fn of(plan: &CollectionPlan, statuses: &BTreeMap<String, CollectionItemStatus>) -> Self {
        let mut counts = Self {
            total: u32::try_from(plan.items.len()).unwrap_or(u32::MAX),
            ..Self::default()
        };
        for item in &plan.items {
            if item.optional {
                counts.optional = counts.optional.saturating_add(1);
            } else {
                counts.required = counts.required.saturating_add(1);
            }
            match statuses.get(&item.key) {
                Some(CollectionItemStatus::Installed | CollectionItemStatus::Satisfied { .. }) => {
                    counts.installed = counts.installed.saturating_add(1);
                }
                Some(CollectionItemStatus::Skipped) => {
                    counts.skipped = counts.skipped.saturating_add(1);
                }
                Some(CollectionItemStatus::Manual) => {
                    counts.manual = counts.manual.saturating_add(1);
                }
                Some(CollectionItemStatus::Unsupported) => {
                    counts.unsupported = counts.unsupported.saturating_add(1);
                }
                _ => counts.pending = counts.pending.saturating_add(1),
            }
        }
        counts
    }
}
