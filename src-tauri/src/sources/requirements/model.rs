//! The requirement closure as the window sees it.

use serde::Serialize;

use crate::domain::source::SourceRef;
use crate::error::Message;
use crate::status::views::Status;

/// What the plan will do with one requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RequirementAction {
    /// The profile already has it on.
    AlreadyOn,
    /// It is in the library but off in the profile.
    Enable,
    /// It is not in the library and its provider can install it.
    Download,
    /// Nothing can install it automatically; `message` says why.
    Blocked,
}

/// One requirement in the closure, with how deep it sits and what the plan will do with it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RequirementStep {
    /// Stable within one plan, for the list key.
    pub id: String,
    /// The library family when the requirement is installed.
    pub family: Option<String>,
    /// The provider mod when the requirement names one.
    pub source: Option<SourceRef>,
    pub name: String,
    /// How many levels below the starting mod this requirement sits.
    pub depth: u32,
    /// Set when this step is one of several alternatives; one of them is enough.
    pub group: Option<String>,
    pub action: RequirementAction,
    pub message: Option<Message>,
}

/// What the plan will do to meet a mod's requirements, with the steps and the counts for each action.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RequirementPlan {
    /// The family the plan starts from.
    pub root: String,
    pub steps: Vec<RequirementStep>,
    /// Requirement chains that loop back on themselves. They are shown, not followed.
    pub cycles: Vec<Message>,
    pub enable: u32,
    pub download: u32,
    pub blocked: u32,
}

/// What installing a requirement plan did, with the fresh status and any steps that failed.
#[derive(Debug, Serialize, specta::Type)]
pub struct RequirementInstallView {
    pub status: Status,
    /// Requirement steps that were enabled or installed.
    pub installed: u32,
    /// Steps the plan could not install, with the reason (a missing key, no direct download, a local mod the library
    /// does not have).
    pub blocked: Vec<Message>,
    /// Downloads that failed while the rest of the plan still ran.
    pub failed: Vec<Message>,
}
