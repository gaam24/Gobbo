//! Folder listings. A listing of a game folder is built from the three layers merged, with the lower entries that
//! whiteouts hide left out.
//!
//! - `session`: the listings a program is reading, and the calls that start, continue and end them.
//! - `listing`: the entries of a folder as the layers show them, and their `WIN32_FIND_DATAW` form.

mod listing;
mod session;

pub(crate) use listing::{Layers, dot_entries, find_layers, has_entry, has_visible_entries};
pub(crate) use session::{find_close, find_first, find_first_ex, find_next};
