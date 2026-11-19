//! The overwrite folder and which mod wins each file.

use crate::deploy::overwrite::{self, OverwriteView};

crate::commands::game_commands! {
    /// The files the game and its tools wrote into the overlay of the active profile.
    query overwrite_files() -> OverwriteView = overwrite::overwrite_files;

    /// Turns the files the game wrote into a new mod, enabled at the top of the list.
    change overwrite_to_mod(name: String) = |root, game| overwrite::overwrite_to_mod(root, game, &name);

    /// Removes the files the game wrote into the overlay of the active profile.
    change clear_overwrite() = overwrite::clear_overwrite;

    /// Chooses which mod's copy of a file wins in the active profile. `family` `None` goes back to the list order.
    change set_file_winner(path: String, family: Option<String>) =
        |root, game| crate::library::set_file_winner(root, game, &path, family.as_deref());

    /// Chooses the winning mod for several file paths at once, as one undoable change.
    change set_file_winners(paths: Vec<String>, family: String) =
        |root, game| crate::overwrite::set_file_winners(root, game, &paths, &family);
}
