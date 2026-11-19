//! The collection commands. The command body is thin; the feature module owns the opening, the profile edit and the
//! install loop.

mod install;
mod page;
mod prepare;
mod round;

use super::controller;
use super::model;
use super::model::{CollectionInstallRequest, CollectionPreview, CollectionSessionView};

// Tauri's `#[command]` and Specta's `#[specta]` put hidden macros beside each command. `collect_commands!`
// names the commands through this module, so the macros move with the single-name re-exports of the functions.
pub use install::{
    __cmd__cancel_collection, __cmd__install_collection, __specta__fn__cancel_collection,
    __specta__fn__install_collection, __tauri_command_name_cancel_collection,
    __tauri_command_name_install_collection, cancel_collection, install_collection,
};
pub use page::{
    __cmd__open_collection_page, __specta__fn__open_collection_page,
    __tauri_command_name_open_collection_page, open_collection_page,
};
pub use prepare::{
    __cmd__import_collection_file, __cmd__prepare_collection_item,
    __specta__fn__import_collection_file, __specta__fn__prepare_collection_item,
    __tauri_command_name_import_collection_file, __tauri_command_name_prepare_collection_item,
    import_collection_file, prepare_collection_item,
};
pub use round::{
    __cmd__collection_resolve_prompt, __cmd__collection_run_round, __cmd__collection_stop,
    __specta__fn__collection_resolve_prompt, __specta__fn__collection_run_round,
    __specta__fn__collection_stop, __tauri_command_name_collection_resolve_prompt,
    __tauri_command_name_collection_run_round, __tauri_command_name_collection_stop,
    collection_resolve_prompt, collection_run_round, collection_stop,
};

crate::commands::game_commands! {
    /// Opens a collection link and returns the plan and the active profile's statuses.
    query inspect_collection(link: String, revision: Option<u32>) -> CollectionPreview =
        |root, game| super::inspect::inspect(root, game, &link, revision);

    /// Creates or updates the profile of a collection and returns its live session.
    query start_collection(request: CollectionInstallRequest) -> CollectionSessionView =
        |root, game| super::session::start(root, game, &request);

    /// The live status of one collection in one profile.
    query collection_status(profile: String, collection: String) -> CollectionSessionView =
        |root, game| super::session::status(root, game, &profile, &collection);

    /// Reads the current revision of every collection in a profile and records the newest each provider reports.
    change check_collection_updates(profile: String) = |root, game| super::check::check(root, game, &profile);

    /// Settles the imported members, records the families the collection added and orders its block. One profile edit.
    query finish_collection(profile: String, collection: String) -> CollectionSessionView =
        |root, game| super::session::finish(root, game, &profile, &collection);

    /// Permanently skips one member. A temporary skip is the window's queue, not a profile change.
    query skip_collection_item(profile: String, collection: String, item: String, permanent: bool) -> CollectionSessionView =
        |root, game| super::session::skip(root, game, &profile, &collection, &item, permanent);

    /// Marks a manual or browse member as installed by hand.
    query mark_collection_item(profile: String, collection: String, item: String) -> CollectionSessionView =
        |root, game| super::session::mark_done(root, game, &profile, &collection, &item);

    /// Removes a collection from a profile. The library and the archives stay.
    query remove_collection(profile: String, collection: String) -> CollectionSessionView =
        |root, game| super::session::remove(root, game, &profile, &collection);
}
