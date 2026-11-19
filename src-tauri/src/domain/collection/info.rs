use serde::{Deserialize, Serialize};
use specta::Type;

/// The stable fields of one collection revision, as the preview and the cache keep them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionInfo {
    pub provider: String,
    pub game_key: String,
    pub slug: String,
    pub revision: u32,
    pub latest_revision: u32,
    /// The revision as the provider names it: `r49` on Nexus, `v1.2.3` on Thunderstore. The number orders revisions;
    /// the label is what the user reads.
    #[serde(default)]
    pub revision_label: String,
    pub name: String,
    pub author: Option<String>,
    pub summary: String,
    pub description: String,
    pub instructions: String,
    pub picture_url: Option<String>,
    pub adult: bool,
    /// The total size of the mod downloads in KB. It fits in `u32` for every real collection; a larger number saturates.
    pub total_size_kb: u32,
    pub mod_count: u32,
    pub game_versions: Vec<String>,
    pub changelog: Option<String>,
    pub url: String,
}

impl CollectionOrigin {
    /// The collection key used by the IPC views and the profile badge.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.provider, self.game_key, self.slug)
    }

    /// Whether a newer revision than the installed one is known. The badge and the collection panel both read this
    /// rule, so they cannot disagree about the same origin.
    pub const fn has_update(&self) -> bool {
        self.latest_revision > self.revision
    }

    /// The revision label, with `r<number>` for an origin saved before labels existed.
    pub fn label(&self) -> String {
        if self.revision_label.is_empty() {
            format!("r{}", self.revision)
        } else {
            self.revision_label.clone()
        }
    }
}

/// The small collection marker a profile view carries. The full origin stays in the profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionBadge {
    pub provider: String,
    pub game_key: String,
    pub slug: String,
    pub revision: u32,
    pub revision_label: String,
    pub name: String,
    pub url: String,
    /// The profile families that belong to this collection.
    pub families: Vec<String>,
    /// Whether the provider reported a newer revision than the one installed in this profile.
    pub update: bool,
}

impl From<&CollectionOrigin> for CollectionBadge {
    fn from(origin: &CollectionOrigin) -> Self {
        Self {
            provider: origin.provider.clone(),
            game_key: origin.game_key.clone(),
            slug: origin.slug.clone(),
            revision: origin.revision,
            revision_label: origin.label(),
            name: origin.name.clone(),
            url: origin.url.clone(),
            families: origin.families.clone(),
            update: origin.has_update(),
        }
    }
}

/// The profile provenance of one installed collection. A profile can hold several.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionOrigin {
    pub provider: String,
    pub game_key: String,
    pub slug: String,
    pub revision: u32,
    /// The newest revision the provider reported at the last install or check. `0` for an origin saved before Gobbo
    /// kept it, so an old profile never claims an update it cannot name.
    #[serde(default)]
    pub latest_revision: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision_label: String,
    pub name: String,
    pub author: Option<String>,
    pub url: String,
    /// Item keys the user chose not to install.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<String>,
    /// Item keys completed without a library source (bundles, direct links and manual entries).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual: Vec<String>,
    /// Family keys this collection added to the profile. Removing the collection removes these, unless another
    /// collection of the profile uses them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// Every family of the profile that is a member of this collection, including mods the list already had. The
    /// mod list shows the collection on these rows; removing the collection never touches the ones not in `added`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub families: Vec<String>,
    /// Family keys an update switched off because a newer revision removed them. A mod the user switched off by hand
    /// is not recorded here, so installing again does not change it. A later revision that lists one again switches it
    /// back on and drops the key from this set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<String>,
    /// Unix time in seconds of the last install or update.
    #[serde(default)]
    pub installed_at: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin_with_revisions(revision: u32, latest: u32) -> CollectionOrigin {
        CollectionOrigin {
            provider: "nexus".into(),
            game_key: "examplegame".into(),
            slug: "example".into(),
            revision,
            latest_revision: latest,
            revision_label: String::new(),
            name: "Example".into(),
            author: None,
            url: String::new(),
            excluded: Vec::new(),
            manual: Vec::new(),
            added: Vec::new(),
            families: Vec::new(),
            disabled: Vec::new(),
            installed_at: 0,
        }
    }

    #[test]
    fn a_badge_reports_an_update_only_for_a_newer_revision() {
        assert!(!CollectionBadge::from(&origin_with_revisions(2, 2)).update);
        assert!(!CollectionBadge::from(&origin_with_revisions(2, 0)).update);
        assert!(CollectionBadge::from(&origin_with_revisions(2, 3)).update);
        assert!(!origin_with_revisions(2, 2).has_update());
        assert!(origin_with_revisions(2, 3).has_update());
    }
}
