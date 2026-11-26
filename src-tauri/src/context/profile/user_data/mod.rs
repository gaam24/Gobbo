//! Per-profile user data outside the game folder: saves, settings and similar folders declared by `[[user_data]]` in
//! `game.toml`. A set is either shared, redirected through the overlay, or copied around a launch when the platform
//! cannot redirect it. The copy path is the fallback for a platform without the overlay redirect.
//!
//! The module owns the sets and the paths; the owner file and the switch order are shared with the legacy copy list
//! in `store.rs`. Redirect sets are never materialized: their writes already live in the profile's upper layer.

mod resolve;
mod store;

#[cfg(test)]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
mod tests;

#[cfg(test)]
pub use resolve::prepare;
pub(super) use resolve::real_roots;
pub use resolve::{
    is_recovery, is_redirected, seeded, sets, state_dir, whiteouts, whiteouts_journal, write_path,
};
pub use store::{capture_current, capture_settings, owner, restore_profile, switch};
