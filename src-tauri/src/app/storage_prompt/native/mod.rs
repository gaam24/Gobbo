//! The native message dialog of the first-start question. It runs before Tauri's event loop exists, so it cannot use
//! `tauri-plugin-dialog`; each platform keeps the one call it needs here.

#[cfg(target_os = "linux")]
mod gtk;
#[cfg(not(target_os = "linux"))]
mod rfd;

#[cfg(not(target_os = "linux"))]
pub(super) use self::rfd::message;
#[cfg(target_os = "linux")]
pub(super) use gtk::message;

/// The kind of message, which picks the icon and the urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Level {
    Info,
    Warning,
    Error,
}

/// The buttons of the dialog. A custom label is what the caller compares the answer against.
#[derive(Debug)]
pub(super) enum Buttons {
    Ok,
    OkCancelCustom(String, String),
    YesNoCancelCustom(String, String, String),
}

/// What the user pressed. `Custom` carries the label of the pressed button.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Answer {
    Ok,
    Cancel,
    Custom(String),
}
