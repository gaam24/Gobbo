//! Dispatch of the mod load order to the writer for the game's format. One format owns one writer: the settings file
//! for `mods.settings`, the list in the mod layer for UE4SS's `mods.txt`. A game with no load order writes nothing.

use std::path::Path;

use crate::deploy::Target;
use crate::deploy::apply::write_settings;
use crate::deploy::error::DeployError;
use crate::deploy::journal::Journal;
use crate::deploy::mods_txt::write_order;
use crate::domain::resolver::DeployPlan;
use crate::games::definition::OrderFormat;

/// Writes the load order with the writer for `target.order_format`.
pub(super) fn write(
    journal: &Journal,
    target: &Target,
    plan: &DeployPlan,
    library: &Path,
    layer: &Path,
) -> Result<(), DeployError> {
    match target.order_format {
        Some(OrderFormat::ModsSettings) => write_settings(journal, target, &plan.settings)?,
        Some(OrderFormat::Ue4ssModsTxt) => write_order(plan, library, target, layer)?,
        None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::data::JOURNAL_FILE;
    use crate::domain::resolver::SettingsRow;
    use crate::test_support::{scratch, write as write_file};
    use std::fs;

    fn plan() -> DeployPlan {
        DeployPlan {
            settings: vec![SettingsRow {
                folder: "modA".into(),
                enabled: true,
                priority: 1,
            }],
            ..DeployPlan::default()
        }
    }

    #[test]
    fn the_settings_format_writes_the_settings_file_but_no_layer_list() {
        let base = scratch!("order-dispatch-settings");
        fs::create_dir_all(base.join("settings")).unwrap();
        let mut target = crate::test_support::target(&base);
        target.order_format = Some(OrderFormat::ModsSettings);
        target.settings = Some(base.join("settings/mods.settings"));
        target.order_file = Some("Mods/mods.txt".into());
        // `write_settings` journals the sections it changes before writing them, so the journal's folder must exist.
        fs::create_dir_all(&target.state).unwrap();
        let journal = Journal::new(target.state.join(JOURNAL_FILE));

        write(
            &journal,
            &target,
            &plan(),
            &base.join("library"),
            &base.join("mods"),
        )
        .unwrap();

        assert!(
            fs::read_to_string(target.settings.as_deref().unwrap())
                .unwrap()
                .contains("[modA]")
        );
        assert!(!base.join("mods/Mods/mods.txt").exists());
    }

    #[test]
    fn the_mods_txt_format_writes_the_layer_list_but_no_settings_file() {
        let base = scratch!("order-dispatch-mods-txt");
        let mut target = crate::test_support::target(&base);
        target.order_format = Some(OrderFormat::Ue4ssModsTxt);
        target.settings = Some(base.join("settings/mods.settings"));
        write_file(target.settings.as_ref().unwrap(), "[modOld]\nEnabled=1\n");
        target.order_file = Some("Mods/mods.txt".into());
        let journal = Journal::new(target.state.join(JOURNAL_FILE));

        write(
            &journal,
            &target,
            &plan(),
            &base.join("library"),
            &base.join("mods"),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(target.settings.as_deref().unwrap()).unwrap(),
            "[modOld]\nEnabled=1\n"
        );
        assert_eq!(
            fs::read_to_string(base.join("mods/Mods/mods.txt")).unwrap(),
            "modA : 1\r\n"
        );
    }

    #[test]
    fn no_load_order_writes_nothing() {
        let base = scratch!("order-dispatch-none");
        let mut target = crate::test_support::target(&base);
        target.order_format = None;
        target.settings = Some(base.join("settings/mods.settings"));
        write_file(target.settings.as_ref().unwrap(), "[modOld]\nEnabled=1\n");
        target.order_file = Some("Mods/mods.txt".into());
        let journal = Journal::new(target.state.join(JOURNAL_FILE));

        write(
            &journal,
            &target,
            &plan(),
            &base.join("library"),
            &base.join("mods"),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(target.settings.as_deref().unwrap()).unwrap(),
            "[modOld]\nEnabled=1\n"
        );
        assert!(!base.join("mods/Mods/mods.txt").exists());
    }
}
