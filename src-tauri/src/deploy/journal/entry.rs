use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One change the journal records before it reaches the disk, so undo knows what may have changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum JournalOp {
    /// A folder created by Gobbo.
    DirCreated { path: PathBuf },
    /// A file deployed from the library.
    FileDeployed { path: PathBuf },
    /// The sections Gobbo changed in the settings file, and whether the file existed before. Undo puts back only these.
    SettingsSections {
        path: PathBuf,
        existed: bool,
        touched: Vec<crate::deploy::settings::Touched>,
    },
    /// An overlay session: the mod layer in `mods` stays after undo, for the next deployment of this profile.
    /// `overwrite`, which holds the game's writes, stays too.
    LayerPrepared { mods: PathBuf, overwrite: PathBuf },
}
