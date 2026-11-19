//! The wizard's state: the steps shown, the group rules, the option kinds and the defaults, all read from the same walk
//! the import uses. The fixture is a small generic installer.

use super::*;
use crate::import::fomod::installer_state;
use crate::import::fomod::model::GroupProblem;
use crate::import::fomod::state::{selection_from_choices, toggle_pick};

const INSTALLER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
    <moduleName>Generic Installer</moduleName>
    <requiredInstallFiles>
        <file source="base/core.txt" destination="core.txt"/>
    </requiredInstallFiles>
    <installSteps order="Explicit">
        <installStep name="first">
            <optionalFileGroups order="Explicit">
                <group name="mode" type="SelectExactlyOne">
                    <plugins>
                        <plugin name="on">
                            <files><file source="a/on.txt" destination="on.txt"/></files>
                            <conditionFlags><flag name="mode">On</flag></conditionFlags>
                            <typeDescriptor><type name="Recommended"/></typeDescriptor>
                        </plugin>
                        <plugin name="off">
                            <files><file source="a/off.txt" destination="off.txt"/></files>
                            <conditionFlags><flag name="mode">Off</flag></conditionFlags>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
                <group name="extras" type="SelectAtLeastOne">
                    <plugins>
                        <plugin name="extra">
                            <files><file source="b/extra.txt" destination="extra.txt"/></files>
                            <typeDescriptor><type name="Recommended"/></typeDescriptor>
                        </plugin>
                        <plugin name="more">
                            <files><file source="b/more.txt" destination="more.txt"/></files>
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
                <group name="pick" type="SelectAtMostOne">
                    <plugins>
                        <plugin name="one">
                            <files><file source="c/one.txt" destination="one.txt"/></files>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="two">
                            <files><file source="c/two.txt" destination="two.txt"/></files>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
        <installStep name="tiers">
            <optionalFileGroups order="Explicit">
                <group name="tiers" type="SelectAny">
                    <plugins>
                        <plugin name="trigger">
                            <files><file source="d/trigger.txt" destination="trigger.txt"/></files>
                            <conditionFlags><flag name="tier">high</flag></conditionFlags>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="tiered">
                            <files><file source="d/tiered.txt" destination="tiered.txt"/></files>
                            <typeDescriptor>
                                <dependencyType>
                                    <defaultType name="Optional"/>
                                    <patterns>
                                        <pattern>
                                            <dependencies operator="And">
                                                <flagDependency flag="tier" value="high"/>
                                            </dependencies>
                                            <type name="Required"/>
                                        </pattern>
                                    </patterns>
                                </dependencyType>
                            </typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
        <installStep name="bundle">
            <optionalFileGroups order="Explicit">
                <group name="bundle" type="SelectAll">
                    <plugins>
                        <plugin name="all-a">
                            <files><file source="e/a.txt" destination="a.txt"/></files>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="all-b">
                            <files><file source="e/b.txt" destination="b.txt"/></files>
                            <typeDescriptor><type name="NotUsable"/></typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

/// A generic installer whose first option sets a flag that makes the second option `Required`. Both options are in one
/// `SelectExactlyOne` group, so a default that picks the first option makes the import count two picks.
const INTERNAL_FLAG_INSTALLER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<config>
    <moduleName>Flagged Installer</moduleName>
    <installSteps order="Explicit">
        <installStep name="choice">
            <optionalFileGroups order="Explicit">
                <group name="variant" type="SelectExactlyOne">
                    <plugins>
                        <plugin name="first">
                            <files><file source="a/first.txt" destination="first.txt"/></files>
                            <conditionFlags><flag name="gate">on</flag></conditionFlags>
                            <typeDescriptor><type name="Recommended"/></typeDescriptor>
                        </plugin>
                        <plugin name="second">
                            <files><file source="a/second.txt" destination="second.txt"/></files>
                            <typeDescriptor>
                                <dependencyType>
                                    <defaultType name="Optional"/>
                                    <patterns>
                                        <pattern>
                                            <dependencies operator="And">
                                                <flagDependency flag="gate" value="on"/>
                                            </dependencies>
                                            <type name="Required"/>
                                        </pattern>
                                    </patterns>
                                </dependencyType>
                            </typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

fn installer() -> ModuleConfig {
    parse(INSTALLER).expect("the fixture is valid XML")
}

fn pick(step: u32, group_index: u32, group: &str, option: &str) -> Choice {
    Choice {
        step,
        group_index,
        group: group.to_string(),
        option: option.to_string(),
    }
}

/// The wizard's answer for the defaults.
fn defaults(config: &ModuleConfig) -> FomodState {
    installer_state(config, None)
}

/// The wizard's answer for one selection.
fn answer(config: &ModuleConfig, choices: &[Choice]) -> FomodState {
    installer_state(config, Some(choices))
}

/// The source patterns the import installs for a selection, sorted for comparison.
fn sources_of(config: &ModuleConfig, choices: &[Choice]) -> Result<Vec<String>, FomodError> {
    resolve_sources(config, choices).map(|sources| {
        let mut names: Vec<String> = sources.into_iter().map(|pattern| pattern.source).collect();
        names.sort();
        names
    })
}

#[test]
fn the_defaults_show_every_step_the_recommended_pick_opens() {
    let state = defaults(&installer());
    // The recommended `on` sets mode=On, so the gated step is shown with the others.
    assert_eq!(state.visible, [0, 1, 2, 3]);
    assert_eq!(
        state.selection,
        [
            pick(0, 0, "mode", "on"),
            pick(0, 1, "extras", "extra"),
            pick(3, 0, "bundle", "all-a"),
        ]
    );
    // The required `core.txt` is not a choice, so the count holds the three picks only.
    assert_eq!(state.picked, 3);
}

#[test]
fn the_default_selection_is_the_one_the_import_accepts() {
    let config = installer();
    let state = defaults(&config);
    assert_eq!(
        sources_of(&config, &state.selection).expect("the defaults are a valid selection"),
        ["a/on.txt", "b/extra.txt", "base/core.txt", "e/a.txt"]
    );
}

#[test]
fn a_hidden_step_is_listed_with_nothing_picked_and_its_choices_are_dropped() {
    let config = installer();
    let state = answer(
        &config,
        &[pick(0, 0, "mode", "off"), pick(1, 0, "pick", "one")],
    );
    assert_eq!(state.visible, [0, 2, 3]);
    assert_eq!(state.steps.len(), config.steps.len());
    assert!(
        state.steps[1].groups[0]
            .options
            .iter()
            .all(|option| !option.picked),
        "a hidden step installs nothing"
    );
    assert_eq!(state.selection, [pick(0, 0, "mode", "off")]);
}

#[test]
fn each_group_rule_is_reported_as_its_problem() {
    let config = installer();
    // No mode pick: exactly-one fails. No extra: at-least-one fails.
    let none = answer(&config, &[]);
    assert_eq!(none.steps[0].groups[0].problem, Some(GroupProblem::PickOne));
    assert_eq!(
        none.steps[0].groups[1].problem,
        Some(GroupProblem::PickAtLeastOne)
    );

    // Two modes: exactly-one fails. The later `off` sets mode=Off, so the gated step is hidden and not checked.
    let two = answer(
        &config,
        &[
            pick(0, 0, "mode", "on"),
            pick(0, 0, "mode", "off"),
            pick(0, 1, "extras", "extra"),
        ],
    );
    assert_eq!(two.steps[0].groups[0].problem, Some(GroupProblem::PickOne));
    assert_eq!(two.visible, [0, 2, 3]);

    // Two picks in the gated at-most-one group fail as at-most, with the gate open.
    let most = answer(
        &config,
        &[
            pick(0, 0, "mode", "on"),
            pick(0, 1, "extras", "extra"),
            pick(1, 0, "pick", "one"),
            pick(1, 0, "pick", "two"),
        ],
    );
    assert_eq!(
        most.steps[1].groups[0].problem,
        Some(GroupProblem::PickAtMostOne)
    );

    // A fine selection has no problem anywhere.
    let fine = answer(
        &config,
        &[pick(0, 0, "mode", "on"), pick(0, 1, "extras", "more")],
    );
    assert!(
        fine.steps
            .iter()
            .all(|step| step.groups.iter().all(|group| group.problem.is_none()))
    );
}

#[test]
fn the_state_and_the_import_report_the_same_rule() {
    let config = installer();
    let choices = [
        pick(0, 0, "mode", "on"),
        pick(0, 1, "extras", "extra"),
        pick(1, 0, "pick", "one"),
        pick(1, 0, "pick", "two"),
    ];
    let state = answer(&config, &choices);
    assert_eq!(
        state.steps[1].groups[0].problem,
        Some(GroupProblem::PickAtMostOne)
    );
    assert!(matches!(
        resolve_sources(&config, &choices),
        Err(FomodError::GroupRule {
            rule: "fomod.group_pick_at_most",
            ..
        })
    ));
}

#[test]
fn a_dependency_type_makes_an_option_required_for_the_wizard_and_the_import() {
    let config = installer();
    let choices = [
        pick(0, 0, "mode", "on"),
        pick(0, 1, "extras", "extra"),
        pick(2, 0, "tiers", "trigger"),
    ];
    let state = answer(&config, &choices);
    // `trigger` sets tier=high, so `tiered` is effectively required: the wizard shows it picked, and the import
    // installs its file although the user never chose it.
    let tiered = &state.steps[2].groups[0].options[1];
    assert_eq!(tiered.kind, OptionKind::Required);
    assert!(tiered.picked);
    assert_eq!(state.selection, choices);
    let sources = sources_of(&config, &choices).expect("the selection is valid");
    assert!(sources.contains(&"d/tiered.txt".to_string()));
}

#[test]
fn a_default_that_sets_a_flag_making_a_later_option_required_has_no_group_problem() {
    let config = parse(INTERNAL_FLAG_INSTALLER).expect("the fixture is valid XML");
    let state = defaults(&config);
    // The wizard's own defaults must satisfy the import: picking the flag-setting `first` would make `second`
    // required and count two picks in a SelectExactlyOne group.
    assert!(
        state.steps[0].groups[0].problem.is_none(),
        "the defaults must not leave an uncleared group error"
    );
    assert!(
        sources_of(&config, &state.selection).is_ok(),
        "the import must accept the wizard's own defaults"
    );
}

#[test]
fn a_select_all_group_picks_every_usable_option_and_hides_the_unusable_one() {
    let config = installer();
    let state = defaults(&config);
    let options = &state.steps[3].groups[0].options;
    assert_eq!(options[0].kind, OptionKind::Optional);
    assert!(options[0].picked);
    assert_eq!(options[1].kind, OptionKind::NotUsable);
    assert!(!options[1].picked);
}

#[test]
fn a_choice_the_installer_does_not_define_is_ignored_by_the_state() {
    let config = installer();
    let state = answer(
        &config,
        &[
            pick(0, 0, "mode", "on"),
            pick(0, 1, "extras", "extra"),
            pick(0, 0, "mode", "missing"),
        ],
    );
    assert!(state.steps[0].groups[0].problem.is_none());
    assert_eq!(
        state.selection,
        [pick(0, 0, "mode", "on"), pick(0, 1, "extras", "extra")]
    );
}

#[test]
fn a_click_on_a_radio_group_replaces_the_group_pick() {
    let view = defaults(&installer());
    assert_eq!(
        toggle_pick(&view, 0, 0, "off"),
        [
            pick(0, 1, "extras", "extra"),
            pick(3, 0, "bundle", "all-a"),
            pick(0, 0, "mode", "off"),
        ]
    );
}

#[test]
fn a_click_on_an_at_most_one_group_toggles_its_own_pick() {
    let config = installer();
    let view = answer(
        &config,
        &[
            pick(0, 0, "mode", "on"),
            pick(0, 1, "extras", "extra"),
            pick(1, 0, "pick", "one"),
        ],
    );
    let kept = [pick(0, 0, "mode", "on"), pick(0, 1, "extras", "extra")];
    assert_eq!(toggle_pick(&view, 1, 0, "one"), kept);
    assert_eq!(
        toggle_pick(&view, 1, 0, "two"),
        [kept[0].clone(), kept[1].clone(), pick(1, 0, "pick", "two")]
    );
}

#[test]
fn a_click_on_a_checkbox_group_toggles_one_option_and_keeps_the_rest() {
    let view = defaults(&installer());
    let with_more = [
        pick(0, 0, "mode", "on"),
        pick(0, 1, "extras", "extra"),
        pick(3, 0, "bundle", "all-a"),
        pick(0, 1, "extras", "more"),
    ];
    assert_eq!(toggle_pick(&view, 0, 1, "more"), with_more);
    assert_eq!(
        toggle_pick(&view, 0, 1, "extra"),
        [pick(0, 0, "mode", "on"), pick(3, 0, "bundle", "all-a")]
    );
}

#[test]
fn a_click_select_all_unusable_required_or_hidden_changes_nothing() {
    let config = installer();
    let view = defaults(&config);
    // `all-a` is picked by SelectAll and `all-b` is NotUsable; neither is a choice.
    assert_eq!(toggle_pick(&view, 3, 0, "all-a"), view.selection);
    assert_eq!(toggle_pick(&view, 3, 0, "all-b"), view.selection);

    // With `mode=off` the gated step is hidden, so its group has no choices.
    let hidden = answer(&config, &[pick(0, 0, "mode", "off")]);
    assert_eq!(toggle_pick(&hidden, 1, 0, "one"), hidden.selection);

    // A `<dependencyType>` that makes an option Required installs it without a choice.
    let config = parse(INTERNAL_FLAG_INSTALLER).expect("the fixture is valid XML");
    let flagged = answer(&config, &[pick(0, 0, "variant", "first")]);
    assert_eq!(
        flagged.steps[0].groups[0].options[1].kind,
        OptionKind::Required
    );
    assert_eq!(toggle_pick(&flagged, 0, 0, "second"), flagged.selection);
}

#[test]
fn saved_choices_are_laid_over_the_defaults_with_the_click_rules() {
    let defaults = defaults(&installer());
    let saved = [
        pick(0, 0, "mode", "on"),
        pick(0, 1, "extras", "missing"),
        pick(9, 0, "gone", "on"),
        pick(1, 0, "pick", "one"),
        pick(1, 0, "pick", "two"),
    ];
    assert_eq!(
        selection_from_choices(&defaults, &saved),
        [
            pick(0, 1, "extras", "extra"),
            pick(3, 0, "bundle", "all-a"),
            pick(0, 0, "mode", "on"),
            pick(1, 0, "pick", "two"),
        ]
    );
}

#[test]
fn a_saved_choice_uses_the_installers_group_name_and_skips_fixed_options() {
    let config = installer();
    let view = answer(
        &config,
        &[
            pick(0, 0, "mode", "on"),
            pick(0, 1, "extras", "extra"),
            pick(2, 0, "tiers", "trigger"),
        ],
    );
    // `tiered` is Required and `all-b` is NotUsable, so neither is copied from a saved answer.
    let saved = [
        Choice {
            group: "renamed".to_string(),
            ..pick(0, 0, "mode", "on")
        },
        pick(2, 0, "tiers", "tiered"),
        pick(3, 0, "bundle", "all-b"),
    ];
    assert_eq!(
        selection_from_choices(&view, &saved),
        [
            pick(0, 1, "extras", "extra"),
            pick(2, 0, "tiers", "trigger"),
            pick(0, 0, "mode", "on"),
        ]
    );
}
