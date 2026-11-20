//! The first-start question: where Gobbo keeps its data. It runs before Tauri, because the data folder, the window
//! state and the webview profile are fixed when the app is built. The question is a one-time native dialog, outside the
//! React interface; the strings below are the only user-facing text that does not live in `src/locales`.
//!
//! The dialog backend is per platform: `rfd` on Windows and macOS, and GTK on the main thread on Linux. `rfd`'s GTK
//! backend initializes GTK on a background thread, which makes Tauri's event loop fail when it starts afterwards.

mod native;

use native::{Answer, Buttons, Level};

use crate::error::AppError;
use crate::infra::storage::{self, PromptContext, Resolved, StorageLocation, StorageMode};

/// What the start does after the storage question.
#[derive(Debug)]
pub enum Start {
    /// The data folder is known; start the window with it.
    Run(StorageLocation),
    /// The user closed the question. Nothing was written.
    Quit,
}

/// Resolves the data folder, asking on the first start. A known decision never shows a dialog.
pub fn resolve_or_prompt() -> Result<Start, AppError> {
    let exe_dir = storage::exe_dir()?;
    let global = storage::global_root().ok();

    match storage::resolve(&exe_dir, global.as_deref(), storage::env_override()) {
        Resolved::Known(location) => Ok(Start::Run(location)),
        Resolved::Choice(context) => {
            let mut prompt = NativePrompt::new();
            Ok(choose(&context, &mut prompt).map_or(Start::Quit, Start::Run))
        }
    }
}

/// Shows an early start failure. There is no window and no translated interface yet, so this dialog is English.
pub fn report_fatal(error: &AppError) {
    eprintln!("gobbo: {error}");
    let _ = native::message(
        Level::Error,
        "Gobbo could not start",
        &format!("Gobbo could not start.\n\n{error}"),
        Buttons::Ok,
    );
}

/// The answers the first-start flow needs. Tests use a fake implementation; the app uses the native dialog.
trait Prompter {
    fn choose(&mut self, context: &PromptContext) -> Option<StorageMode>;
    fn confirm_portable(&mut self, context: &PromptContext) -> bool;
    /// Whether to ask again after a failed write.
    fn error(&mut self, error: &AppError) -> bool;
}

/// Asks until a choice is written, or until the user quits. `None` means quit.
fn choose(context: &PromptContext, prompt: &mut impl Prompter) -> Option<StorageLocation> {
    let mut context = context.clone();

    loop {
        let mode = prompt.choose(&context)?;

        if mode == StorageMode::Portable
            && context.global_data_exists
            && !prompt.confirm_portable(&context)
        {
            continue;
        }

        match storage::commit(&context, mode) {
            Ok(location) => return Some(location),
            Err(error) => {
                if !prompt.error(&error) {
                    return None;
                }
                // A failed portable write may mean the folder became read-only; the next question must not offer it.
                context.portable_available = storage::can_write(&context.exe_dir);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    En,
    Pl,
}

fn language(locale: Option<&str>) -> Language {
    let primary = locale.and_then(|locale| locale.split(['-', '_']).next());

    if primary.is_some_and(|part| part.eq_ignore_ascii_case("pl")) {
        Language::Pl
    } else {
        Language::En
    }
}

/// The few strings of the pre-Tauri dialog. They are not in `src/locales` because React has not started yet; keep
/// them next to the only code that shows them.
struct Strings {
    title: &'static str,
    intro: &'static str,
    portable: &'static str,
    portable_hint: &'static str,
    global: &'static str,
    global_hint: &'static str,
    quit: &'static str,
    unavailable: &'static str,
    no_folder: &'static str,
    portable_unavailable: &'static str,
    existing_data: &'static str,
    decision_hint: &'static str,
    confirm_title: &'static str,
    confirm_body: &'static str,
    confirm_start: &'static str,
    confirm_back: &'static str,
    error_title: &'static str,
    error_body: &'static str,
    error_read_only: &'static str,
    error_write: &'static str,
    error_io: &'static str,
    error_appdata: &'static str,
    try_again: &'static str,
}

impl Strings {
    const EN: Self = Self {
        title: "Where should Gobbo keep its data?",
        intro: "Gobbo keeps its library, profiles and settings in one place. Choose where:",
        portable: "Portable",
        portable_hint: "Everything stays in the data folder next to the program. Moving the folder moves the library too.",
        global: "Global",
        global_hint: "The usual Gobbo folder, shared by every copy on this computer.",
        quit: "Quit",
        unavailable: "Not available",
        no_folder: "Neither the program folder nor the global data folder can be written. Move Gobbo to a writable folder or fix the system's app data folder.",
        portable_unavailable: "The program folder is read-only, so Portable is not available.",
        existing_data: "Existing Gobbo data was found in the global folder. Choosing Portable starts with an empty library; the existing data stays in the global folder.",
        decision_hint: "The choice is saved in gobbo.storage.json. Delete that file and start Gobbo again to choose again.",
        confirm_title: "Start with an empty library?",
        confirm_body: "The existing library stays in the global folder and will not be moved. The portable copy starts with an empty list.",
        confirm_start: "Start portable",
        confirm_back: "Back",
        error_title: "Gobbo could not save the choice",
        error_body: "The data folder could not be prepared:",
        error_read_only: "The program folder is read-only:",
        error_write: "The choice could not be saved in:",
        error_io: "The folder could not be created or written:",
        error_appdata: "The system did not provide an AppData folder.",
        try_again: "Try again",
    };

    const PL: Self = Self {
        title: "Gdzie Gobbo ma trzymać dane?",
        intro: "Gobbo trzyma bibliotekę, profile i ustawienia w jednym miejscu. Wybierz gdzie:",
        portable: "Portable",
        portable_hint: "Wszystko zostaje w folderze data przy programie. Przeniesienie folderu przenosi też bibliotekę.",
        global: "Globalny",
        global_hint: "Zwykły folder Gobbo, wspólny dla każdej kopii na tym komputerze.",
        quit: "Zakończ",
        unavailable: "Niedostępny",
        no_folder: "Ani folder programu, ani globalny folder danych nie pozwalają na zapis. Przenieś Gobbo do zapisywalnego folderu albo napraw systemowy folder danych.",
        portable_unavailable: "Folder programu jest tylko do odczytu, więc tryb Portable jest niedostępny.",
        existing_data: "Znaleziono istniejące dane Gobbo w folderze globalnym. Wybór Portable zacznie z pustą biblioteką; istniejące dane zostaną w folderze globalnym.",
        decision_hint: "Wybór zostanie zapisany w pliku gobbo.storage.json. Usuń ten plik i uruchom Gobbo ponownie, aby wybrać jeszcze raz.",
        confirm_title: "Zacząć z pustą biblioteką?",
        confirm_body: "Istniejąca biblioteka zostanie w folderze globalnym i nie zostanie przeniesiona. Kopia portable zacznie z pustą listą.",
        confirm_start: "Zacznij portable",
        confirm_back: "Wróć",
        error_title: "Nie udało się zapisać wyboru",
        error_body: "Nie udało się przygotować folderu danych:",
        error_read_only: "Folder programu jest tylko do odczytu:",
        error_write: "Nie udało się zapisać wyboru w:",
        error_io: "Nie udało się utworzyć lub zapisać folderu:",
        error_appdata: "System nie udostępnił folderu AppData.",
        try_again: "Spróbuj ponownie",
    };

    fn for_locale(locale: Option<&str>) -> Self {
        match language(locale) {
            Language::En => Self::EN,
            Language::Pl => Self::PL,
        }
    }
}

struct NativePrompt {
    strings: Strings,
}

impl NativePrompt {
    fn new() -> Self {
        Self {
            strings: Strings::for_locale(sys_locale::get_locale().as_deref()),
        }
    }

    fn body(&self, context: &PromptContext) -> String {
        let global = context.global_root.as_ref().map_or_else(
            || self.strings.unavailable.to_string(),
            |path| path.display().to_string(),
        );
        let mut text = format!(
            "{}\n\n{}:  {}\n{}\n\n{}:  {}\n{}",
            self.strings.intro,
            self.strings.portable,
            context.portable_root.display(),
            self.strings.portable_hint,
            self.strings.global,
            global,
            self.strings.global_hint,
        );

        if !context.portable_available {
            text.push_str("\n\n");
            text.push_str(self.strings.portable_unavailable);
        }

        if context.global_data_exists {
            text.push_str("\n\n");
            text.push_str(self.strings.existing_data);
        }

        text.push_str("\n\n");
        text.push_str(self.strings.decision_hint);
        text
    }
}

impl Prompter for NativePrompt {
    fn choose(&mut self, context: &PromptContext) -> Option<StorageMode> {
        let portable = self.strings.portable.to_string();
        let global = self.strings.global.to_string();
        let quit = self.strings.quit.to_string();
        let both = context.portable_available && context.global_root.is_some();
        let portable_is_yes = context.default_mode == StorageMode::Portable;
        let buttons = if both {
            let (yes, no) = if portable_is_yes {
                (portable.clone(), global.clone())
            } else {
                (global.clone(), portable.clone())
            };
            Buttons::YesNoCancelCustom(yes, no, quit)
        } else if context.portable_available {
            Buttons::OkCancelCustom(portable.clone(), quit)
        } else if context.global_root.is_some() {
            Buttons::OkCancelCustom(global.clone(), quit)
        } else {
            // Neither folder can be written. The caller cannot start, so show the reason and quit.
            let _ = native::message(
                Level::Error,
                self.strings.error_title,
                self.strings.no_folder,
                Buttons::Ok,
            );
            return None;
        };
        let result = native::message(
            Level::Info,
            self.strings.title,
            &self.body(context),
            buttons,
        );
        answer(result, &portable, &global, portable_is_yes)
    }

    fn confirm_portable(&mut self, _context: &PromptContext) -> bool {
        confirm(
            Level::Warning,
            self.strings.confirm_title,
            self.strings.confirm_body.to_string(),
            self.strings.confirm_start,
            self.strings.confirm_back,
        )
    }

    fn error(&mut self, error: &AppError) -> bool {
        let body = format!(
            "{}\n\n{}",
            self.strings.error_body,
            describe(error, &self.strings)
        );
        confirm(
            Level::Error,
            self.strings.error_title,
            body,
            self.strings.try_again,
            self.strings.quit,
        )
    }
}

/// Asks a yes/no question with two custom labels and reports whether the user chose `ok`.
fn confirm(level: Level, title: &str, body: String, ok: &str, cancel: &str) -> bool {
    let result = native::message(
        level,
        title,
        &body,
        Buttons::OkCancelCustom(ok.to_string(), cancel.to_string()),
    );
    matches!(result, Answer::Ok) || matches!(result, Answer::Custom(label) if label == ok)
}

fn with_path(prefix: &str, error: &AppError) -> String {
    match error.params.get("path") {
        Some(path) if !path.is_empty() => format!("{prefix} {path}"),
        _ => prefix.to_string(),
    }
}

/// The few start errors that have a stable code. Anything else keeps the code and its parameters for a bug report.
fn describe(error: &AppError, strings: &Strings) -> String {
    match error.code.as_str() {
        "storage.portable_unavailable" => with_path(strings.error_read_only, error),
        "storage.decision_write_failed" => with_path(strings.error_write, error),
        "io.failed" => with_path(strings.error_io, error),
        "settings.appdata_missing" => strings.error_appdata.to_string(),
        _ => error.to_string(),
    }
}

/// Maps a native dialog answer back to a mode. Custom buttons return their label; the standard ones are used when only
/// one folder is available.
fn answer(
    result: Answer,
    portable: &str,
    global: &str,
    portable_is_yes: bool,
) -> Option<StorageMode> {
    match result {
        Answer::Custom(label) if label == portable => Some(StorageMode::Portable),
        Answer::Custom(label) if label == global => Some(StorageMode::Global),
        Answer::Ok => Some(if portable_is_yes {
            StorageMode::Portable
        } else {
            StorageMode::Global
        }),
        Answer::Cancel | Answer::Custom(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;

    use super::*;
    use crate::infra::storage::Resolved;
    use crate::test_support::scratch;

    struct FakePrompt {
        choices: VecDeque<Option<StorageMode>>,
        confirms: VecDeque<bool>,
        errors: VecDeque<bool>,
    }

    impl FakePrompt {
        fn new(choices: &[Option<StorageMode>]) -> Self {
            Self {
                choices: choices.iter().copied().collect(),
                confirms: VecDeque::new(),
                errors: VecDeque::new(),
            }
        }

        fn with_confirms(mut self, confirms: &[bool]) -> Self {
            self.confirms = confirms.iter().copied().collect();
            self
        }

        fn with_errors(mut self, errors: &[bool]) -> Self {
            self.errors = errors.iter().copied().collect();
            self
        }
    }

    impl Prompter for FakePrompt {
        fn choose(&mut self, _context: &PromptContext) -> Option<StorageMode> {
            self.choices.pop_front().flatten()
        }

        fn confirm_portable(&mut self, _context: &PromptContext) -> bool {
            self.confirms.pop_front().unwrap_or(false)
        }

        fn error(&mut self, _error: &AppError) -> bool {
            self.errors.pop_front().unwrap_or(false)
        }
    }

    fn context(name: &str, marker: bool) -> PromptContext {
        let root = scratch!(name);
        let exe = root.join("program");
        let global = root.join("global");
        fs::create_dir_all(&exe).unwrap();

        if marker {
            fs::write(storage::portable_marker(&exe), "").unwrap();
        }

        match storage::resolve(&exe, Some(&global), None) {
            Resolved::Choice(context) => context,
            Resolved::Known(_) => panic!("expected a question"),
        }
    }

    fn with_global_data(name: &str) -> PromptContext {
        let mut context = context(name, true);
        let global = context.global_root.as_ref().expect("a global folder");
        fs::create_dir_all(global).unwrap();
        fs::write(global.join("settings.json"), "{}").unwrap();
        context.global_data_exists = true;
        context.default_mode = StorageMode::Global;
        context
    }

    #[test]
    fn polish_locales_choose_the_polish_text() {
        assert_eq!(language(Some("pl")), Language::Pl);
        assert_eq!(language(Some("pl-PL")), Language::Pl);
        assert_eq!(language(Some("PL_pl")), Language::Pl);
        assert_eq!(language(Some("en-US")), Language::En);
        assert_eq!(language(None), Language::En);
    }

    #[test]
    fn a_missing_answer_quits_without_writing() {
        let context = context("storage-prompt-quit", true);
        let mut prompt = FakePrompt::new(&[None]);
        assert!(choose(&context, &mut prompt).is_none());
        assert!(!context.local_decision_file.exists());
        assert!(!context.portable_root.exists());
    }

    #[test]
    fn choosing_portable_writes_the_local_decision() {
        let context = context("storage-prompt-portable", true);
        let mut prompt = FakePrompt::new(&[Some(StorageMode::Portable)]);
        let location = choose(&context, &mut prompt).expect("a choice");
        assert_eq!(location.mode, StorageMode::Portable);
        assert!(context.local_decision_file.is_file());
        assert!(context.portable_root.is_dir());
    }

    #[test]
    fn choosing_global_writes_the_global_folder() {
        let context = context("storage-prompt-global", false);
        let mut prompt = FakePrompt::new(&[Some(StorageMode::Global)]);
        let location = choose(&context, &mut prompt).expect("a choice");
        assert_eq!(location.mode, StorageMode::Global);
        assert!(location.root.is_dir());
        assert!(context.local_decision_file.is_file());
    }

    #[test]
    fn portable_with_existing_data_asks_for_confirmation() {
        let context = with_global_data("storage-prompt-confirm");
        let mut prompt =
            FakePrompt::new(&[Some(StorageMode::Portable), Some(StorageMode::Portable)])
                .with_confirms(&[false, true]);
        let location = choose(&context, &mut prompt).expect("a choice");
        assert_eq!(location.mode, StorageMode::Portable);
        assert!(context.portable_root.is_dir());
        assert!(
            context
                .global_root
                .as_ref()
                .is_some_and(|global| global.join("settings.json").is_file())
        );
    }

    #[test]
    fn a_failed_write_can_ask_again() {
        let mut context = context("storage-prompt-retry", false);
        context.portable_available = false;
        let mut prompt = FakePrompt::new(&[Some(StorageMode::Portable), Some(StorageMode::Global)])
            .with_errors(&[true]);
        let location = choose(&context, &mut prompt).expect("a choice");
        assert_eq!(location.mode, StorageMode::Global);
    }

    #[test]
    fn a_refused_retry_quits() {
        let mut context = context("storage-prompt-refused", false);
        context.portable_available = false;
        let mut prompt = FakePrompt::new(&[Some(StorageMode::Portable)]).with_errors(&[false]);
        assert!(choose(&context, &mut prompt).is_none());
    }

    #[test]
    fn the_answer_maps_custom_labels() {
        assert_eq!(
            answer(
                Answer::Custom("Portable".into()),
                "Portable",
                "Global",
                false
            ),
            Some(StorageMode::Portable)
        );
        assert_eq!(
            answer(Answer::Custom("Global".into()), "Portable", "Global", true),
            Some(StorageMode::Global)
        );
        assert_eq!(
            answer(Answer::Custom("Quit".into()), "Portable", "Global", true),
            None
        );
        assert_eq!(
            answer(Answer::Ok, "Portable", "Global", true),
            Some(StorageMode::Portable)
        );
        assert_eq!(
            answer(Answer::Ok, "Portable", "Global", false),
            Some(StorageMode::Global)
        );
    }

    #[test]
    fn a_start_error_names_the_folder() {
        let error = AppError::new("io.failed").with("path", r"C:\Gobbo\data");
        let text = describe(&error, &Strings::EN);
        assert!(text.contains(r"C:\Gobbo\data"), "{text}");
    }

    #[test]
    fn a_failed_portable_choice_does_not_leave_a_decision() {
        let mut context = context("storage-prompt-failed", true);
        context.portable_available = false;
        let mut prompt = FakePrompt::new(&[Some(StorageMode::Portable)]).with_errors(&[false]);
        let _ = choose(&context, &mut prompt);
        assert!(!context.local_decision_file.exists());
        assert!(!context.portable_root.exists());
    }
}
