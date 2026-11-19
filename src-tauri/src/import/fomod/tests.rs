//! FOMOD tests: installer steps, conditions and the files each choice selects.

mod choices;
mod files;
mod parsing;
mod properties;
mod spec;
mod state;
mod visibility;

use super::errors::FomodError;
use super::model::ModuleConfig;
use super::sources::step_visible;
use super::*;

const CONFIG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <config xmlns="http://qconsulting.ca/fo3/ModConfig5.0">
        <moduleName>Fixture Mod</moduleName>
        <installSteps order="Explicit">
            <installStep name="visuals">
                <optionalFileGroups order="Explicit">
                    <group name="textures" type="SelectExactlyOne">
                        <plugins>
                            <plugin name="option-a">
                                <description>Light textures</description>
                                <files>
                                    <folder source="textures/low" destination="textures" priority="0"/>
                                </files>
                                <conditionFlags>
                                    <flag name="quality">low</flag>
                                </conditionFlags>
                                <typeDescriptor>
                                    <type name="Recommended"/>
                                </typeDescriptor>
                            </plugin>
                            <plugin name="option-b">
                                <files>
                                    <folder source="textures/high" destination="textures"/>
                                </files>
                                <typeDescriptor>
                                    <type name="Optional"/>
                                </typeDescriptor>
                            </plugin>
                        </plugins>
                    </group>
                    <group name="patch" type="SelectAny">
                        <plugins>
                            <plugin name="font-patch">
                                <files>
                                    <file source="fonts/*" destination="fonts"/>
                                </files>
                                <typeDescriptor>
                                    <type name="Required"/>
                                </typeDescriptor>
                            </plugin>
                        </plugins>
                    </group>
                </optionalFileGroups>
            </installStep>
        </installSteps>
        <conditionalFileInstalls>
            <patterns>
                <pattern>
                    <dependencies operator="And">
                        <flagDependency flag="quality" value="low"/>
                        <fileDependency file="fonts/base.nif" state="Active"/>
                    </dependencies>
                    <files>
                        <folder source="patch/hd" destination="patch"/>
                    </files>
                </pattern>
            </patterns>
        </conditionalFileInstalls>
    </config>"#;

fn config() -> ModuleConfig {
    parse(CONFIG).expect("valid XML")
}

#[test]
fn a_nested_or_group_keeps_its_operator() {
    use super::model::{Condition, Dependencies, Dependency};
    use super::sources::satisfied;

    let dependencies = Dependencies::Any(vec![
        Condition::Dependency(Dependency::Flag {
            flag: "a".to_string(),
            value: "1".to_string(),
        }),
        Condition::Group(Dependencies::All(vec![
            Condition::Dependency(Dependency::Flag {
                flag: "b".to_string(),
                value: "1".to_string(),
            }),
            Condition::Dependency(Dependency::Flag {
                flag: "c".to_string(),
                value: "1".to_string(),
            }),
        ])),
    ]);
    let installed = rustc_hash::FxHashSet::default();
    let mut flags = rustc_hash::FxHashMap::default();
    flags.insert("a".to_string(), "0".to_string());
    flags.insert("b".to_string(), "1".to_string());
    flags.insert("c".to_string(), "1".to_string());
    assert!(satisfied(&dependencies, &flags, &installed));

    flags.insert("a".to_string(), "1".to_string());
    flags.insert("b".to_string(), "0".to_string());
    assert!(satisfied(&dependencies, &flags, &installed));

    flags.insert("a".to_string(), "0".to_string());
    assert!(!satisfied(&dependencies, &flags, &installed));
}
