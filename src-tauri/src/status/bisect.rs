//! The bisect search as the window shows it: the test now, the suspects left, the mods on in this test and the mod found
//! at the end. The steps are in `profiles::bisect`, the file in `context::profile::bisect`.

use crate::context::Data;
use crate::context::profile::bisect::read;
use crate::library::Library;
use serde::Serialize;

/// The search in the view.
#[derive(Debug, Serialize, specta::Type)]
pub struct BisectView {
    /// The test now, from 1.
    pub step: u32,
    /// About how many tests are left, at most.
    pub steps_left: u32,
    pub suspects: u32,
    /// The names of the suspects switched on in this test.
    pub testing: Vec<String>,
    /// The mod that holds the problem, once one is left.
    pub found: Option<String>,
}

/// The search of the active profile, for the status.
pub fn bisect_view(data: &Data, library: &Library, profile: &str) -> Option<BisectView> {
    let search = read(data).filter(|search| search.profile == profile)?;
    let name = |id: &String| {
        library
            .families
            .get(id)
            .map_or_else(|| id.clone(), |family| family.name.clone())
    };
    let left = search.suspects.len();
    Some(BisectView {
        step: search.step,
        steps_left: if left <= 1 { 0 } else { (left - 1).bit_width() },
        suspects: u32::try_from(left).unwrap_or(u32::MAX),
        testing: search.testing.iter().map(name).collect(),
        found: (left == 1).then(|| name(&search.suspects[0])),
    })
}
