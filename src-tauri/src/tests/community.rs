//! Importing an r2modman/Gale profile: matching Thunderstore packages, unknown ones listed, and a normal profile.

use super::*;
use crate::domain::source::SourceRef;
use crate::profiles::community::{CommunityMatch, import, inspect};
use crate::test_support::import_from;
use crate::test_support::write;

const PROFILE: &str = "- name: Owner-One\n  version:\n    major: 1\n    minor: 2\n    patch: 3\n  enabled: true\n\
                        - name: Owner-Missing\n  version:\n    major: 2\n    minor: 0\n    patch: 0\n  enabled: false\n";

#[test]
fn a_community_profile_matches_the_library_and_creates_a_normal_profile() {
    let f = fixture("community-import");
    let zip = f.zip(
        "Owner-One-1.2.3.zip",
        &[
            (
                "Mods/modOne/info.json",
                r#"{"modName":"One","version":"1.2.3"}"#,
            ),
            ("Mods/modOne/content/x.ws", "x"),
        ],
    );
    import_from(
        &f.data,
        SLOT_GAME,
        &zip,
        None,
        Some(SourceRef::new("thunderstore", "Owner-One")),
        &mut |_, _| {},
    )
    .unwrap();

    let dir = f.base.join("My Profile");
    write(&dir.join("mods.yml"), PROFILE);
    let preview = inspect(&f.data, SLOT_GAME, &dir.display().to_string()).unwrap();
    assert_eq!(preview.profile, "My Profile");
    assert_eq!((preview.missing, preview.other_versions), (1, 0));
    assert_eq!(preview.mods[0].state, CommunityMatch::Matched);
    assert_eq!(preview.mods[0].selected_version.as_deref(), Some("1.2.3"));
    assert_eq!(preview.mods[1].state, CommunityMatch::Missing);
    assert!(!preview.has_configs);

    // Another version in the file is reported, not silently selected.
    write(
        &dir.join("mods.yml"),
        "- name: Owner-One\n  version:\n    major: 9\n    minor: 9\n    patch: 9\n  enabled: true\n",
    );
    let changed = inspect(&f.data, SLOT_GAME, &dir.display().to_string()).unwrap();
    assert_eq!(changed.mods[0].state, CommunityMatch::OtherVersion);
    assert_eq!(changed.mods[0].selected_version.as_deref(), Some("1.2.3"));

    // The slug is an id: a profile that spells it in another case still finds the package.
    write(
        &dir.join("mods.yml"),
        "- name: owner-one\n  version:\n    major: 1\n    minor: 2\n    patch: 3\n  enabled: true\n",
    );
    let folded = inspect(&f.data, SLOT_GAME, &dir.display().to_string()).unwrap();
    assert_eq!(folded.mods[0].state, CommunityMatch::Matched);
    assert_eq!(folded.mods[0].selected_version.as_deref(), Some("1.2.3"));

    write(&dir.join("mods.yml"), PROFILE);
    let imported = import(&f.data, SLOT_GAME, &dir.display().to_string(), "Imported").unwrap();
    assert_eq!(imported.mods, 1);
    assert_eq!(imported.missing, ["Owner-Missing"]);
    assert!(imported.other_versions.is_empty());
    let active = imported
        .status
        .profiles
        .iter()
        .find(|profile| profile.active)
        .unwrap();
    assert_eq!(active.name, "Imported");
    let one = imported
        .status
        .mods
        .iter()
        .find(|item| item.folder == "Owner-One-1.2.3")
        .unwrap();
    assert!(one.enabled);

    // A folder imported twice gets a free name, like any other recipe's new profile.
    let again = import(&f.data, SLOT_GAME, &dir.display().to_string(), "Imported").unwrap();
    let names: Vec<&str> = again
        .status
        .profiles
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert!(
        names.contains(&"Imported"),
        "the first import keeps its name"
    );
    assert!(
        names.contains(&"Imported (2)"),
        "the second import gets a free name"
    );
}
