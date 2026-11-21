//! The configuration: the app's settings file and each game's settings. Provider keys live in the OS credential store.

use crate::context::Data;
use crate::error::AppError;
use crate::infra::json;
use serde::{Deserialize, Deserializer, Serialize};

pub use crate::context::{Settings, save_settings};

/// App settings shared by all games, e.g. the interface language. Provider keys live in the OS credential store, not
/// here, so they are never written to disk in plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppSettings {
    #[serde(default)]
    pub language: Language,
    /// The right inspector is hidden, so the mod list has more room.
    #[serde(default)]
    pub inspector_collapsed: bool,
    /// The providers, by id, whose download links Gobbo takes (for example `nexus` for `nxm://`), so a provider's "Mod
    /// Manager Download" button sends the file here. Empty by default, because only one program on the computer can take
    /// the links.
    #[serde(default, deserialize_with = "taken_links")]
    #[specta(type = Vec<String>)]
    pub source_links: Vec<String>,
    /// The main window opens with the size and place it had when it was closed. Off, it opens at the default size.
    #[serde(default = "on_by_default")]
    pub remember_window: bool,
    /// What closing the main window does. `Ask` opens the closing dialog; a concrete action runs without asking and is
    /// what the dialog remembers when "don't show again" is checked. `Ask` from the start. A file from before the dialog
    /// has the old tray switch instead; see `saved_close_action`.
    #[serde(
        default,
        alias = "minimize_to_tray",
        deserialize_with = "saved_close_action"
    )]
    #[specta(type = CloseAction)]
    pub close_action: CloseAction,
    /// Windows 11 Efficiency mode: off, only while the window is in the background, or always. Other systems keep it
    /// off because they have no equivalent.
    #[serde(default)]
    pub efficiency_mode: EfficiencyMode,
    /// Asks the saved providers for newer revisions of the active profile's collections when a game opens. One read
    /// per collection, on the same budget as every other provider read. On from the start.
    #[serde(default = "on_by_default")]
    pub check_collections_on_launch: bool,
    /// Asks the official catalog for new or changed game definitions when the app starts. On from the start.
    #[serde(default = "on_by_default")]
    pub check_catalog_on_launch: bool,
}

/// What closing the main window does. `Ask` shows the dialog with the three concrete choices; the others perform one
/// of them directly. A concrete action is what "don't show this again" writes to the settings.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CloseAction {
    /// Show the closing dialog every time. The default for both new and old settings files.
    #[default]
    Ask,
    /// Stop a running game with the usual confirmation and end the app.
    Exit,
    /// Minimize the window to the taskbar; background work keeps running.
    Minimize,
    /// Hide the window in the system tray; the tray is the way back in.
    HideToTray,
}

/// When Gobbo asks Windows for Efficiency mode (`EcoQoS`). `Auto` keeps the app at normal priority while the user works
/// in it and lowers it only in the background.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum EfficiencyMode {
    Off,
    #[default]
    Auto,
    Always,
}

impl EfficiencyMode {
    /// Whether the process should be in Efficiency mode for the window state. `focused` is true when the main window
    /// has focus; `minimized` is true when it is minimized.
    pub const fn active(self, focused: bool, minimized: bool) -> bool {
        match self {
            Self::Off => false,
            Self::Auto => !focused || minimized,
            Self::Always => true,
        }
    }
}

/// Applies an Efficiency mode choice. A platform without it, or Windows 10, simply keeps the normal priority.
#[cfg_attr(
    not(windows),
    expect(
        clippy::missing_const_for_fn,
        reason = "the Windows implementation calls the system, so the function cannot be const there"
    )
)]
pub fn apply_efficiency(mode: EfficiencyMode, focused: bool, minimized: bool) {
    let _ = crate::platform::set_efficiency_mode(mode.active(focused, minimized));
}

/// A switch that settings saved before it existed have no value for: it starts on.
pub(super) const fn on_by_default() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            inspector_collapsed: false,
            source_links: Vec::new(),
            remember_window: true,
            close_action: CloseAction::Ask,
            efficiency_mode: EfficiencyMode::Auto,
            check_collections_on_launch: true,
            check_catalog_on_launch: true,
        }
    }
}

/// The setting as it was saved. Before each provider had its own switch, one `true` or `false` covered every provider.
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedSourceLinks {
    Providers(Vec<String>),
    AllProviders(bool),
}

/// Reads `source_links` from either form. A saved `true` takes the links of every provider that has links, as the single
/// switch did before.
fn taken_links<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Ok(match SavedSourceLinks::deserialize(deserializer)? {
        SavedSourceLinks::Providers(ids) => ids,
        SavedSourceLinks::AllProviders(false) => Vec::new(),
        SavedSourceLinks::AllProviders(true) => crate::providers::providers()
            .iter()
            .filter(|provider| !provider.schemes().is_empty())
            .map(|provider| provider.id().as_str().to_string())
            .collect(),
    })
}

/// The closing setting as it was saved. Before the closing dialog, one switch said whether closing hid the window in the
/// tray. The two names never appear in the same file: a save writes only `close_action`.
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedCloseAction {
    Action(CloseAction),
    TraySwitch(bool),
}

/// Reads `close_action` from either form. A switch turned off was a deliberate "closing quits", so it becomes `Exit`. A
/// switch left on was the old default, which most people never chose, so the dialog asks once.
fn saved_close_action<'de, D: Deserializer<'de>>(deserializer: D) -> Result<CloseAction, D::Error> {
    Ok(match SavedCloseAction::deserialize(deserializer)? {
        SavedCloseAction::Action(action) => action,
        SavedCloseAction::TraySwitch(false) => CloseAction::Exit,
        SavedCloseAction::TraySwitch(true) => CloseAction::Ask,
    })
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, CloseAction, EfficiencyMode};

    fn read(json: &str) -> AppSettings {
        serde_json::from_str(json).expect("the settings must parse")
    }

    #[test]
    fn a_single_saved_switch_takes_the_links_of_every_provider() {
        assert_eq!(
            read(r#"{"source_links": true}"#).source_links,
            ["nexus", "thunderstore"]
        );
        assert!(read(r#"{"source_links": false}"#).source_links.is_empty());
    }

    #[test]
    fn a_list_of_providers_is_taken_as_it_is() {
        assert_eq!(
            read(r#"{"source_links": ["thunderstore"]}"#).source_links,
            ["thunderstore"]
        );
    }

    #[test]
    fn settings_without_links_take_none() {
        assert!(read("{}").source_links.is_empty());
        assert!(AppSettings::default().source_links.is_empty());
    }

    #[test]
    fn an_old_settings_file_checks_the_catalog_on_launch() {
        assert!(read("{}").check_catalog_on_launch);
        assert!(AppSettings::default().check_catalog_on_launch);
        assert!(!read(r#"{"check_catalog_on_launch":false}"#).check_catalog_on_launch);
    }

    #[test]
    fn an_old_settings_file_asks_on_close() {
        assert_eq!(read("{}").close_action, CloseAction::Ask);
        assert_eq!(
            read(r#"{"minimize_to_tray":true}"#).close_action,
            CloseAction::Ask
        );
        assert_eq!(AppSettings::default().close_action, CloseAction::Ask);
    }

    #[test]
    fn an_old_switch_turned_off_keeps_closing_the_app() {
        assert_eq!(
            read(r#"{"minimize_to_tray":false}"#).close_action,
            CloseAction::Exit
        );
    }

    #[test]
    fn a_saved_close_action_is_written_under_its_new_name() {
        let settings = read(r#"{"minimize_to_tray":false}"#);
        let json = serde_json::to_string(&settings).expect("the settings must serialize");
        assert!(json.contains(r#""close_action":"exit""#));
        assert!(!json.contains("minimize_to_tray"));
        assert_eq!(read(&json).close_action, CloseAction::Exit);
    }

    #[test]
    fn a_saved_close_action_is_read() {
        assert_eq!(
            read(r#"{"close_action":"ask"}"#).close_action,
            CloseAction::Ask
        );
        assert_eq!(
            read(r#"{"close_action":"exit"}"#).close_action,
            CloseAction::Exit
        );
        assert_eq!(
            read(r#"{"close_action":"minimize"}"#).close_action,
            CloseAction::Minimize
        );
        assert_eq!(
            read(r#"{"close_action":"hide_to_tray"}"#).close_action,
            CloseAction::HideToTray
        );
    }

    #[test]
    fn an_old_settings_file_uses_automatic_efficiency_mode() {
        assert_eq!(read("{}").efficiency_mode, EfficiencyMode::Auto);
        assert_eq!(
            read(r#"{"efficiency_mode":"always"}"#).efficiency_mode,
            EfficiencyMode::Always
        );
        assert!(!EfficiencyMode::Off.active(false, true));
        assert!(EfficiencyMode::Auto.active(false, false));
        assert!(!EfficiencyMode::Auto.active(true, false));
        assert!(EfficiencyMode::Always.active(true, false));
    }
}

/// Interface language. `System` follows the operating system's language list.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    System,
    En,
    Pl,
}

pub(super) const APP_SETTINGS_FILE: &str = "settings.json";

/// Reads the app settings. A missing file means the default settings.
pub fn load_app_settings(root: &Data) -> AppSettings {
    json::read_json_or_default(&root.dir.join(APP_SETTINGS_FILE))
}

pub fn save_app_settings(root: &Data, settings: AppSettings) -> Result<(), AppError> {
    json::write_json(&root.dir.join(APP_SETTINGS_FILE), &settings)
}
