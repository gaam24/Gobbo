//! Mod packs: a recipe that rebuilds profiles in another library. The feature owns its commands, its reader/writer and
//! its tests; the shared pack records stay in `domain/pack.rs`.

/// The stop code for a pack install the user stopped. `download.cancelled`, `collection.cancelled` and
/// `import.cancelled` are deliberate siblings for the same underlying stop in other contexts; the frontend chooses
/// the message from the code, so none of them is a duplicate to merge.
pub const PACK_CANCELLED: &str = "pack.cancelled";

mod adopt;
pub mod check;
pub mod commands;
pub mod download;
pub mod export;
pub mod import;
pub mod inspect;
pub mod link;
pub mod model;
