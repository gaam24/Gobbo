//! FOMOD 5.1 coverage: required files, group types, file attributes, dependency types and module dependencies.
//! Fixtures use generic names so they read the same on every platform and for every game.

use super::*;

/// A choice in the first group of a step.
fn choice_in(step: u32, group: &str, option: &str) -> Choice {
    Choice {
        step,
        group_index: 0,
        group: group.to_string(),
        option: option.to_string(),
    }
}

/// A choice in the first group of the first step.
fn choice(group: &str, option: &str) -> Choice {
    choice_in(0, group, option)
}

/// The sources a config installs, as plain strings in resolution order.
fn selected_sources(config: &ModuleConfig, choices: &[Choice]) -> Vec<String> {
    resolve_sources(config, choices)
        .expect("the choices are valid")
        .into_iter()
        .map(|pattern| pattern.source)
        .collect()
}

const REQUIRED: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <requiredInstallFiles>
        <file source="core/settings.ini" destination="config"/>
        <folder source="content/shared" destination="content"/>
    </requiredInstallFiles>
    <installSteps>
        <installStep name="mode">
            <optionalFileGroups>
                <group name="variant" type="SelectAny">
                    <plugins>
                        <plugin name="option-a">
                            <files><file source="a/one.txt" destination="a"/></files>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

const LEGACY_REQUIRED: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <requiredInstall>
        <files>
            <plugin name="always">
                <files><folder source="content" destination="content"/></files>
            </plugin>
        </files>
    </requiredInstall>
</config>"#;

const SELECT_ALL: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <installSteps>
        <installStep name="all">
            <optionalFileGroups>
                <group name="everything" type="SelectAll">
                    <plugins>
                        <plugin name="option-a">
                            <files><file source="a/one.txt" destination="a"/></files>
                        </plugin>
                        <plugin name="option-b">
                            <files><file source="b/two.txt" destination="b"/></files>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

const FILE_FLAGS: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <installSteps>
        <installStep name="flags">
            <optionalFileGroups>
                <group name="modes" type="SelectAny">
                    <plugins>
                        <plugin name="option-a">
                            <files><file source="a/selected.txt" destination="a"/></files>
                            <conditionFlags><flag name="variant">high</flag></conditionFlags>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="option-b">
                            <files>
                                <file source="b/always.txt" destination="b" alwaysInstall="true"/>
                                <file source="b/usable.txt" destination="b" installIfUsable="true"/>
                                <file source="b/never.txt" destination="b"/>
                            </files>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="option-c">
                            <files>
                                <file source="c/usable.txt" destination="c" installIfUsable="true"/>
                            </files>
                            <typeDescriptor>
                                <dependencyType>
                                    <defaultType name="Optional"/>
                                    <patterns>
                                        <pattern>
                                            <dependencies>
                                                <flagDependency flag="variant" value="high"/>
                                            </dependencies>
                                            <type name="NotUsable"/>
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

const PRIORITY: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <installSteps>
        <installStep name="first">
            <optionalFileGroups>
                <group name="late" type="SelectAny">
                    <plugins>
                        <plugin name="plugin-late">
                            <files><file source="z/late.txt" destination="shared" priority="10"/></files>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
        <installStep name="second">
            <optionalFileGroups>
                <group name="early" type="SelectAny">
                    <plugins>
                        <plugin name="plugin-early">
                            <files><file source="a/early.txt" destination="shared" priority="-5"/></files>
                        </plugin>
                        <plugin name="plugin-middle">
                            <files><file source="m/middle.txt" destination="shared" priority="0"/></files>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

const DEPENDENCY_TYPE: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <installSteps>
        <installStep name="setup">
            <optionalFileGroups>
                <group name="flags" type="SelectAny">
                    <plugins>
                        <plugin name="option-a">
                            <files><file source="a/set.txt" destination="a"/></files>
                            <conditionFlags><flag name="variant">high</flag></conditionFlags>
                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                        </plugin>
                        <plugin name="option-b">
                            <files>
                                <file source="b/usable.txt" destination="b" installIfUsable="true"/>
                            </files>
                            <typeDescriptor>
                                <dependencyType>
                                    <defaultType name="Optional"/>
                                    <patterns>
                                        <pattern>
                                            <dependencies>
                                                <flagDependency flag="variant" value="high"/>
                                            </dependencies>
                                            <type name="NotUsable"/>
                                        </pattern>
                                        <pattern>
                                            <dependencies>
                                                <flagDependency flag="variant" value="high"/>
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

const MODULE_DEPENDENCIES: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <moduleDependencies operator="And">
        <flagDependency flag="preset" value="On"/>
    </moduleDependencies>
    <installSteps>
        <installStep name="only">
            <optionalFileGroups>
                <group name="g" type="SelectAny"><plugins/></group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

const GAME_DEPENDENCY: &str = r#"<config>
    <moduleName>Fixture Mod</moduleName>
    <moduleDependencies operator="And">
        <gameDependency version="99.0"/>
        <fommDependency version="99.0"/>
    </moduleDependencies>
    <installSteps>
        <installStep name="only">
            <optionalFileGroups>
                <group name="g" type="SelectAny">
                    <plugins>
                        <plugin name="option-a">
                            <files><file source="a/one.txt" destination="a"/></files>
                        </plugin>
                    </plugins>
                </group>
            </optionalFileGroups>
        </installStep>
    </installSteps>
</config>"#;

#[test]
fn required_install_files_are_parsed_as_a_file_list() {
    let config = parse(REQUIRED).expect("valid XML");
    assert_eq!(config.required_files.len(), 2);
    assert_eq!(config.required_files[0].source, "core/settings.ini");
    assert_eq!(config.required_files[1].source, "content/shared");
    assert!(config.required_files.iter().all(|p| p.always_install));
    // Required files are not a step, so they do not shift the choice indices of the real steps.
    assert_eq!(config.steps.len(), 1);
    assert_eq!(config.steps[0].name, "mode");
}

#[test]
fn required_install_files_are_installed_without_a_choice() {
    let config = parse(REQUIRED).expect("valid XML");
    assert_eq!(
        selected_sources(&config, &[]),
        ["core/settings.ini", "content/shared"]
    );
    assert_eq!(
        selected_sources(&config, &[choice("variant", "option-a")]),
        ["core/settings.ini", "content/shared", "a/one.txt"]
    );
}

#[test]
fn the_legacy_required_install_form_still_installs_its_files() {
    let config = parse(LEGACY_REQUIRED).expect("valid XML");
    assert_eq!(selected_sources(&config, &[]), ["content"]);
}

#[test]
fn a_select_all_group_is_parsed_and_picks_every_option() {
    let config = parse(SELECT_ALL).expect("valid XML");
    assert_eq!(config.steps[0].groups[0].kind, GroupKind::SelectAll);
    assert_eq!(selected_sources(&config, &[]), ["a/one.txt", "b/two.txt"]);
}

#[test]
fn file_attributes_are_parsed() {
    let config = parse(FILE_FLAGS).expect("valid XML");
    let files = &config.steps[0].groups[0].options[1].files;
    assert_eq!(files.len(), 3);
    assert!(files[0].always_install);
    assert!(!files[0].install_if_usable);
    assert!(!files[1].always_install);
    assert!(files[1].install_if_usable);
    assert!(!files[2].always_install);
    assert!(!files[2].install_if_usable);
    assert!(files.iter().all(|pattern| pattern.priority == 0));
}

#[test]
fn always_install_and_install_if_usable_files_go_in_without_a_choice() {
    let config = parse(FILE_FLAGS).expect("valid XML");
    // No option is picked and no flag is set, so option-c keeps its default Optional kind and its usable file goes in.
    assert_eq!(
        selected_sources(&config, &[]),
        ["b/always.txt", "b/usable.txt", "c/usable.txt"]
    );
}

#[test]
fn a_not_usable_option_does_not_install_its_usable_files() {
    let config = parse(FILE_FLAGS).expect("valid XML");
    // option-a sets variant=high, which makes option-c NotUsable at resolve time.
    assert_eq!(
        selected_sources(&config, &[choice("modes", "option-a")]),
        ["a/selected.txt", "b/always.txt", "b/usable.txt"]
    );
}

#[test]
fn a_higher_priority_is_resolved_later_and_wins_on_an_equal_destination() {
    let config = parse(PRIORITY).expect("valid XML");
    let priorities: Vec<i32> = config
        .steps
        .iter()
        .flat_map(|step| &step.groups)
        .flat_map(|group| &group.options)
        .flat_map(|option| &option.files)
        .map(|pattern| pattern.priority)
        .collect();
    assert_eq!(priorities, [10, -5, 0]);
    let choices = [
        choice_in(0, "late", "plugin-late"),
        choice_in(1, "early", "plugin-early"),
        choice_in(1, "early", "plugin-middle"),
    ];
    assert_eq!(
        selected_sources(&config, &choices),
        ["a/early.txt", "m/middle.txt", "z/late.txt"]
    );
}

#[test]
fn a_dependency_type_keeps_its_parse_time_default_kind() {
    let config = parse(DEPENDENCY_TYPE).expect("valid XML");
    let option = &config.steps[0].groups[0].options[1];
    assert_eq!(option.kind, OptionKind::Optional);
    let dependency_type = option
        .dependency_type
        .as_ref()
        .expect("the option has a dependency type");
    assert_eq!(dependency_type.default_kind, OptionKind::Optional);
    assert_eq!(dependency_type.patterns.len(), 2);
    assert_eq!(dependency_type.patterns[0].kind, OptionKind::NotUsable);
    assert_eq!(dependency_type.patterns[1].kind, OptionKind::Required);
}

#[test]
fn the_first_matching_dependency_pattern_decides_the_effective_kind() {
    let config = parse(DEPENDENCY_TYPE).expect("valid XML");
    // Both patterns match variant=high, but the first one (NotUsable) wins, so no usable file is installed.
    assert_eq!(
        selected_sources(&config, &[choice("flags", "option-a")]),
        ["a/set.txt"]
    );
}

#[test]
fn a_dependency_type_uses_its_default_when_no_pattern_matches() {
    let config = parse(DEPENDENCY_TYPE).expect("valid XML");
    // Without the flag, no pattern matches; the default Optional kind installs the usable file.
    assert_eq!(selected_sources(&config, &[]), ["b/usable.txt"]);
}

#[test]
fn module_dependencies_are_parsed() {
    let config = parse(MODULE_DEPENDENCIES).expect("valid XML");
    let dependencies = config
        .module_dependencies
        .as_ref()
        .expect("the config has module dependencies");
    assert!(matches!(dependencies, Dependencies::All(children) if children.len() == 1));
}

#[test]
fn unmet_module_dependencies_are_reported_with_a_stable_code() {
    let config = parse(MODULE_DEPENDENCIES).expect("valid XML");
    let Err(error) = resolve_sources(&config, &[]) else {
        panic!("module dependencies cannot be met before any step");
    };
    assert_eq!(error, FomodError::ModuleDependenciesUnmet);
    let app: crate::error::AppError = error.into();
    assert_eq!(app.code, "fomod.module_dependencies_unmet");
}

#[test]
fn game_and_fomm_dependencies_are_treated_as_met() {
    // The game version is not known at import time, and Gobbo is not FOMM, so neither version can fail the import.
    let config = parse(GAME_DEPENDENCY).expect("valid XML");
    assert_eq!(
        selected_sources(&config, &[choice("g", "option-a")]),
        ["a/one.txt"]
    );
}
