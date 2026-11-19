//! Attaching a collection to a profile, skipping members and closing the install. The waiting entries reuse the pack
//! mechanism: a member joins the list as soon as the library has its archive, and one final profile edit reorders the
//! block and records one history point.

mod edits;
mod planning;
mod removal;
#[cfg(test)]
mod tests;
mod view;

pub use edits::{finish, finish_with_manual, mark_done, skip, status};
pub use planning::start;
#[cfg(test)]
pub(super) use planning::start_with_plan;
pub use removal::remove;
