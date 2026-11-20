//! Where a path the program uses really is: the layer the layers say holds it, the upper layer for a write, and the
//! whiteouts that hide what the game deleted. Every file call of the hook goes through here first.
//!
//! - `names`: the full path of a name, and whether it is inside the game folder.
//! - `session`: the layer paths and the UTF-16 root prefix, cached once.
//! - `probe`: one attribute query per path, with a missing or denied answer.
//! - `target`: the layer a path resolves to, what is visible, and the copy-up of a file before a write.
//! - `disk`: the plain file-system checks the hook makes itself.

mod disk;
mod names;
mod probe;
mod session;
mod spelling;
mod target;

pub(crate) use names::{
    Place, absolute, place, place_path, place_units, shown_name, unsafe_name, unsafe_units,
};
pub(crate) use session::{long_path, paths, short_path_units};
pub(crate) use spelling::{find_exact, with_pattern};
pub(crate) use target::{
    Failure, Redirected, Target, copy_up, in_lower, is_visible_dir, make_write_target, missing,
    name_taken, redirect_opened, redirect_relative_on, shown, visible, writes_requested,
};
