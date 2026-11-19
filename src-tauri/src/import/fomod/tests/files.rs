//! Which files an installed choice brings in: the conditions on them, and how their sources match.

use super::*;

#[test]
fn conditional_files_follow_flags_and_installed_files() {
    // The walk records a pattern's destination, so `fonts/*` records "fonts", not the "fonts/base.nif" file it
    // matches. Point the conditional dependency at that recorded destination so a real selection can satisfy it.
    // The shared CONFIG is left alone: other tests rely on its conditional staying unsatisfied.
    let xml = CONFIG.replace("file=\"fonts/base.nif\"", "file=\"fonts\"");
    assert_ne!(
        xml, CONFIG,
        "the fixture must name the conditional file dependency"
    );
    let config = parse(&xml).expect("valid XML");

    let has_conditional = |option: &str| {
        resolve_sources(
            &config,
            &[Choice {
                step: 0,
                group_index: 0,
                group: "textures".into(),
                option: option.into(),
            }],
        )
        .expect("valid choices")
        .iter()
        .any(|pattern| pattern.source == "patch/hd")
    };

    // option-a sets quality=low and the required font pattern records "fonts": both dependencies hold.
    assert!(has_conditional("option-a"));
    // option-b sets no quality flag, so the same installed file is not enough.
    assert!(!has_conditional("option-b"));
}

#[test]
fn sources_match_files_folders_and_wildcards() {
    assert!(matches_source("textures/2k", "textures/2k/a.nif"));
    assert!(matches_source("textures/2k/", "textures/2k/a.nif"));
    assert!(matches_source("textures/2k", "TEXTURES\\2K\\a.nif"));
    assert!(matches_source("fonts/*", "fonts/base.nif"));
    assert!(matches_source("*.nif", "fonts/base.nif"));
    assert!(!matches_source("textures/2k", "textures/4k/a.nif"));
    assert!(!matches_source("fonts/*", "textures/2k/a.nif"));
    assert!(!matches_source("", "anything"));
}
