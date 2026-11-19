//! Getting mods from Nexus: from an `nxm://` link (the "Mod Manager Download" button), or straight from the API for
//! a premium account. A download lands in the game's `downloads/` folder, with a small file next to it that names its
//! Nexus mod and file, and waits there for its import. The import keeps its own copy, so the download goes after it.

mod file;

pub use file::download_into;
