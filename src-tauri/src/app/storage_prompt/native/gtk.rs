//! The first-start dialog on Linux, on the main thread. Tauri's GTK event loop must own the default main context on
//! the main thread; `rfd`'s GTK backend initializes GTK on a background thread, which makes `tao` fail with `Failed to
//! acquire ownership of main context, already acquired by another thread`. The dialog is the same GTK message dialog
//! `rfd` would show, without the extra thread.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

use super::{Answer, Buttons, Level};

pub(in crate::app::storage_prompt) fn message(
    level: Level,
    title: &str,
    description: &str,
    buttons: Buttons,
) -> Answer {
    if let Err(error) = gtk::init() {
        // Without GTK there is no dialog and no window either. The caller quits, and stderr keeps the reason.
        eprintln!("gobbo: could not initialize GTK for the start dialog: {error}");
        return Answer::Cancel;
    }

    let message_type = match level {
        Level::Info => gtk::MessageType::Info,
        Level::Warning => gtk::MessageType::Warning,
        Level::Error => gtk::MessageType::Error,
    };
    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .title(title)
        .text(title)
        .secondary_text(description)
        .message_type(message_type)
        .buttons(gtk::ButtonsType::None)
        .build();
    match &buttons {
        Buttons::Ok => {
            dialog.add_button("OK", gtk::ResponseType::Ok);
        }
        Buttons::OkCancelCustom(ok, cancel) => {
            dialog.add_button(ok, gtk::ResponseType::Ok);
            dialog.add_button(cancel, gtk::ResponseType::Cancel);
        }
        Buttons::YesNoCancelCustom(yes, no, cancel) => {
            dialog.add_button(yes, gtk::ResponseType::Yes);
            dialog.add_button(no, gtk::ResponseType::No);
            dialog.add_button(cancel, gtk::ResponseType::Cancel);
        }
    }

    // `gtk_dialog_run` is deprecated, and a nested main loop is what it does internally anyway.
    let answer = Rc::new(Cell::new(None));
    let main_loop = gtk::glib::MainLoop::new(None, false);
    {
        let answer = Rc::clone(&answer);
        let main_loop = main_loop.clone();
        dialog.connect_response(move |_, response| {
            answer.set(Some(answer_for(&buttons, response)));
            main_loop.quit();
        });
    }

    dialog.show_all();
    main_loop.run();
    dialog.close();
    answer.take().unwrap_or(Answer::Cancel)
}

/// Maps a GTK response to the label the caller compares. A custom button keeps its label, as `rfd` does, because the
/// caller decides which mode that label means.
fn answer_for(buttons: &Buttons, response: gtk::ResponseType) -> Answer {
    match (buttons, response) {
        (Buttons::Ok, gtk::ResponseType::Ok) => Answer::Ok,
        (Buttons::OkCancelCustom(ok, _), gtk::ResponseType::Ok) => Answer::Custom(ok.clone()),
        (Buttons::YesNoCancelCustom(yes, _, _), gtk::ResponseType::Yes) => {
            Answer::Custom(yes.clone())
        }
        (Buttons::YesNoCancelCustom(_, no, _), gtk::ResponseType::No) => Answer::Custom(no.clone()),
        (
            Buttons::OkCancelCustom(_, cancel) | Buttons::YesNoCancelCustom(_, _, cancel),
            gtk::ResponseType::Cancel,
        ) => Answer::Custom(cancel.clone()),
        _ => Answer::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_custom_button_returns_its_label() {
        let buttons = Buttons::YesNoCancelCustom(
            "Portable".to_string(),
            "Global".to_string(),
            "Quit".to_string(),
        );
        assert_eq!(
            answer_for(&buttons, gtk::ResponseType::Yes),
            Answer::Custom("Portable".to_string())
        );
        assert_eq!(
            answer_for(&buttons, gtk::ResponseType::No),
            Answer::Custom("Global".to_string())
        );
        assert_eq!(
            answer_for(&buttons, gtk::ResponseType::Cancel),
            Answer::Custom("Quit".to_string())
        );
    }

    #[test]
    fn closing_the_dialog_is_a_cancel() {
        let buttons = Buttons::OkCancelCustom("Start".to_string(), "Back".to_string());
        assert_eq!(
            answer_for(&buttons, gtk::ResponseType::DeleteEvent),
            Answer::Cancel
        );
    }
}
