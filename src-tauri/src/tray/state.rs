//! The tray's presentation state: the game and busy code the window reports, the translated labels, and the icon handle.
//! This is not game state — it is the last thing the window told the tray, so the menu still makes sense with no window.

use std::sync::Mutex;

use tauri::{Wry, tray::TrayIcon};

use super::labels::TrayLabels;

#[derive(Default)]
struct Inner {
    game: Option<String>,
    busy: Option<String>,
    labels: TrayLabels,
}

/// State owned by the tray module and kept in Tauri's managed state for the whole process.
#[derive(Default)]
pub struct TrayState {
    inner: Mutex<Inner>,
    tray: Mutex<Option<TrayIcon<Wry>>>,
}

impl TrayState {
    /// Creates the state with English labels and no game until the window reports one.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                game: None,
                busy: None,
                labels: TrayLabels::default(),
            }),
            tray: Mutex::new(None),
        }
    }

    /// The translated labels, cloned for one render.
    pub fn labels(&self) -> TrayLabels {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .labels
            .clone()
    }

    /// Replaces the labels when the interface language changes.
    pub fn set_labels(&self, labels: TrayLabels) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .labels = labels;
    }

    /// The game and busy code the window last reported.
    pub fn context(&self) -> (Option<String>, Option<String>) {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (inner.game.clone(), inner.busy.clone())
    }

    /// Remembers which game the window shows and what it is busy with. Called on every relevant state change, so the
    /// tray keeps the last useful context after the window is hidden.
    pub fn set_context(&self, game: Option<String>, busy: Option<String>) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.game = game;
        inner.busy = busy;
    }

    /// Keeps the icon handle so the menu, tooltip and icon can be replaced without creating a second one.
    pub fn set_tray(&self, icon: TrayIcon<Wry>) {
        *self
            .tray
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(icon);
    }

    /// The icon handle, cloned because `TrayIcon` is reference-counted.
    pub fn tray(&self) -> Option<TrayIcon<Wry>> {
        self.tray
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
