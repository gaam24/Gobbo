//! The names of the lifecycle events the task registry publishes to the window.

/// The event that carries a newly started [`TaskView`](super::model::TaskView).
pub(super) const TASK_STARTED_EVENT: &str = "task-started";
/// The event that carries a finished [`TaskView`](super::model::TaskView).
pub(super) const TASK_FINISHED_EVENT: &str = "task-finished";
/// The event that carries a cancelled [`TaskView`](super::model::TaskView).
pub(super) const TASK_CANCELLED_EVENT: &str = "task-cancelled";
/// The event that carries a [`TaskView`](super::model::TaskView) whose `percent` changed.
pub(super) const TASK_PROGRESS_EVENT: &str = "task-progress";
