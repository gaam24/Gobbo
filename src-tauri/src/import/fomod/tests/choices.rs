//! Choosing options: what is in by default, the groups that want exactly one, and options that do not exist.

use super::*;

#[test]
fn required_option_goes_in_without_a_choice() {
    let sources = resolve_sources(
        &config(),
        &[Choice {
            step: 0,
            group_index: 0,
            group: "textures".into(),
            option: "option-a".into(),
        }],
    )
    .expect("valid choices");
    let sources: Vec<String> = sources.into_iter().map(|p| p.source).collect();
    assert_eq!(sources, ["textures/low", "fonts/*"]);
}

#[test]
fn select_exactly_one_refuses_two_and_none() {
    let two = resolve_sources(
        &config(),
        &[
            Choice {
                step: 0,
                group_index: 0,
                group: "textures".into(),
                option: "option-a".into(),
            },
            Choice {
                step: 0,
                group_index: 0,
                group: "textures".into(),
                option: "option-b".into(),
            },
        ],
    );
    assert!(matches!(
        two,
        Err(FomodError::GroupRule {
            rule: "fomod.group_pick_only",
            ..
        })
    ));

    let none = resolve_sources(&config(), &[]);
    assert!(matches!(
        none,
        Err(FomodError::GroupRule {
            rule: "fomod.group_pick_exactly",
            ..
        })
    ));
}

#[test]
fn unknown_option_is_refused() {
    let result = resolve_sources(
        &config(),
        &[Choice {
            step: 0,
            group_index: 0,
            group: "textures".into(),
            option: "option-missing".into(),
        }],
    );
    assert!(
        matches!(result, Err(FomodError::UnknownOption { option, .. }) if option == "option-missing")
    );
}

const DUPLICATE_GROUPS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <config xmlns="http://qconsulting.ca/fo3/ModConfig5.0">
        <moduleName>Two same names</moduleName>
        <installSteps>
            <installStep name="visuals">
                <optionalFileGroups>
                    <group name="textures" type="SelectAny">
                        <plugins>
                            <plugin name="option-a">
                                <files><folder source="textures/low" destination="textures"/></files>
                            </plugin>
                        </plugins>
                    </group>
                    <group name="textures" type="SelectAny">
                        <plugins>
                            <plugin name="option-b">
                                <files><folder source="textures/high" destination="textures"/></files>
                            </plugin>
                        </plugins>
                    </group>
                </optionalFileGroups>
            </installStep>
        </installSteps>
    </config>"#;

fn duplicate_groups() -> ModuleConfig {
    parse(DUPLICATE_GROUPS).expect("valid XML")
}

fn sources_of(config: &ModuleConfig, choices: &[Choice]) -> Vec<String> {
    resolve_sources(config, choices)
        .expect("valid choices")
        .into_iter()
        .map(|pattern| pattern.source)
        .collect()
}

#[test]
fn choices_go_to_the_group_at_their_index_not_to_every_group_with_that_name() {
    let config = duplicate_groups();
    assert_eq!(
        sources_of(
            &config,
            &[Choice {
                step: 0,
                group_index: 0,
                group: "textures".into(),
                option: "option-a".into(),
            }],
        ),
        ["textures/low"]
    );
    assert_eq!(
        sources_of(
            &config,
            &[Choice {
                step: 0,
                group_index: 1,
                group: "textures".into(),
                option: "option-b".into(),
            }],
        ),
        ["textures/high"]
    );
}

#[test]
fn an_option_of_one_group_is_not_accepted_by_the_other_group_with_the_same_name() {
    let result = resolve_sources(
        &duplicate_groups(),
        &[Choice {
            step: 0,
            group_index: 1,
            group: "textures".into(),
            option: "option-a".into(),
        }],
    );
    assert!(
        matches!(result, Err(FomodError::UnknownOption { option, .. }) if option == "option-a")
    );
}

#[test]
fn a_choice_with_an_index_outside_the_installer_is_refused() {
    for choice in [
        Choice {
            step: 9,
            group_index: 0,
            group: "textures".into(),
            option: "option-a".into(),
        },
        Choice {
            step: 0,
            group_index: 9,
            group: "textures".into(),
            option: "option-a".into(),
        },
    ] {
        assert!(matches!(
            resolve_sources(&config(), &[choice]),
            Err(FomodError::UnknownOption { .. })
        ));
    }
}

#[test]
fn an_old_choice_without_indices_falls_back_to_a_unique_group_name() {
    // A record from before the indices keeps step 0 and group 0, while the group it names sits later in the step.
    let sources = resolve_sources(
        &config(),
        &[
            Choice {
                step: 0,
                group_index: 0,
                group: "textures".into(),
                option: "option-a".into(),
            },
            Choice {
                step: 0,
                group_index: 0,
                group: "patch".into(),
                option: "font-patch".into(),
            },
        ],
    )
    .expect("choice by group name");
    let sources: Vec<String> = sources.into_iter().map(|pattern| pattern.source).collect();
    assert_eq!(sources, ["textures/low", "fonts/*"]);
}
