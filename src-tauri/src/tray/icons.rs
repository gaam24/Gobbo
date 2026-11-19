//! The small tray icons. Windows uses a multi-size ICO; Linux uses a 32 px PNG. They are embedded so a portable copy
//! never has to find a resource file at runtime.

use tauri::image::Image;

use super::view::IconState;

#[cfg(windows)]
const IDLE: &[u8] = include_bytes!("../../icons/tray.ico");
#[cfg(windows)]
const CHANGES: &[u8] = include_bytes!("../../icons/tray-changes.ico");
#[cfg(windows)]
const WORKING: &[u8] = include_bytes!("../../icons/tray-working.ico");
#[cfg(windows)]
const ATTENTION: &[u8] = include_bytes!("../../icons/tray-attention.ico");

#[cfg(not(windows))]
const IDLE: &[u8] = include_bytes!("../../icons/tray.png");
#[cfg(not(windows))]
const CHANGES: &[u8] = include_bytes!("../../icons/tray-changes.png");
#[cfg(not(windows))]
const WORKING: &[u8] = include_bytes!("../../icons/tray-working.png");
#[cfg(not(windows))]
const ATTENTION: &[u8] = include_bytes!("../../icons/tray-attention.png");

/// Decodes the icon for one state. The bytes are part of the binary, so the only failure is a corrupt asset.
pub fn load(state: IconState) -> tauri::Result<Image<'static>> {
    let bytes = match state {
        IconState::Idle => IDLE,
        IconState::Changes => CHANGES,
        IconState::Working => WORKING,
        IconState::Attention => ATTENTION,
    };
    Image::from_bytes(bytes)
}
