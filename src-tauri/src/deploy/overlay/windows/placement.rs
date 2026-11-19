//! Where the hook DLL is found. The Windows overlay loads the hook from Gobbo's folder; it never copies a mod file into
//! the game folder. The mod layer is the only place the game's DLLs come from, and the hook makes them visible at the
//! paths the game expects.

use std::path::{Path, PathBuf};

use crate::deploy::DeployError;
use crate::error::FileError;

/// The hook's file name for the architecture of this Gobbo process.
const HOOK_FILE: &str = "gobbo_overlay_hook.dll";
/// The hook's file name for a 32-bit game or child process on 64-bit Windows.
const HOOK32_FILE: &str = "gobbo_overlay_hook32.dll";

/// DLL names the hook itself loads before the game's static imports. A mod file with one of these names cannot be
/// virtualized early: the loader already has the system module under that base name. The launcher leaves the import
/// alone and says so; the UI warning for a mod that ships one is still to be added.
const RESERVED: [&str; 1] = ["ws2_32.dll"];

/// The DLL names the hook loads before the game. A mod's copy cannot be virtualized early: the loader already has the
/// system module under that base name. The UI shows this warning in the deployment preview.
pub(super) fn reserved(name: &str) -> bool {
    RESERVED
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

/// The hook for this Gobbo process. Missing it is an error before anything is written.
pub(super) fn hook_dll() -> Result<PathBuf, DeployError> {
    let exe = std::env::current_exe().map_err(|e| FileError::new(Path::new("gobbo"), e))?;
    let next_to_exe = exe.with_file_name(HOOK_FILE);
    hook_file(HOOK_FILE).ok_or(DeployError::OverlayHookMissing { path: next_to_exe })
}

/// The hook for a 32-bit game. `None` when Gobbo was built without it; a 32-bit launch then fails with a clear error
/// instead of running the game unhooked.
pub(super) fn hook_dll32() -> Option<PathBuf> {
    hook_file(HOOK32_FILE)
}

/// A hook file next to the Gobbo executable, or one folder above it. The one above is the dev layout (`target/debug`);
/// the one next to it is the installed layout.
fn hook_file(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let next_to_exe = exe.with_file_name(name);
    let one_up = exe
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join(name));
    [Some(next_to_exe), one_up]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hook_reserved_names_are_case_insensitive() {
        assert!(reserved("WS2_32.DLL"));
        assert!(!reserved("version.dll"));
    }
}
