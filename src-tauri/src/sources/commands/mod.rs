//! The source commands of the window: update checks, provider ids and pictures of mods, provider pages, keys, and the
//! download queue of deep links and direct downloads.

mod keys;
mod links;
mod manual;
mod pages;
mod requirements;
mod updates;

use crate::domain::source::SourceMeta;
use crate::providers;
use crate::sources::update;
use crate::status::views::UpdateReport;

// Tauri's `#[command]` and Specta's `#[specta]` put hidden macros beside each command. `collect_commands!`
// names the commands through this module, so the macros move with the single-name re-exports of the functions.
pub use keys::{
    __cmd__open_source_key_page, __cmd__save_source_key, __cmd__source_key_saved,
    __cmd__source_providers, __cmd__validate_source_key, __specta__fn__open_source_key_page,
    __specta__fn__save_source_key, __specta__fn__source_key_saved, __specta__fn__source_providers,
    __specta__fn__validate_source_key, __tauri_command_name_open_source_key_page,
    __tauri_command_name_save_source_key, __tauri_command_name_source_key_saved,
    __tauri_command_name_source_providers, __tauri_command_name_validate_source_key,
    open_source_key_page, save_source_key, source_key_saved, source_providers, validate_source_key,
};
pub use links::{
    __cmd__claim_download, __cmd__clear_downloads, __cmd__forget_collection_download,
    __cmd__list_downloads, __cmd__remove_download, __cmd__retry_download,
    __cmd__take_collection_link, __specta__fn__claim_download, __specta__fn__clear_downloads,
    __specta__fn__forget_collection_download, __specta__fn__list_downloads,
    __specta__fn__remove_download, __specta__fn__retry_download,
    __specta__fn__take_collection_link, __tauri_command_name_claim_download,
    __tauri_command_name_clear_downloads, __tauri_command_name_forget_collection_download,
    __tauri_command_name_list_downloads, __tauri_command_name_remove_download,
    __tauri_command_name_retry_download, __tauri_command_name_take_collection_link, claim_download,
    clear_downloads, forget_collection_download, list_downloads, remove_download, retry_download,
    take_collection_link, watch_links,
};
pub use manual::{
    __cmd__manual_update_cancel, __cmd__manual_update_download_done, __cmd__manual_update_finish,
    __cmd__manual_update_next, __cmd__manual_update_reopen, __cmd__manual_update_status,
    __cmd__manual_update_step, __cmd__start_manual_updates, __specta__fn__manual_update_cancel,
    __specta__fn__manual_update_download_done, __specta__fn__manual_update_finish,
    __specta__fn__manual_update_next, __specta__fn__manual_update_reopen,
    __specta__fn__manual_update_status, __specta__fn__manual_update_step,
    __specta__fn__start_manual_updates, __tauri_command_name_manual_update_cancel,
    __tauri_command_name_manual_update_download_done, __tauri_command_name_manual_update_finish,
    __tauri_command_name_manual_update_next, __tauri_command_name_manual_update_reopen,
    __tauri_command_name_manual_update_status, __tauri_command_name_manual_update_step,
    __tauri_command_name_start_manual_updates, manual_update_cancel, manual_update_download_done,
    manual_update_finish, manual_update_next, manual_update_reopen, manual_update_status,
    manual_update_step, start_manual_updates,
};
pub use pages::{
    __cmd__open_source_page, __cmd__open_update_page, __specta__fn__open_source_page,
    __specta__fn__open_update_page, __tauri_command_name_open_source_page,
    __tauri_command_name_open_update_page, open_source_page, open_update_page,
};
pub use requirements::{
    __cmd__cancel_requirement, __cmd__install_requirement, __cmd__install_requirements,
    __cmd__preview_requirements, __specta__fn__cancel_requirement,
    __specta__fn__install_requirement, __specta__fn__install_requirements,
    __specta__fn__preview_requirements, __tauri_command_name_cancel_requirement,
    __tauri_command_name_install_requirement, __tauri_command_name_install_requirements,
    __tauri_command_name_preview_requirements, cancel_requirement, install_requirement,
    install_requirements, preview_requirements,
};
pub use updates::{
    __cmd__cancel_update_all, __cmd__cancel_update_mod, __cmd__update_all, __cmd__update_mod,
    __specta__fn__cancel_update_all, __specta__fn__cancel_update_mod, __specta__fn__update_all,
    __specta__fn__update_mod, __tauri_command_name_cancel_update_all,
    __tauri_command_name_cancel_update_mod, __tauri_command_name_update_all,
    __tauri_command_name_update_mod, cancel_update_all, cancel_update_mod, update_all, update_mod,
};
crate::commands::game_commands! {
    /// Checks every source of the game for updates through its provider. Requires the provider's saved key when it
    /// needs one.
    query check_updates() -> Vec<UpdateReport> = super::update::check_updates;

    /// Checks every source of one mod for updates through its provider. Requires the provider's saved key when it needs
    /// one.
    query check_mod_update(id: String) -> UpdateReport = |root, game| super::update::check_mod_update(root, game, &id);

    /// Reads the page of each source that never had it read: name, author, category, description and picture. Without a
    /// provider key it does nothing. Answers how many mods got their page; the window then reads the status again.
    query fill_source_info() -> u32 = super::update::fill_source_info;

    /// Saves the source id the user typed for a mod. `None` removes that provider's entry.
    change set_mod_source(id: String, provider: String, mod_id: Option<String>) =
        |root, game| crate::library::set_mod_source(root, game, &id, &provider, mod_id);

    /// Hides the version the provider lists now, until the provider lists another one.
    change ignore_version(id: String, provider: String) =
        |root, game| super::update::ignore_version(root, game, &id, &provider);

    /// Shows the ignored version again.
    change unignore_version(id: String, provider: String) =
        |root, game| super::update::unignore_version(root, game, &id, &provider);

    /// A mod's saved page picture as a data URL. `None` when the file is gone.
    query source_picture(provider: String, file: String) -> Option<String> =
        |root, game| providers::require(&provider)?.picture(&root.game(game).images(), &file);

    /// Reads a mod's page from its provider and saves the common metadata. Requires the provider's saved key when it
    /// needs one.
    query refresh_source_meta(provider: String, mod_id: String) -> SourceMeta =
        |root, game| super::refresh::refresh_source_meta(root, game, &provider, &mod_id);

    /// Selects the installed version that matches the provider's newest file, after the user downloaded it by hand.
    change select_update_version(id: String, provider: String) =
        |root, game| super::update::select_installed_update(root, game, &id, &provider);
}
