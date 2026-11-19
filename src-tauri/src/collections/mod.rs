//! Collections and modpacks: reading the recipe, planning the members, creating a profile, installing the selected
//! members and checking the provider for newer revisions. The provider adapters live in `providers`; this module owns
//! the recipe and the profile.

/// The stop code for a collection install the user stopped. `download.cancelled`, `pack.cancelled` and
/// `import.cancelled` are deliberate siblings for the same underlying stop in other contexts; the frontend chooses
/// the message from the code, so none of them is a duplicate to merge.
pub const COLLECTION_CANCELLED: &str = "collection.cancelled";

pub mod bundle;
pub mod cache;
pub mod check;
pub mod commands;
pub mod controller;
pub mod download;
pub mod external;
pub mod inspect;
pub mod manifest;
pub mod model;
pub mod patch;
pub mod session;
pub mod status;
