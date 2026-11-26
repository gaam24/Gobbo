//! Searching for games: every store's installs of a game (Steam first, then the stores in `stores`), checked with the
//! marker rule, plus the store that claims a folder the user picks. Read-only: it writes nothing.

mod markers;
mod model;
mod scan;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use markers::has_markers_for;
pub use markers::{expected_markers, game_folder, has_markers, marker_file_for};
pub use model::StoreInstall;
pub use model::{Found, GameScan};
#[cfg(test)]
pub use scan::{classify_in, scan_in};
pub use scan::{classify_install, scan};
