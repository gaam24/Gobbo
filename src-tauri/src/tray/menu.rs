//! Turns the menu description into a native Tauri menu. The module owns the item objects until the menu is created.

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

use super::view::{Rendered, SpecItem};

fn make(app: &AppHandle, item: &SpecItem) -> tauri::Result<Box<dyn IsMenuItem<Wry>>> {
    match item {
        SpecItem::Action { id, label, enabled } => Ok(Box::new(MenuItem::with_id(
            app,
            id.as_str(),
            label,
            *enabled,
            None::<&str>,
        )?)),
        SpecItem::Check { id, label, checked } => Ok(Box::new(CheckMenuItem::with_id(
            app,
            id.as_str(),
            label,
            true,
            *checked,
            None::<&str>,
        )?)),
        SpecItem::Separator => Ok(Box::new(PredefinedMenuItem::separator(app)?)),
        SpecItem::Submenu {
            id,
            label,
            enabled,
            items,
        } => {
            let mut owned: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::with_capacity(items.len());

            for child in items {
                owned.push(make(app, child)?);
            }

            let refs: Vec<&dyn IsMenuItem<Wry>> =
                owned.iter().map(std::convert::AsRef::as_ref).collect();
            Ok(Box::new(Submenu::with_id_and_items(
                app,
                id.as_str(),
                label,
                *enabled,
                &refs,
            )?))
        }
    }
}

/// Creates a fresh menu for one render. The previous menu is replaced by the caller.
pub fn build(app: &AppHandle, rendered: &Rendered) -> tauri::Result<Menu<Wry>> {
    let mut owned: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::with_capacity(rendered.items.len());

    for item in &rendered.items {
        owned.push(make(app, item)?);
    }

    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(std::convert::AsRef::as_ref).collect();
    Menu::with_items(app, &refs)
}
