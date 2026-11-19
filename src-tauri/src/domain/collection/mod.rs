//! Provider collections (Nexus collections, Thunderstore modpacks) as a recipe: the collection metadata, its mods and the order they enter a profile. The provider
//! adapter maps its API to these types; the feature module reads the collection archive and stores the plan. No I/O
//! lives here.

use std::collections::BTreeSet;

mod info;
mod item;
mod plan;
mod status;
mod update;

/// The profile changes an update makes to mods the collection switched off earlier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisabledChange {
    /// Families to switch off: the collection added them, the new revision no longer lists them and no other
    /// collection of the profile uses them.
    pub disable: BTreeSet<String>,
    /// Families to switch back on because the new revision lists them again.
    pub enable: BTreeSet<String>,
    /// The set to record on the origin after the update.
    pub next: BTreeSet<String>,
}

pub use info::{CollectionBadge, CollectionInfo, CollectionOrigin};
pub use item::{CollectionItem, CollectionSource, CollectionUpdatePolicy, OPTIONAL_PHASE};
pub use plan::{
    CollectionIssue, CollectionIssueSeverity, CollectionPlan, CollectionRule, CollectionRuleKind,
    PLAN_FORMAT, order_items,
};
pub use status::{CollectionCounts, CollectionItemStatus};
pub use update::reconcile_disabled;
