//! Steps hidden by a `<visible>` condition: the resolver must not require a choice from them or install their files.

use super::*;
use rustc_hash::{FxHashMap, FxHashSet};

const CONFIG_VISIBLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
    <moduleName>Test Mod</moduleName>
    <installSteps order="Explicit">
        <installStep name="choice">
            <optionalFileGroups order="Explicit">
                <group name="mode" type="SelectExactlyOne">
                    <plugins>
                        <plugin name="enabled">
                            <files><file source="a/on" destination="on"/></files>
                            <conditionFlags><flag name="mode">On</flag></conditionFlags>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="disabled">
                            <files><file source="a/off" destination="off"/></files>
                            <conditionFlags><flag name="mode">Off</flag></conditionFlags>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
        <installStep name="gated">
            <visible>
                <dependencies operator="And">
                    <flagDependency flag="mode" value="On"/>
                </dependencies>
            </visible>
            <optionalFileGroups order="Explicit">
                <group name="gated group" type="SelectExactlyOne">
                    <plugins>
                        <plugin name="gated pick">
                            <files><file source="a/gated" destination="gated"/></files>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

const CONFIG_FILE_DEPENDENCY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
    <moduleName>Test Mod</moduleName>
    <installSteps order="Explicit">
        <installStep name="gated">
            <visible>
                <dependencies operator="And">
                    <fileDependency file="base\core.dll" state="Active"/>
                </dependencies>
            </visible>
            <optionalFileGroups order="Explicit">
                <group name="gated" type="SelectAny"><plugins/></group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

fn visible_config() -> ModuleConfig {
    parse(CONFIG_VISIBLE).expect("the test XML is valid")
}

fn choice(step: u32, group: &str, option: &str) -> Choice {
    Choice {
        step,
        group_index: 0,
        group: group.to_string(),
        option: option.to_string(),
    }
}

#[test]
fn a_visible_condition_is_evaluated_from_the_flags_of_earlier_steps() {
    let config = visible_config();
    let visible = config.steps[1]
        .visible
        .as_ref()
        .expect("the step has a visible condition");
    let installed = FxHashSet::default();

    let on = FxHashMap::from_iter([("mode".to_string(), "On".to_string())]);
    assert!(step_visible(Some(visible), &on, &installed));
    let off = FxHashMap::from_iter([("mode".to_string(), "Off".to_string())]);
    assert!(!step_visible(Some(visible), &off, &installed));
    // Flag values are compared without regard to case, like in the other FOMOD checks.
    let lower = FxHashMap::from_iter([("mode".to_string(), "on".to_string())]);
    assert!(step_visible(Some(visible), &lower, &installed));
    assert!(step_visible(None, &off, &installed));
}

#[test]
fn a_hidden_step_needs_no_choice_and_installs_no_files() {
    let sources = resolve_sources(&visible_config(), &[choice(0, "mode", "disabled")])
        .expect("a hidden step does not require a choice");
    let sources: Vec<&str> = sources
        .iter()
        .map(|pattern| pattern.source.as_str())
        .collect();
    assert_eq!(sources, ["a/off"]);
}

#[test]
fn a_visible_step_still_requires_its_choice_and_installs_its_files() {
    let config = visible_config();
    let missing = resolve_sources(&config, &[choice(0, "mode", "enabled")]);
    assert!(matches!(
        missing,
        Err(FomodError::GroupRule {
            rule: "fomod.group_pick_exactly",
            ..
        })
    ));

    let sources = resolve_sources(
        &config,
        &[
            choice(0, "mode", "enabled"),
            choice(1, "gated group", "gated pick"),
        ],
    )
    .expect("the choices are valid");
    let sources: Vec<&str> = sources
        .iter()
        .map(|pattern| pattern.source.as_str())
        .collect();
    assert_eq!(sources, ["a/on", "a/gated"]);
}

#[test]
fn a_visible_file_dependency_matches_a_destination_selected_earlier() {
    let config = parse(CONFIG_FILE_DEPENDENCY).expect("the test XML is valid");
    let visible = config.steps[0]
        .visible
        .as_ref()
        .expect("the step has a visible condition");
    let flags = FxHashMap::default();

    let empty = FxHashSet::default();
    assert!(!step_visible(Some(visible), &flags, &empty));
    let installed = FxHashSet::from_iter(["base/core.dll".to_string()]);
    assert!(step_visible(Some(visible), &flags, &installed));
}
