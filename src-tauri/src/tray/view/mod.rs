//! The tray's view model. `snapshot` is the only part that reads the app; `render` turns plain data into one menu
//! description, so the state, labels and enabled rules can be tested without a window or an operating system.

mod model;
mod render;
mod snapshot;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use model::{Game, GameChoice, Stage};
pub use model::{IconState, Rendered, Snapshot, SpecItem};
#[cfg(test)]
pub use render::action_code;
// Tests reach action_code through `use super::*`; a glob is not counted as a read by `unused_imports`.
#[cfg(test)]
const _: fn(crate::status::stage::NextAction) -> &'static str = action_code;
pub use render::render;
pub use snapshot::snapshot;
