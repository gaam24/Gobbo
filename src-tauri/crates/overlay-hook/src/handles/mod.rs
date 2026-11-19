//! Handles to files in a layer. A program that asks such a handle for its final path (`GetFinalPathNameByHandleW`) must
//! get the game path. `std::filesystem::canonical` and `relative` do this, and they open the path to ask: with the
//! layer's path as the answer, the file reads as lying outside the game folder, and the program refuses it (Cyber Engine
//! Tweaks refuses its mods that way). The hook maps the final path of the handle; it does not keep a handle table.
//!
//! - `final_path`: `GetFinalPathNameByHandleW`, answered with the game path for a path in a layer.
//! - `mapped`: `GetMappedFileNameW`, answered in the NT form with the game path for a mapped file in a layer.
//! - `path`: the paths a handle has, in the layer and as the program sees them, and the game folder.

mod final_path;
mod mapped;
mod path;

pub(crate) use final_path::final_path;
pub(crate) use mapped::mapped_file_name;
pub(crate) use path::{
    LayerKind, final_path_units, guest_module_path_units, layer_kind, layer_kind_units, layer_path,
    layer_path_kind, map_layer_path, map_layer_path_cached, map_layer_path_nt, outside_path_units,
    program_path, relative_in_layers, relative_in_lower_layers, relative_in_upper,
};
