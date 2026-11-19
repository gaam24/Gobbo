//! Families: the inputs of the planner, and how a family gets its id.

use super::*;
use crate::context::library::INDEX_FILE;
use crate::domain::source::SourceRef;

#[test]
fn inputs_carry_files_only_for_enabled_versions() {
    let base = scratch("inputs");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    write(&library_dir.join("modB/content/x.ws"), "b");
    let library = Library::load(&base.join(INDEX_FILE), &library_dir, &game()).unwrap();

    let mods = [listed("modA", true), listed("modB", false)];
    let inputs = library.inputs(&mods);
    assert_eq!(
        inputs[0]
            .files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["content/x.ws"]
    );
    assert_eq!(inputs[0].id.as_ref(), library.versions["modA"].family);
    assert!(
        inputs[0].layout.is_some(),
        "a loaded library has its layout profiles"
    );
    assert!(inputs[1].files.is_empty());
    assert!(!inputs[1].enabled);
}

#[test]
fn family_ids_prefer_the_nexus_id_and_fall_back_to_the_name() {
    assert_eq!(
        family_id_from(Some(&SourceRef::new("nexus", "35686")), "Remove Mods Menu"),
        "nexus:35686"
    );
    assert_eq!(
        family_id_from(
            Some(&SourceRef::new("nexus", "225").with_part("100")),
            "Addon"
        ),
        "nexus:225#100"
    );
    assert_eq!(
        family_id_from(None, "Remove Mods Menu!"),
        "local:remove-mods-menu"
    );
    assert_eq!(family_id_from(None, "!!!"), "local:mod");
}

#[test]
fn a_manual_import_without_a_part_joins_a_family_only_when_the_names_match() {
    let mut library = Library::default();
    let mut family = crate::library::Family {
        id: "nexus:225".into(),
        name: "Addon A".into(),
        ..crate::library::Family::default()
    };
    family.sources.insert(
        crate::domain::source::ProviderId::new("nexus"),
        crate::domain::source::SourceMod {
            mod_id: "225".into(),
            part_id: None,
            part_name: None,
            part_category: None,
            meta: None,
        },
    );
    library.families.insert("nexus:225".into(), family);
    library.versions.insert(
        "addon-a".into(),
        ModVersion {
            folder: "addon-a".into(),
            family: "nexus:225".into(),
            slot: String::new(),
            files: Vec::new().into(),
            size_bytes: 0,
            info: ModInfo {
                name: "Addon A".into(),
                ..ModInfo::default()
            },
            edit: ModInfoEdit::default(),
            archive: None,
            parsed_sources: std::collections::BTreeMap::from([(
                crate::domain::source::ProviderId::new("nexus"),
                "225".to_string(),
            )]),
            parsed_parts: std::collections::BTreeMap::new(),
            installed_at: 0,
            fomod: Vec::new(),
            fomod_installer: false,
            integrity: None,
        },
    );
    let source = SourceRef::new("nexus", "225");
    assert_eq!(library.family_for_source(&source, "Addon A"), "nexus:225");
    assert_eq!(
        library.family_for_source(&source, "Addon B"),
        "nexus:225#unresolved:addon-b"
    );
}

#[test]
fn an_import_keeps_one_family_for_a_part_and_separates_another_part() {
    let mut library = Library::default();
    let mut first = crate::library::Family {
        id: "nexus:225".into(),
        name: "First addon".into(),
        ..crate::library::Family::default()
    };
    first.sources.insert(
        crate::domain::source::ProviderId::new("nexus"),
        crate::domain::source::SourceMod {
            mod_id: "225".into(),
            part_id: Some("100".into()),
            part_name: Some("First addon".into()),
            part_category: Some("main".into()),
            meta: None,
        },
    );
    library.families.insert("nexus:225".into(), first);

    let same = SourceRef::new("nexus", "225").with_part("100");
    assert_eq!(library.family_for_source(&same, "First addon"), "nexus:225");

    let other = SourceRef::new("nexus", "225").with_part("200");
    assert_eq!(
        library.family_for_source(&other, "Second addon"),
        "nexus:225#200"
    );

    let unknown = SourceRef::new("nexus", "225");
    assert_eq!(
        library.family_for_source(&unknown, "Unknown addon"),
        "nexus:225#unresolved:unknown-addon"
    );
}
