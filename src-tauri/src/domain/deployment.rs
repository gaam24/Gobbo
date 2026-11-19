//! The completion marker of a deployment, `deployment.json`: the value that says an apply reached its end. The file
//! lives next to the journal; `deploy::journal` writes it and `infra::deployment` reads it.

use serde::{Deserialize, Serialize};

/// Written only after every file and the settings are in place, so a journal alone is an interrupted apply and not a
/// deployment the game may use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentRecord {
    /// The on-disk format. A marker written before the field existed is the current version, because every marker of
    /// that time had this shape. A newer value means a later build wrote it and this one must not trust it.
    #[serde(default = "crate::domain::state_schema")]
    pub schema_version: u32,
    /// How many files the apply placed.
    pub files: u32,
    /// A stable fingerprint of the plan, so a repair can tell whether the journal matches the list.
    pub plan_hash: String,
}

impl DeploymentRecord {
    /// Whether this build understands the marker. A marker without `schema_version` defaults to the current version,
    /// so a deployment of an older Gobbo still counts as applied.
    pub const fn is_supported(&self) -> bool {
        self.schema_version <= crate::domain::STATE_SCHEMA
    }
}
