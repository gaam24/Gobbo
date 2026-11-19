//! The bisect search file: the suspects, the half that is on in the current test, and the list as it was before the
//! search. The steps are in `profiles::bisect`; the view for the window is in `status::bisect`.

use crate::context::Data;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

const BISECT_FILE: &str = "bisect.json";

/// A running search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Search {
    pub profile: String,
    /// Each family id of the list with its enabled flag before the search, to put back at the end.
    pub original: Vec<(String, bool)>,
    /// The mods that may still hold the problem, in list order.
    pub suspects: Vec<String>,
    /// The suspects switched on in the current test. The other suspects are off.
    pub testing: Vec<String>,
    pub step: u32,
}

/// The file of the search, in the game's data folder.
pub fn file(data: &Data) -> PathBuf {
    data.dir.join(BISECT_FILE)
}

/// The running search, or `None` when there is none or the file cannot be read.
pub fn read(data: &Data) -> Option<Search> {
    crate::infra::json::read_json_or_none(&file(data))
}
