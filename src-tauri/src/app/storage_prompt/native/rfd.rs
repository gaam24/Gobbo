//! The first-start dialog on Windows and macOS, through the same `rfd` that backs `tauri-plugin-dialog`. It runs on
//! the calling thread there, so it can be shown before Tauri starts.

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use super::{Answer, Buttons, Level};

pub(in crate::app::storage_prompt) fn message(
    level: Level,
    title: &str,
    description: &str,
    buttons: Buttons,
) -> Answer {
    let level = match level {
        Level::Info => MessageLevel::Info,
        Level::Warning => MessageLevel::Warning,
        Level::Error => MessageLevel::Error,
    };
    let buttons = match buttons {
        Buttons::Ok => MessageButtons::Ok,
        Buttons::OkCancelCustom(ok, cancel) => MessageButtons::OkCancelCustom(ok, cancel),
        Buttons::YesNoCancelCustom(yes, no, cancel) => {
            MessageButtons::YesNoCancelCustom(yes, no, cancel)
        }
    };
    let result = MessageDialog::new()
        .set_title(title)
        .set_description(description)
        .set_buttons(buttons)
        .set_level(level)
        .show();
    match result {
        MessageDialogResult::Ok => Answer::Ok,
        MessageDialogResult::Custom(label) => Answer::Custom(label),
        // None of the button sets above uses the standard Yes/No buttons.
        MessageDialogResult::Yes | MessageDialogResult::No | MessageDialogResult::Cancel => {
            Answer::Cancel
        }
    }
}
