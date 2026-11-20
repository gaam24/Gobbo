use std::collections::{BTreeMap, BTreeSet};

use crate::domain::collection::{
    CollectionIssue, CollectionIssueSeverity, CollectionItem, CollectionSource,
    CollectionUpdatePolicy, OPTIONAL_PHASE,
};
use crate::domain::fomod::Choice;
use crate::domain::naming::normalized_relative_path;
use crate::providers::{CollectionMod, CollectionRevision};

use super::model::{Choices, Mod, Source, issue};
use super::rules::unique_key;

/// The API data, indexed for the manifest's members. The manifest wins for names and choices; the API fills pictures,
/// summaries and sizes the manifest leaves out.
pub(super) struct ApiIndex<'a> {
    by_file: BTreeMap<(String, String), &'a CollectionMod>,
    by_mod: BTreeMap<String, &'a CollectionMod>,
}

impl<'a> ApiIndex<'a> {
    pub(super) fn new(revision: &'a CollectionRevision) -> Self {
        let mut by_file = BTreeMap::new();
        let mut by_mod = BTreeMap::new();

        for module in &revision.mods {
            by_file.insert((module.mod_id.clone(), module.file_id.clone()), module);
            by_mod.entry(module.mod_id.clone()).or_insert(module);
        }

        Self { by_file, by_mod }
    }

    fn find(&self, mod_id: &str, file_id: &str) -> Option<&'a CollectionMod> {
        self.by_file
            .get(&(mod_id.to_string(), file_id.to_string()))
            .copied()
            .or_else(|| self.by_mod.get(mod_id).copied())
    }
}

pub(super) fn item_from_manifest(
    module: Mod,
    api: &ApiIndex<'_>,
    seen: &mut BTreeSet<String>,
    issues: &mut Vec<CollectionIssue>,
) -> CollectionItem {
    let source = module.source;
    let source_file_size = source.file_size;
    let source_logical = source.logical_filename.clone();
    let source_expression = source.file_expression.clone();
    let source_md5 = source.md5.clone();
    let source_policy = source.update_policy.clone();
    let source_instructions = source.instructions.clone();
    let source_adult = source.adult_content;
    let api_mod = source
        .mod_id
        .zip(source.file_id)
        .and_then(|(mod_id, file_id)| api.find(&mod_id.to_string(), &file_id.to_string()))
        .or_else(|| {
            source
                .mod_id
                .and_then(|mod_id| api.find(&mod_id.to_string(), ""))
        });
    let (source, key_base, unsupported) = source_of(&source, module.name.trim());
    let key = unique_key(key_base, source_suffix(&source), seen);
    let name = if module.name.trim().is_empty() {
        api_mod.map_or_else(|| key.clone(), |module| module.name.clone())
    } else {
        module.name.trim().to_string()
    };
    let (choices, mut choice_issues) = choices(module.choices);
    issues.append(&mut choice_issues);
    let (patches, mut patch_issues) = patches(module.patches);
    issues.append(&mut patch_issues);

    if !module.file_overrides.is_empty() {
        issues.push(issue(
            CollectionIssueSeverity::Unsupported,
            Some(key.clone()),
            "collection.file_overrides_ignored",
            [("name", name.clone())],
        ));
    }

    let size_kb = source_file_size.map_or_else(
        || api_mod.map_or(0, |module| module.size_kb),
        |bytes| u32::try_from(bytes.saturating_div(1024)).unwrap_or(u32::MAX),
    );

    if unsupported {
        issues.push(issue(
            CollectionIssueSeverity::Unsupported,
            Some(key.clone()),
            "collection.unsupported_source",
            [("name", name.clone())],
        ));
    }

    let phase = module.phase.unwrap_or(0);
    let mut item = CollectionItem {
        key,
        name,
        version: module
            .version
            .or_else(|| api_mod.and_then(|module| module.version.clone())),
        author: module
            .author
            .or_else(|| api_mod.and_then(|module| module.author.clone())),
        category: module
            .details
            .and_then(|details| details.category)
            .or_else(|| api_mod.and_then(|module| module.category.clone())),
        summary: api_mod.map_or_else(String::new, |module| module.summary.clone()),
        picture_url: api_mod.and_then(|module| module.picture_url.clone()),
        optional: module.optional || phase == OPTIONAL_PHASE,
        phase,
        source,
        file_name: source_logical
            .or(source_expression)
            .or_else(|| api_mod.and_then(|module| module.file_name.clone())),
        size_kb,
        md5: source_md5,
        update_policy: source_policy
            .as_deref()
            .map_or(CollectionUpdatePolicy::Exact, parse_policy),
        choices,
        patches,
        instructions: source_instructions.unwrap_or_default(),
        adult: source_adult.unwrap_or(false) || api_mod.is_some_and(|module| module.adult),
    };

    if let Some(text) = module.instructions.filter(|text| !text.trim().is_empty()) {
        if item.instructions.is_empty() {
            item.instructions = text;
        } else {
            item.instructions.push_str("\n\n");
            item.instructions.push_str(&text);
        }
    }

    if matches!(item.source, CollectionSource::Provider { .. }) && api_mod.is_none() {
        issues.push(issue(
            CollectionIssueSeverity::Warning,
            Some(item.key.clone()),
            "collection.not_on_nexus",
            [("name", item.name.clone())],
        ));
    }

    item
}

pub(super) fn item_from_api(key: String, provider: &str, module: &CollectionMod) -> CollectionItem {
    let source = CollectionSource::Provider {
        provider: provider.to_string(),
        mod_id: module.mod_id.clone(),
        file_id: module.file_id.clone(),
        part_id: None,
    };
    CollectionItem {
        key,
        name: module.name.clone(),
        version: module.version.clone(),
        author: module.author.clone(),
        category: module.category.clone(),
        summary: module.summary.clone(),
        picture_url: module.picture_url.clone(),
        optional: module.optional,
        phase: 0,
        source,
        file_name: module.file_name.clone(),
        size_kb: module.size_kb,
        md5: None,
        update_policy: CollectionUpdatePolicy::Exact,
        choices: Vec::new(),
        patches: BTreeMap::new(),
        instructions: String::new(),
        adult: module.adult,
    }
}

#[allow(
    clippy::option_if_let_else,
    reason = "the three values of a source stay together in one place"
)]
fn source_of(source: &Source, name: &str) -> (CollectionSource, String, bool) {
    match source.kind.as_str() {
        "nexus" => match (source.mod_id, source.file_id) {
            (Some(mod_id), Some(file_id)) if mod_id > 0 && file_id > 0 => (
                CollectionSource::Provider {
                    provider: "nexus".to_string(),
                    mod_id: mod_id.to_string(),
                    file_id: file_id.to_string(),
                    part_id: None,
                },
                format!("nexus:{mod_id}"),
                false,
            ),
            _ => (
                CollectionSource::Unsupported {
                    source_kind: "nexus".to_string(),
                },
                format!("unsupported:nexus:{name}"),
                true,
            ),
        },
        "bundle" => match source
            .file_expression
            .as_deref()
            .and_then(normalized_relative_path)
        {
            Some(path) => (
                CollectionSource::Bundle {
                    path: path.clone(),
                    tag: source.tag.clone(),
                },
                format!("bundle:{}", source.tag.as_deref().unwrap_or(&path)),
                false,
            ),
            None => (
                CollectionSource::Unsupported {
                    source_kind: "bundle".to_string(),
                },
                format!("unsupported:bundle:{name}"),
                true,
            ),
        },
        "direct" => match source
            .url
            .as_deref()
            .filter(|url| url.starts_with("https://"))
        {
            Some(url) => (
                CollectionSource::Direct {
                    url: url.to_string(),
                },
                format!("direct:{url}"),
                false,
            ),
            None => (
                CollectionSource::Unsupported {
                    source_kind: "direct".to_string(),
                },
                format!("unsupported:direct:{name}"),
                true,
            ),
        },
        "browse" => match source
            .url
            .as_deref()
            .filter(|url| url.starts_with("https://"))
        {
            Some(url) => (
                CollectionSource::Browse {
                    url: url.to_string(),
                },
                format!("browse:{url}"),
                false,
            ),
            None => (
                CollectionSource::Unsupported {
                    source_kind: "browse".to_string(),
                },
                format!("unsupported:browse:{name}"),
                true,
            ),
        },
        "manual" => (CollectionSource::Manual, format!("manual:{name}"), false),
        other => (
            CollectionSource::Unsupported {
                source_kind: other.to_string(),
            },
            format!("unsupported:{other}:{name}"),
            true,
        ),
    }
}

fn source_suffix(source: &CollectionSource) -> String {
    match source {
        CollectionSource::Provider { file_id, .. } => file_id.clone(),
        CollectionSource::Bundle { path, .. } => path.clone(),
        CollectionSource::Direct { url } | CollectionSource::Browse { url } => url.clone(),
        CollectionSource::Manual => String::new(),
        CollectionSource::Unsupported { source_kind } => source_kind.clone(),
    }
}

fn parse_policy(text: &str) -> CollectionUpdatePolicy {
    match text {
        "latest" => CollectionUpdatePolicy::Latest,
        "prefer" => CollectionUpdatePolicy::Prefer,
        _ => CollectionUpdatePolicy::Exact,
    }
}

fn choices(choices: Option<Choices>) -> (Vec<Choice>, Vec<CollectionIssue>) {
    let Some(choices) = choices else {
        return (Vec::new(), Vec::new());
    };

    if choices
        .kind
        .as_deref()
        .is_some_and(|kind| !kind.eq_ignore_ascii_case("fomod"))
    {
        return (
            Vec::new(),
            vec![issue(
                CollectionIssueSeverity::Warning,
                None,
                "collection.unknown_installer",
                [("kind", choices.kind.unwrap_or_default())],
            )],
        );
    }

    let mut out = Vec::new();

    for (step, choice_step) in choices.options.iter().enumerate() {
        for (group_index, group) in choice_step.groups.iter().enumerate() {
            for choice in &group.choices {
                if choice.name.is_empty() {
                    continue;
                }

                out.push(Choice {
                    step: u32::try_from(step).unwrap_or(u32::MAX),
                    group_index: u32::try_from(group_index).unwrap_or(u32::MAX),
                    group: group.name.clone(),
                    option: choice.name.clone(),
                });
            }
        }
    }

    (out, Vec::new())
}

fn patches(patches: BTreeMap<String, String>) -> (BTreeMap<String, String>, Vec<CollectionIssue>) {
    let mut out = BTreeMap::new();
    let mut issues = Vec::new();

    for (path, crc) in patches {
        let Some(path) = normalized_relative_path(&path) else {
            issues.push(issue(
                CollectionIssueSeverity::Unsupported,
                None,
                "collection.patch_path_invalid",
                [("path", path)],
            ));
            continue;
        };
        let crc = crc.trim().to_ascii_uppercase();

        if crc.is_empty() || !crc.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            issues.push(issue(
                CollectionIssueSeverity::Unsupported,
                None,
                "collection.patch_bad_crc",
                [("path", path)],
            ));
            continue;
        }

        out.insert(path, crc);
    }

    (out, issues)
}
