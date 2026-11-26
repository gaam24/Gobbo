//! Updates for the portable archive. An installed copy updates through the updater plugin's installer. A portable copy
//! has no installer: this module downloads the signed portable archive through the plugin (which checks the signature
//! before it returns the bytes), replaces the program files next to the executable, keeps the data folder, and starts
//! the new copy.

pub mod archive;
pub mod commands;
mod model;
pub mod portable;
pub mod relaunch;
pub mod swap;

#[allow(
    unused_imports,
    reason = "the façade exposes the moved IPC types at `update::PortableUpdate` and `update::PortableUpdateStep`; callers import them from `model` directly"
)]
pub use model::{PortableUpdate, PortableUpdateStep};

use crate::platform::portable_target;
use serde::Serialize;

/// The update path of this copy, as the window needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    /// Installed with the installer: the updater plugin runs the installer.
    Installed,
    /// The portable archive: Gobbo replaces its program files itself.
    Portable,
    /// A portable copy on a system with no portable archive: nothing can update it.
    Unsupported,
}

/// The channel of a copy. Only the portable marker decides: the archive ships it and the installer does not.
pub fn channel(has_portable_marker: bool, os: &str, arch: &str) -> UpdateChannel {
    match (has_portable_marker, portable_target(os, arch)) {
        (false, _) => UpdateChannel::Installed,
        (true, Some(_)) => UpdateChannel::Portable,
        (true, None) => UpdateChannel::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_archive_has_its_own_manifest_key_on_both_systems() {
        assert_eq!(
            portable_target("windows", "x86_64"),
            Some("windows-x86_64-portable")
        );
        assert_eq!(
            portable_target("linux", "x86_64"),
            Some("linux-x86_64-portable")
        );
        assert_eq!(portable_target("linux", "aarch64"), None);
        assert_eq!(portable_target("macos", "x86_64"), None);
    }

    #[test]
    fn only_the_portable_marker_makes_a_copy_portable() {
        assert_eq!(
            channel(false, "windows", "x86_64"),
            UpdateChannel::Installed
        );
        assert_eq!(channel(true, "windows", "x86_64"), UpdateChannel::Portable);
        assert_eq!(channel(true, "linux", "x86_64"), UpdateChannel::Portable);
        assert_eq!(
            channel(true, "linux", "aarch64"),
            UpdateChannel::Unsupported
        );
        assert_eq!(channel(false, "linux", "aarch64"), UpdateChannel::Installed);
    }
}
