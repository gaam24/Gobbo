use std::fs;
use std::path::{Path, PathBuf};

use crate::games::GameDef;

/// Whether `root` is the game's folder on this platform. Windows checks `markers`; Linux also accepts a folder with
/// the files from `linux_markers`, so a native-only install is found.
pub fn has_markers(root: &Path, game: &GameDef) -> bool {
    has_markers_for(!crate::platform::is_windows(), root, game)
}

/// The game folder, or the only immediate child that has the markers. A pre-installed game often keeps the real folder
/// one level down (`Game/Game`); two matching children are ambiguous and give `None` when the parent does not match.
pub fn game_folder(root: &Path, game: &GameDef, is_linux: bool) -> Option<PathBuf> {
    if has_markers_for(is_linux, root, game) {
        return Some(root.to_path_buf());
    }

    let mut children = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|child| child.is_dir() && has_markers_for(is_linux, child, game));
    let first = children.next()?;
    children.next().is_none().then_some(first)
}

/// The marker rule with its platform given, so both sides can be tested on one machine. A folder qualifies when all
/// `markers` exist or, on Linux, when all `linux_markers` exist. `any_marker` relaxes each set the same way; with no
/// markers the folder itself is enough, and an empty `linux_markers` leaves the rule as it was.
pub fn has_markers_for(is_linux: bool, root: &Path, game: &GameDef) -> bool {
    root.is_dir() && satisfied_set(is_linux, root, game).is_some()
}

/// The marker set the folder satisfies under the rule of `has_markers_for`: the definition's `markers` first, then, on
/// Linux, its `linux_markers`. `None` when neither is satisfied. Every reader of the rule goes through here, so the
/// folder that is accepted and the marker that is shown cannot disagree.
fn satisfied_set<'a>(is_linux: bool, root: &Path, game: &'a GameDef) -> Option<&'a [String]> {
    /// Whether one set of markers is satisfied. An empty set needs no file, which keeps the folder itself enough.
    fn set_matches(markers: &[String], any: bool, root: &Path) -> bool {
        markers.is_empty()
            || if any {
                markers.iter().any(|marker| root.join(marker).exists())
            } else {
                markers.iter().all(|marker| root.join(marker).exists())
            }
    }

    let any = game.detect.any_marker;
    let markers = game.detect.markers.as_slice();
    let linux = game.detect.linux_markers.as_slice();

    if set_matches(markers, any, root) {
        Some(markers)
    } else if is_linux && !linux.is_empty() && set_matches(linux, any, root) {
        Some(linux)
    } else {
        None
    }
}

/// The marker file to show for a folder the rule accepts: the first file of the satisfied set that the folder has.
/// `None` when the folder is not accepted, or when the satisfied set names no file.
pub fn marker_file_for(is_linux: bool, root: &Path, game: &GameDef) -> Option<String> {
    if !root.is_dir() {
        return None;
    }

    satisfied_set(is_linux, root, game)?
        .iter()
        .find(|marker| root.join(marker).exists())
        .cloned()
}

/// The marker files a game folder of this game is asked to have on this platform, as a message shows them: the
/// definition's `markers`, and on Linux its `linux_markers` after ` /`, because either set is enough.
pub fn expected_markers(is_linux: bool, game: &GameDef) -> String {
    let mut sets = vec![game.detect.markers.join(", ")];

    if is_linux {
        sets.push(game.detect.linux_markers.join(", "));
    }

    sets.retain(|set| !set.is_empty());
    sets.join(" / ")
}
