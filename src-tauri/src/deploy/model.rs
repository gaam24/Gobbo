//! The result of a deployment, as the window shows it.

use serde::Serialize;

/// The result of an apply as the window sees it.
#[derive(Debug, Serialize, specta::Type)]
pub struct ApplyView {
    /// How many files the deployment placed.
    pub files: u32,
}

/// The phase of an apply, for the progress channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DeployStage {
    Preparing,
    Writing,
    Settings,
    Done,
}

/// One progress step of an apply. The window shows the percent and the file count; the stage names the phase when the
/// count is not moving (preparing the plan, writing settings, finishing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct DeployProgress {
    pub stage: DeployStage,
    pub done: u32,
    pub total: u32,
    pub percent: u32,
}

impl DeployProgress {
    /// A step with the counts clamped to `u32`. A total of zero is complete only for `Done`; a preparing plan shows 0.
    pub fn at(stage: DeployStage, done: usize, total: usize) -> Self {
        let done = u32::try_from(done).unwrap_or(u32::MAX);
        let total = u32::try_from(total).unwrap_or(u32::MAX);
        let unknown = if stage == DeployStage::Done { 100 } else { 0 };
        let percent =
            crate::domain::progress::percent(u64::from(done), u64::from(total)).unwrap_or(unknown);
        Self {
            stage,
            done,
            total,
            percent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_never_leaves_zero_to_hundred() {
        assert_eq!(DeployProgress::at(DeployStage::Preparing, 0, 0).percent, 0);
        assert_eq!(DeployProgress::at(DeployStage::Writing, 0, 10).percent, 0);
        assert_eq!(DeployProgress::at(DeployStage::Writing, 5, 10).percent, 50);
        assert_eq!(
            DeployProgress::at(DeployStage::Writing, 10, 10).percent,
            100
        );
        assert_eq!(
            DeployProgress::at(DeployStage::Writing, 11, 10).percent,
            100
        );
        assert_eq!(DeployProgress::at(DeployStage::Done, 0, 0).percent, 100);
    }
}
