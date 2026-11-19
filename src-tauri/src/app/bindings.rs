//! The IPC types as the frontend reads them. In a debug build `src/bindings.ts` is written from the commands; a test
//! fails when the committed file is out of date.

use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(debug_assertions)]
use crate::app::BINDINGS;

/// The IPC types as `bindings.ts` should hold them.
pub fn text(builder: &tauri_specta::Builder<tauri::Wry>) -> Result<String, String> {
    static RUN: AtomicU32 = AtomicU32::new(0);
    let run = RUN.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("gobbo-bindings-{}-{run}.ts", std::process::id()));
    builder
        .export(specta_typescript::Typescript::default(), &path)
        .map_err(|error| format!("could not generate IPC types: {error}"))?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read the generated IPC types: {error}"))?;
    let _ = std::fs::remove_file(&path);
    Ok(text)
}

/// Writes `bindings.ts` when the commands changed. An unchanged file stays untouched: every start, also the short one
/// that hands a clicked link to the running app, would otherwise make the dev server reload the window.
#[cfg(debug_assertions)]
pub fn update(builder: &tauri_specta::Builder<tauri::Wry>) {
    let Ok(text) = text(builder) else {
        crate::infra::log::error("could not generate the IPC bindings");
        return;
    };
    if std::fs::read_to_string(BINDINGS).ok().as_deref() != Some(text.as_str())
        && let Err(error) = std::fs::write(BINDINGS, text)
    {
        crate::infra::log::error(format!("could not write the IPC bindings: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::BINDINGS;
    use crate::commands;

    /// `bindings.ts` must match the commands. When this test fails, run `write_bindings` or the app in debug mode.
    #[test]
    fn bindings_match_the_commands() {
        let committed = std::fs::read_to_string(BINDINGS).unwrap_or_default();
        assert!(
            text(&commands::builder()).unwrap() == committed,
            "bindings.ts is out of date; run the app in debug mode to regenerate it"
        );
    }

    /// Writes the generated bindings into the repository. Run by hand after changing an IPC type.
    #[test]
    #[ignore = "writes src/bindings.ts"]
    fn write_bindings() {
        std::fs::write(BINDINGS, text(&commands::builder()).unwrap()).unwrap();
    }
}
