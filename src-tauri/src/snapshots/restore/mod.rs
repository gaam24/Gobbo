//! Restoring a snapshot: the preview, the fresh backup, the checked copy back and the materialization.

mod apply;
mod files;
mod preview;
#[cfg(test)]
mod tests;

pub use apply::restore;
pub use preview::preview;

#[cfg(test)]
use apply::restore_redirect_state;
