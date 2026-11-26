use super::{AdoptPolicy, ArchiveLabel, Family, ModVersion};
use crate::domain::source::{ProviderId, SourceRef, SourceRequirement, SourceSync};
use crate::domain::version::IgnoredVersion;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn a_label_saved_with_a_name_still_loads_with_it() {
    let label: ArchiveLabel =
        serde_json::from_str(r#"{"name":"Example Pack","version":"2.0"}"#).unwrap();
    assert_eq!(
        label,
        ArchiveLabel {
            name: Some("Example Pack".into()),
            version: "2.0".into(),
        }
    );
}

/// An older `library.json` has no integrity field; it must still load and save without one.
#[test]
fn an_old_mod_version_loads_without_an_integrity_field() {
    let old = r#"{
        "folder":"m","family":"m","slot":"","files":[],"size_bytes":0,
        "info":{"name":"M","version":null,"author":null,"description":null},
        "archive":null,"installed_at":0
    }"#;
    let version: ModVersion = serde_json::from_str(old).unwrap();
    assert_eq!(version.integrity, None);
    assert!(
        !serde_json::to_string(&version)
            .unwrap()
            .contains("integrity")
    );
}

#[test]
fn a_label_without_a_name_is_saved_without_the_key() {
    let label = ArchiveLabel {
        name: None,
        version: "1.0.0".into(),
    };
    let text = serde_json::to_string(&label).unwrap();
    assert_eq!(text, r#"{"version":"1.0.0"}"#);
    let back: ArchiveLabel = serde_json::from_str(&text).unwrap();
    assert_eq!(back, label);
}

/// A non-default source family with every field each policy cares about, so the exact field outcomes below pin
/// the asymmetry instead of relying on an empty fixture.
fn source_family() -> Family {
    let provider = ProviderId::new("example-provider");
    Family {
        id: "example-family".into(),
        name: "Example Mod".into(),
        tags: BTreeSet::from(["example-tag".to_string()]),
        notes: "Source notes".into(),
        instructions: "Source instructions".into(),
        requires: vec!["source-dependency".into()],
        requirements: vec![SourceRequirement {
            source: SourceRef::new("example-provider", "10").with_file("20"),
            name: "Source requirement".into(),
            from_source: true,
            group: Some(2),
            ignored: false,
        }],
        incompatible: vec!["source-conflict".into()],
        overrides: vec!["source-override".into()],
        source_dlcs: BTreeMap::from([(provider.clone(), vec![vec!["Example DLC".into()]])]),
        source_sync: BTreeMap::from([(
            provider.clone(),
            SourceSync {
                file_id: "20".into(),
                version_id: "30".into(),
                checked_at: 100,
                mod_level: true,
                older_version: Some("1.0".into()),
            },
        )]),
        latest: BTreeMap::from([(provider.clone(), "2.0".into())]),
        latest_file_ids: BTreeMap::from([(provider.clone(), "40".into())]),
        ignored_versions: BTreeMap::from([(
            provider,
            IgnoredVersion {
                version: "1.5".into(),
                file_id: Some("15".into()),
            },
        )]),
        ..Family::default()
    }
}

#[test]
fn the_two_adopt_policies_cover_different_fields() {
    let source = source_family();
    let provider = ProviderId::new("example-provider");
    let remap = |id: &str| format!("remapped:{id}");

    // Hardcoded outcomes, not values read back from `source`: a shallow copy, a field copied from the wrong place
    // or a missed remap cannot pass either policy.
    let tags = BTreeSet::from(["example-tag".to_string()]);
    let requirements = vec![SourceRequirement {
        source: SourceRef::new("example-provider", "10").with_file("20"),
        name: "Source requirement".into(),
        from_source: true,
        group: Some(2),
        ignored: false,
    }];
    let source_dlcs = BTreeMap::from([(provider.clone(), vec![vec!["Example DLC".into()]])]);
    let source_sync = BTreeMap::from([(
        provider.clone(),
        SourceSync {
            file_id: "20".into(),
            version_id: "30".into(),
            checked_at: 100,
            mod_level: true,
            older_version: Some("1.0".into()),
        },
    )]);
    let latest = BTreeMap::from([(provider.clone(), "2.0".into())]);
    let latest_file_ids = BTreeMap::from([(provider.clone(), "40".into())]);
    let ignored_versions = BTreeMap::from([(
        provider,
        IgnoredVersion {
            version: "1.5".into(),
            file_id: Some("15".into()),
        },
    )]);

    let mut merged = Family::default();
    super::adopt_fields(&mut merged, &source, AdoptPolicy::Merge, remap);
    assert_eq!(merged.tags, tags);
    assert_eq!(merged.notes, "Source notes");
    assert_eq!(merged.instructions, "Source instructions");
    assert_eq!(merged.requires, ["remapped:source-dependency"]);
    assert_eq!(merged.requirements, requirements);
    assert_eq!(merged.incompatible, ["remapped:source-conflict"]);
    assert!(merged.overrides.is_empty(), "Merge never takes overrides");
    assert_eq!(merged.source_dlcs, source_dlcs);
    assert_eq!(merged.source_sync, source_sync);
    assert_eq!(merged.latest, latest);
    assert_eq!(merged.latest_file_ids, latest_file_ids);
    assert_eq!(merged.ignored_versions, ignored_versions);

    let mut filled = Family::default();
    super::adopt_fields(&mut filled, &source, AdoptPolicy::FillEmpty, remap);
    assert_eq!(filled.tags, tags);
    assert_eq!(filled.notes, "Source notes");
    assert_eq!(filled.instructions, "Source instructions");
    assert_eq!(filled.requires, ["remapped:source-dependency"]);
    assert_eq!(filled.requirements, requirements);
    assert_eq!(filled.incompatible, ["remapped:source-conflict"]);
    assert_eq!(filled.overrides, ["remapped:source-override"]);
    assert_eq!(filled.source_dlcs, source_dlcs);
    assert!(
        filled.source_sync.is_empty()
            && filled.latest.is_empty()
            && filled.latest_file_ids.is_empty()
            && filled.ignored_versions.is_empty(),
        "FillEmpty leaves all four update-metadata fields alone"
    );
}
