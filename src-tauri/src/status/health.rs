//! The profile health report: one list for the things that block an apply, the warnings that do not, and the next
//! action for each. It reads the same `Status` every screen reads, so it cannot disagree with the list or the header.

use serde::Serialize;

use crate::context::Data;
use crate::error::{AppError, Message};
use crate::infra::clock::now_seconds;
use crate::status::stage::GameStage;

/// Whether a health item blocks an apply or is only a warning the user should see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    Problem,
    Warning,
}

/// The kind of next step a health item offers, which the window turns into a button or view change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HealthActionKind {
    OpenSettings,
    ShowMods,
    OpenOverwrite,
    Apply,
}

/// The next step attached to a health item, by its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct HealthAction {
    pub kind: HealthActionKind,
}

/// One thing the health report shows: a problem or warning, its message and the action it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct HealthItem {
    /// Stable within one report, for the list key.
    pub id: String,
    pub severity: HealthSeverity,
    pub message: Message,
    pub action: Option<HealthAction>,
}

/// The items the health check found, with the time it ran, for the window to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct HealthReport {
    pub checked_at: u32,
    pub items: Vec<HealthItem>,
}

/// Builds the health report from the game status. A problem is something that blocks an apply; a warning is something
/// the user should see but that does not stop the next apply.
pub fn health(root: &Data, game_id: &str) -> Result<HealthReport, AppError> {
    let status = crate::status::status(root, game_id)?;
    let mut items = Vec::new();

    for (at, problem) in status.problems.iter().enumerate() {
        items.push(HealthItem {
            id: format!("problem:{at}:{}", problem.code),
            severity: HealthSeverity::Problem,
            message: problem.clone(),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        });
    }

    for (at, warning) in status.warnings.iter().enumerate() {
        items.push(HealthItem {
            id: format!("warning:{at}:{}", warning.code),
            severity: HealthSeverity::Warning,
            message: warning.clone(),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        });
    }

    match &status.stage {
        GameStage::Setup { .. } => items.push(HealthItem {
            id: "stage:setup".to_string(),
            severity: HealthSeverity::Problem,
            message: Message::new("health.setup"),
            action: Some(HealthAction {
                kind: HealthActionKind::OpenSettings,
            }),
        }),
        GameStage::Empty => items.push(HealthItem {
            id: "stage:empty".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.emptyLibrary"),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        }),
        GameStage::NoEnabled { total } => items.push(HealthItem {
            id: "stage:no-enabled".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.noEnabled").with("total", total),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        }),
        GameStage::Draft { changes, .. } => items.push(HealthItem {
            id: "stage:draft".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.draft").with("count", changes),
            action: Some(HealthAction {
                kind: HealthActionKind::Apply,
            }),
        }),
        GameStage::Blocked { .. } | GameStage::Ready { .. } | GameStage::Applied { .. } => {}
    }

    if status.conflict_count > 0 {
        items.push(HealthItem {
            id: "conflicts".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.conflicts").with("count", status.conflict_count),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        });
    }

    if status.has_overwrite {
        items.push(HealthItem {
            id: "overwrite".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.overwrite"),
            action: Some(HealthAction {
                kind: HealthActionKind::OpenOverwrite,
            }),
        });
    }

    if status.bisect.is_some() {
        items.push(HealthItem {
            id: "bisect".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.bisect"),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        });
    }

    let requirements: u32 = status
        .mods
        .iter()
        .map(|mod_view| mod_view.unmet_requirements)
        .sum();
    if requirements > 0 {
        items.push(HealthItem {
            id: "requirements".to_string(),
            severity: HealthSeverity::Warning,
            message: Message::new("health.requirements").with("count", requirements),
            action: Some(HealthAction {
                kind: HealthActionKind::ShowMods,
            }),
        });
    }

    Ok(HealthReport {
        checked_at: now_seconds(),
        items,
    })
}
