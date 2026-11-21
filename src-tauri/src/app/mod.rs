//! Starting the app: the storage question, the plugins, the window and the setup that needs the settings. The commands
//! the window calls are in `commands`; the command-line mode is in `cli`.

#[cfg(any(debug_assertions, test))]
mod bindings;
mod cli;
mod storage_prompt;

pub use cli::run_cli;

/// The path of the generated frontend bindings, checked against the commands in a debug build.
#[cfg(any(debug_assertions, test))]
pub const BINDINGS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

use std::path::{Path, PathBuf};

use tauri::utils::config::AppDirectoriesOverride;
use tauri::{Manager, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::commands;
use crate::context::Data;
use crate::games;

/// Starts the Tauri window: resolves storage, sets up the plugins and state, and runs the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(
    clippy::large_stack_frames,
    reason = "tauri::generate_context! embeds the app's context in this frame"
)]
pub fn run() {
    // A copy that a portable update started waits for the copy that is leaving before it takes the single-instance
    // lock or the data folder.
    crate::update::relaunch::wait_for_parent();
    // The first-start question runs before Tauri: the data folder, the window state and the webview profile are
    // fixed when the app is built. A known choice never shows a dialog.
    let storage = match storage_prompt::resolve_or_prompt() {
        Ok(storage_prompt::Start::Run(storage)) => storage,
        Ok(storage_prompt::Start::Quit) => return,
        Err(error) => {
            storage_prompt::report_fatal(&error);
            std::process::exit(1);
        }
    };
    crate::infra::log::init(&storage.root);
    crate::infra::log::install_panic_hook();
    crate::games::set_user_definition_dirs(storage.root.clone());
    crate::games::set_catalog_definition_dirs(storage.root.clone());
    crate::infra::log::info(format!(
        "Gobbo {} starting in {}",
        env!("CARGO_PKG_VERSION"),
        storage.root.display()
    ));
    let builder = commands::builder();
    #[cfg(debug_assertions)]
    bindings::update(&builder);

    let app = tauri::Builder::default();
    // A second start (a link clicked in the browser) hands its arguments to the running app and exits. It goes first,
    // so the second app never gets as far as a window. The E2E build skips it: the suite must not hand off to a Gobbo
    // the user has open, and it starts its own app with a separate data folder.
    // The `deep-link` feature of this plugin is what forwards `nxm://` / `ror2mm://` from that second start to
    // `watch_links`. Without it the window is only focused and the download is silently lost; no test catches this.
    #[cfg(all(any(target_os = "linux", windows), not(feature = "e2e")))]
    let app = app.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    // A debug build with the `mcp-bridge` feature lets the coding assistant see and drive this window. The bridge
    // listens on localhost only, and the `debug_assertions` gate keeps it out of a release build.
    #[cfg(all(debug_assertions, feature = "mcp-bridge"))]
    let app = app.plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .bind_address("127.0.0.1")
            .build(),
    );
    // The E2E feature embeds the WebDriver server in the app, so the suite needs no external tauri-driver.
    #[cfg(all(debug_assertions, feature = "e2e"))]
    let app = app.plugin(tauri_plugin_wdio_webdriver::init());
    // The plugin saves the main window's size and place as they change. It does not restore them by itself: `setup`
    // does, when the app settings allow it, so the setting is read first.
    let app = app
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .skip_initial_state("main")
                .build(),
        )
        .on_window_event(|window, event| {
            // The tray owns the app's lifetime while background work continues. E2E builds keep the ordinary close so
            // WebdriverIO can end a session. Everywhere else Rust blocks the close and the tray decides: the saved choice, or
            // the closing dialog.
            if let WindowEvent::CloseRequested { api, .. } = event
                && !cfg!(feature = "e2e")
                && let Some(data) = window.app_handle().try_state::<Data>()
            {
                api.prevent_close();
                crate::tray::close::request(window.app_handle(), &data);
            }
            // Efficiency mode follows focus and minimize. The settings file is small and this runs on window events,
            // not on every status, so it is read again rather than kept in a second global.
            if matches!(event, WindowEvent::Focused(_) | WindowEvent::Resized(_))
                && let Some(data) = window.app_handle().try_state::<Data>()
            {
                let settings = crate::settings::config::load_app_settings(data.inner());
                crate::settings::config::apply_efficiency(
                    settings.efficiency_mode,
                    window.is_focused().unwrap_or(true),
                    window.is_minimized().unwrap_or(false),
                );
            }
        })
        .setup(|app| {
            if let Ok(exe_dir) = crate::infra::storage::exe_dir() {
                crate::update::swap::cleanup_leftovers(&exe_dir);
            }

            if let Ok(resources) = app.path().resource_dir()
                && let Some(games) = bundled_resource(&resources, "games")
            {
                let loaders = bundled_resource(&resources, "loaders")
                    .unwrap_or_else(|| resources.join("loaders"));
                games::set_definition_dirs(games, loaders);
            }

            games::drop_stale_catalog();

            let dir = app.path().app_data_dir()?;
            let data = match crate::context::open(dir.clone(), &crate::providers::archive_name_from)
            {
                Ok(data) => data,
                Err(error) => {
                    crate::infra::log::error(format!(
                        "could not migrate the data folders: {error}"
                    ));
                    let data = Data::with_cache(dir);
                    data.clean_stale_temps();
                    data
                }
            };
            app.manage(data);
            // The registry owns the handle so every task publishes its own lifecycle events, no matter which game
            // the window is showing.
            app.manage(crate::infra::tasks::TaskFlags::new(app.handle().clone()));
            app.manage(crate::packs::commands::PackLinkPending::default());
            app.manage(crate::sources::update::manual::ManualUpdateSessions::default());
            let settings = crate::settings::config::load_app_settings(&app.state::<Data>());

            if settings.remember_window
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.restore_state(StateFlags::all());
            }

            if let Some(window) = app.get_webview_window("main") {
                crate::settings::config::apply_efficiency(
                    settings.efficiency_mode,
                    window.is_focused().unwrap_or(true),
                    window.is_minimized().unwrap_or(false),
                );
            }

            if let Err(error) = crate::tray::init(app.handle()) {
                crate::infra::log::warn(format!("could not start the system tray: {error}"));
            }

            commands::watch_links(app);
            Ok(())
        })
        .invoke_handler(builder.invoke_handler());

    // Portable and custom mode put every Tauri app directory in one folder: `app_data_dir`, the window state file and
    // the default webview profile all follow it. Global mode keeps today's layout and needs no override.
    let mut context = tauri::generate_context!();

    if let Some(root) = storage.tauri_root() {
        context.config_mut().app.app_directories_override =
            Some(AppDirectoriesOverride::Root(root));
    }

    if let Err(error) = app.run(context) {
        crate::infra::log::error(format!("could not run the window: {error}"));
        std::process::exit(1);
    }
}

/// The folder of a directory resource. Tauri's bundler may copy it as `<name>/` or as its contents, depending on the
/// platform, so both layouts are accepted.
fn bundled_resource(resources: &Path, name: &str) -> Option<PathBuf> {
    let bundled = resources.join(name);

    if bundled.join(name).is_dir() {
        Some(bundled.join(name))
    } else if bundled.is_dir() {
        Some(bundled)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    /// The command names that `commands::builder` registers, as `build.rs` read them from `commands.txt`.
    const COMMANDS: &str = include_str!(concat!(env!("OUT_DIR"), "/app-commands.txt"));

    /// The global data folder is built from this name; it must be the identifier Tauri uses for `app_data_dir`.
    #[test]
    fn the_storage_app_id_matches_the_tauri_identifier() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], crate::infra::storage::APP_ID);
    }

    /// The installer and the portable archive ship both definition folders. A game with a loader name cannot start
    /// without the second one.
    #[test]
    fn the_bundled_resources_include_games_and_loaders() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let resources = config["bundle"]["resources"].as_object().unwrap();
        assert_eq!(
            resources.get("../games").and_then(|value| value.as_str()),
            Some("games")
        );
        assert_eq!(
            resources.get("../loaders").and_then(|value| value.as_str()),
            Some("loaders")
        );
    }

    /// Each command has a permission in the main window's capability. Without one, the window cannot call it, so a
    /// command added to the builder must be added to `capabilities/default.json` too.
    #[test]
    fn every_command_has_a_permission_in_the_main_capability() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|permission| permission.as_str())
            .collect();
        for name in COMMANDS.lines() {
            let permission = format!("allow-{}", name.replace('_', "-"));
            assert!(
                permissions.contains(&permission.as_str()),
                "{permission} is missing from capabilities/default.json"
            );
        }
    }

    /// The deep-link plugin reads only the schemes in this config when a link starts the app. Every provider that adds
    /// a scheme must add it here too, or a cold start with that link is ignored.
    #[test]
    fn every_provider_scheme_and_the_pack_scheme_are_configured_deep_links() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let configured: Vec<&str> = config["plugins"]["deep-link"]["desktop"]["schemes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|scheme| scheme.as_str())
            .collect();
        let expected: Vec<&str> = crate::providers::providers()
            .iter()
            .flat_map(|provider| provider.schemes().iter().copied())
            .collect();
        for scheme in expected {
            assert!(
                configured.contains(&scheme),
                "the provider scheme {scheme} is not configured"
            );
        }

        assert!(
            configured.contains(&"gobbo"),
            "the pack scheme is not configured"
        );
    }
}
