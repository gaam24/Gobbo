//! Reading `ModuleConfig.xml` into the types of `model`.

use std::collections::BTreeMap;

use super::errors::FomodError;
use super::model::{
    Condition, Conditional, Dependencies, Dependency, DependencyType, FomodOption, Group,
    GroupKind, ModuleConfig, OptionKind, Pattern, Step, TypePattern,
};

/// Parses the `ModuleConfig.xml` content. Versions without `installSteps` (only `requiredInstall`) give
/// a "Required" step with one group, so the wizard has something to show.
pub fn parse(xml: &str) -> Result<ModuleConfig, FomodError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| FomodError::Parse {
        message: e.to_string(),
    })?;
    let root = doc.root_element();

    if root.tag_name().name() != "config" {
        return Err(FomodError::Parse {
            message: format!(
                "the first tag is <{}>, but it must be <config>",
                root.tag_name().name()
            ),
        });
    }

    let module_name = text_of_child(root, "moduleName").unwrap_or_default();
    // `fommDependency` inside this tree is ignored: Gobbo is not FOMM and the version of that tool is irrelevant.
    // `gameDependency` is parsed as met; the game version is not known at import time.
    let module_dependencies = child(root, "moduleDependencies")
        .map(parse_dependencies)
        .transpose()?;
    // The spec's `requiredInstallFiles` is a file list. Its files go in without a choice and before every step.
    // It may coexist with `installSteps`, so it is kept out of `steps` to leave their choice indices untouched.
    let required_files = child(root, "requiredInstallFiles").map_or_else(Vec::new, |node| {
        parse_patterns(node)
            .into_iter()
            .map(|mut pattern| {
                pattern.always_install = true;
                pattern
            })
            .collect()
    });

    let mut steps = Vec::new();

    if let Some(steps_node) = child(root, "installSteps") {
        for step_node in steps_node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "installStep")
        {
            let name = attr(step_node, "name").unwrap_or_default();
            // `<visible>` is itself a composite dependency. Its usual `<dependencies>` child is parsed as a nested
            // group, which keeps that child's operator instead of flattening it into the outer one.
            let visible = child(step_node, "visible")
                .map(parse_dependencies)
                .transpose()?;
            let mut groups = Vec::new();

            if let Some(groups_node) = child(step_node, "optionalFileGroups") {
                for group_node in groups_node
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "group")
                {
                    groups.push(parse_group(group_node)?);
                }
            }

            steps.push(Step {
                name,
                visible,
                groups,
            });
        }
    } else if let Some(required) = child(root, "requiredInstall") {
        // Tolerant fallback for the older, invented `<requiredInstall><files><plugin …/></files>` form.
        let mut groups = Vec::new();

        for files_node in required
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "files")
        {
            groups.push(Group {
                name: "Required".to_string(),
                kind: GroupKind::SelectAtLeastOne,
                // These plugins are required by definition, so they are picked without a wizard choice.
                options: parse_plugins(files_node)?
                    .into_iter()
                    .map(|mut option| {
                        option.kind = OptionKind::Required;
                        option
                    })
                    .collect(),
            });
        }

        steps.push(Step {
            name: "Required".to_string(),
            visible: None,
            groups,
        });
    }

    let mut conditional = Vec::new();

    if let Some(node) = child(root, "conditionalFileInstalls")
        && let Some(patterns) = child(node, "patterns")
    {
        for pattern in patterns
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "pattern")
        {
            let dependencies = match child(pattern, "dependencies") {
                Some(deps) => parse_dependencies(deps)?,
                None => Dependencies::All(Vec::new()),
            };
            let files = child(pattern, "files").map_or_else(Vec::new, parse_patterns);
            conditional.push(Conditional {
                dependencies,
                files,
            });
        }
    }

    Ok(ModuleConfig {
        module_name,
        required_files,
        steps,
        module_dependencies,
        conditional,
    })
}

fn parse_group(node: roxmltree::Node) -> Result<Group, FomodError> {
    let name = attr(node, "name").unwrap_or_default();
    let kind = match attr(node, "type").unwrap_or_default().as_str() {
        "SelectAtLeastOne" => GroupKind::SelectAtLeastOne,
        "SelectExactlyOne" => GroupKind::SelectExactlyOne,
        "SelectAtMostOne" => GroupKind::SelectAtMostOne,
        "SelectAll" => GroupKind::SelectAll,
        _ => GroupKind::SelectAny,
    };
    let options = child(node, "plugins")
        .map(parse_plugins)
        .transpose()?
        .unwrap_or_default();
    Ok(Group {
        name,
        kind,
        options,
    })
}

fn parse_plugins(node: roxmltree::Node) -> Result<Vec<FomodOption>, FomodError> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "plugin")
        .map(parse_option)
        .collect()
}

fn parse_option(node: roxmltree::Node) -> Result<FomodOption, FomodError> {
    // The option name is the `<plugin name="...">` attribute, and the description is the child element.
    let name = attr(node, "name").unwrap_or_default();
    let description = text_of_child(node, "description").unwrap_or_default();
    let files = child(node, "files").map_or_else(Vec::new, parse_patterns);
    let mut flags = BTreeMap::new();

    if let Some(flags_node) = child(node, "conditionFlags") {
        for flag in flags_node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "flag")
        {
            if let Some(key) = attr(flag, "name") {
                flags.insert(key, flag.text().unwrap_or_default().trim().to_string());
            }
        }
    }
    // The parse-time kind is the `<defaultType>` of a `<dependencyType>`. The walk evaluates that dependency type
    // against the flags and installed files set before the option, so the effective kind can differ.
    let (kind, dependency_type) = child(node, "typeDescriptor")
        .map(parse_type_descriptor)
        .transpose()?
        .unwrap_or((OptionKind::Optional, None));
    Ok(FomodOption {
        name,
        description,
        kind,
        files,
        flags,
        dependency_type,
    })
}

/// Reads a `<typeDescriptor>`: either a plain `<type>` or a `<dependencyType>` with a default kind and patterns.
fn parse_type_descriptor(
    node: roxmltree::Node,
) -> Result<(OptionKind, Option<DependencyType>), FomodError> {
    let Some(dependency) = child(node, "dependencyType") else {
        let kind = child(node, "type")
            .and_then(|kind| attr(kind, "name").as_deref().map(parse_option_kind))
            .unwrap_or(OptionKind::Optional);
        return Ok((kind, None));
    };
    let default_kind = child(dependency, "defaultType")
        .and_then(|kind| attr(kind, "name").as_deref().map(parse_option_kind))
        .unwrap_or(OptionKind::Optional);
    let mut patterns = Vec::new();

    if let Some(patterns_node) = child(dependency, "patterns") {
        for node in patterns_node
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "pattern")
        {
            let dependencies = child(node, "dependencies")
                .map(parse_dependencies)
                .transpose()?
                .unwrap_or_else(|| Dependencies::All(Vec::new()));
            let kind = child(node, "type")
                .and_then(|kind| attr(kind, "name").as_deref().map(parse_option_kind))
                .unwrap_or(OptionKind::Optional);
            // The first matching pattern wins in the resolver, so the XML order is kept.
            patterns.push(TypePattern { dependencies, kind });
        }
    }

    Ok((
        default_kind,
        Some(DependencyType {
            default_kind,
            patterns,
        }),
    ))
}

fn parse_option_kind(text: &str) -> OptionKind {
    match text.trim() {
        "Required" => OptionKind::Required,
        "Recommended" => OptionKind::Recommended,
        "NotUsable" => OptionKind::NotUsable,
        "CouldBeUsable" => OptionKind::CouldBeUsable,
        _ => OptionKind::Optional,
    }
}

fn parse_patterns(node: roxmltree::Node) -> Vec<Pattern> {
    node.children()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "file" | "folder"))
        .map(|pattern| Pattern {
            source: attr(pattern, "source").unwrap_or_default(),
            destination: attr(pattern, "destination").unwrap_or_default(),
            always_install: parse_bool(attr(pattern, "alwaysInstall").as_deref()),
            install_if_usable: parse_bool(attr(pattern, "installIfUsable").as_deref()),
            priority: parse_priority(attr(pattern, "priority").as_deref()),
        })
        .collect()
}

/// Reads an `xs:boolean` FOMOD attribute. An absent or unknown value keeps the spec default of false.
fn parse_bool(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
}

/// Reads an `xs:integer` FOMOD attribute. An absent or malformed value keeps the spec default of zero.
fn parse_priority(value: Option<&str>) -> i32 {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn parse_dependencies(node: roxmltree::Node) -> Result<Dependencies, FomodError> {
    let mut deps = Vec::new();

    for child_node in node.children().filter(roxmltree::Node::is_element) {
        match child_node.tag_name().name() {
            "fileDependency" => deps.push(Condition::Dependency(Dependency::File {
                file: attr(child_node, "file").unwrap_or_default(),
                active: attr(child_node, "state")
                    .unwrap_or_default()
                    .eq_ignore_ascii_case("Active"),
            })),
            "flagDependency" => deps.push(Condition::Dependency(Dependency::Flag {
                flag: attr(child_node, "flag").unwrap_or_default(),
                value: attr(child_node, "value").unwrap_or_default(),
            })),
            // A game version cannot be checked at import time, so `gameDependency` is treated as met.
            "gameDependency" => deps.push(Condition::Dependency(Dependency::Game)),
            // A nested group keeps its own operator; flattening `Or(And(a,b))` into one `All` changed the meaning.
            "dependencies" => deps.push(Condition::Group(parse_dependencies(child_node)?)),
            // Everything else is ignored, including `fommDependency`: Gobbo is not FOMM, so its version is meaningless.
            _ => {}
        }
    }

    Ok(match attr(node, "operator").unwrap_or_default().as_str() {
        "Or" => Dependencies::Any(deps),
        _ => Dependencies::All(deps),
    })
}

fn child<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

fn text_of_child(node: roxmltree::Node, name: &str) -> Option<String> {
    child(node, name)
        .and_then(|n| n.text())
        .map(|text| text.trim().to_string())
}

fn attr(node: roxmltree::Node, name: &str) -> Option<String> {
    node.attribute(name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
