use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::domain::collection::{
    CollectionIssue, CollectionIssueSeverity, CollectionItem, CollectionPlan, CollectionSource,
    CollectionUpdatePolicy, PLAN_FORMAT, order_items,
};
use crate::error::AppError;
use crate::infra::archive::{Archive, Entry};
use crate::providers::CollectionRevision;

use super::items::{ApiIndex, item_from_api, item_from_manifest};
use super::model::{Manifest, issue};
use super::rules::{manifest_rules, unique_key};

/// The largest `collection.json` Gobbo reads. Real collections are far below this; a larger file is not a recipe.
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

/// The most members one collection may list. A collection is a mod list, not a game engine.
const MAX_ITEMS: usize = 10_000;

/// Reads and bounds the collection manifest: an oversized file or member list is rejected instead of read into memory.
pub fn read_plan(
    archive: &Path,
    revision: &CollectionRevision,
    game_key: &str,
) -> Result<CollectionPlan, AppError> {
    let archive = Archive::open(archive).map_err(AppError::from)?;
    let entry = manifest_entry(&archive)?;
    let bytes = archive
        .read_bytes_limited(entry, MAX_MANIFEST_BYTES)
        .map_err(AppError::from)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::user("collection.manifest_invalid").with("detail", error))?;
    build(manifest, revision, game_key, &archive)
}

/// Builds a plan from the API metadata alone. It has no FOMOD choices, rules, bundles or patches; the installer opens
/// the wizard for a FOMOD and marks external resources for the user.
pub fn plan_from_api(revision: &CollectionRevision, game_key: &str) -> CollectionPlan {
    let mut items = Vec::with_capacity(revision.mods.len());
    let mut seen = BTreeSet::new();
    let mut issues = Vec::new();

    for module in &revision.mods {
        let key = unique_key(
            format!("{}:{}", revision.info.provider, module.mod_id),
            module.file_id.clone(),
            &mut seen,
        );
        items.push(item_from_api(key, &revision.info.provider, module));
    }

    for (index, resource) in revision.external.iter().enumerate() {
        let source = match &resource.url {
            Some(url) if url.starts_with("https://") => {
                CollectionSource::Browse { url: url.clone() }
            }
            Some(_) => {
                issues.push(issue(
                    CollectionIssueSeverity::Unsupported,
                    None,
                    "collection.external_url_unsafe",
                    [("name", resource.name.clone())],
                ));
                CollectionSource::Unsupported {
                    source_kind: resource.resource_type.clone(),
                }
            }
            None => CollectionSource::Manual,
        };
        let name = if resource.name.is_empty() {
            format!("External resource {}", index + 1)
        } else {
            resource.name.clone()
        };
        let key = unique_key(format!("external:{index}"), String::new(), &mut seen);
        items.push(CollectionItem {
            key,
            name,
            version: None,
            author: None,
            category: Some(resource.resource_type.clone()),
            summary: String::new(),
            picture_url: None,
            optional: false,
            phase: 0,
            source,
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: String::new(),
            adult: false,
        });
    }

    let mut info = revision.info.clone();
    info.game_key = game_key.to_string();
    let order = items.iter().map(|item| item.key.clone()).collect();
    CollectionPlan {
        format: PLAN_FORMAT,
        info,
        items,
        rules: Vec::new(),
        issues,
        order,
        has_bundles: false,
        has_patches: false,
        // A provider whose API is the whole recipe (a Thunderstore modpack) has nothing more to read.
        limited: !revision.complete,
    }
}

fn manifest_entry(archive: &Archive) -> Result<usize, AppError> {
    let candidates: Vec<&Entry> = archive
        .entries()
        .iter()
        .filter(|entry| !entry.is_dir && entry.name.rsplit('/').next() == Some("collection.json"))
        .collect();
    candidates
        .first()
        .map(|entry| {
            archive
                .entries()
                .iter()
                .position(|known| known.name == entry.name)
                .unwrap_or(0)
        })
        .ok_or_else(|| AppError::user("collection.manifest_missing"))
}

fn build(
    manifest: Manifest,
    revision: &CollectionRevision,
    game_key: &str,
    archive: &Archive,
) -> Result<CollectionPlan, AppError> {
    if let Some(domain) = manifest
        .info
        .domain_name
        .as_deref()
        .filter(|domain| !domain.trim().is_empty())
        && !domain.eq_ignore_ascii_case(game_key)
    {
        return Err(AppError::user("collection.game_mismatch")
            .with("expected", game_key)
            .with("found", domain));
    }

    if manifest.mods.len() > MAX_ITEMS {
        return Err(AppError::user("collection.too_many_items").with("limit", MAX_ITEMS));
    }

    let api = ApiIndex::new(revision);
    let mut items = Vec::with_capacity(manifest.mods.len());
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();

    for module in manifest.mods {
        items.push(item_from_manifest(module, &api, &mut seen, &mut issues));
    }

    let rules = manifest_rules(&manifest.mod_rules, &items, &mut issues);
    let (order, order_issues) = order_items(&items, &rules);
    issues.extend(order_issues.into_iter().map(|message| CollectionIssue {
        severity: CollectionIssueSeverity::Warning,
        item: None,
        message,
    }));

    if !json_empty(&manifest.load_order) {
        issues.push(issue(
            CollectionIssueSeverity::Unsupported,
            None,
            "collection.game_specific_load_order",
            [],
        ));
    }

    if !json_empty(&manifest.tools) {
        issues.push(issue(
            CollectionIssueSeverity::Unsupported,
            None,
            "collection.tools_not_run",
            [],
        ));
    }

    if !json_empty(&manifest.plugins) || !json_empty(&manifest.plugin_rules) {
        issues.push(issue(
            CollectionIssueSeverity::Unsupported,
            None,
            "collection.game_specific_plugins",
            [],
        ));
    }

    if revision.unsupported_archive_paths.iter().any(|path| {
        archive
            .entries()
            .iter()
            .any(|entry| entry.name.starts_with(path.as_str()))
    }) {
        issues.push(issue(
            CollectionIssueSeverity::Unsupported,
            None,
            "collection.ini_tweaks_not_applied",
            [],
        ));
    }

    let has_bundles = items
        .iter()
        .any(|item| matches!(item.source, CollectionSource::Bundle { .. }));
    let has_patches = items.iter().any(|item| !item.patches.is_empty());
    let mut info = revision.info.clone();
    info.game_key = game_key.to_string();

    if !manifest.info.name.trim().is_empty() {
        info.name = manifest.info.name;
    }

    if let Some(author) = manifest
        .info
        .author
        .filter(|author| !author.trim().is_empty())
    {
        info.author = Some(author);
    }

    if !manifest.info.description.trim().is_empty() {
        info.description = manifest.info.description;
    }

    if !manifest.info.install_instructions.trim().is_empty() {
        info.instructions = manifest.info.install_instructions;
    }

    if !manifest.info.game_versions.is_empty() {
        info.game_versions = manifest.info.game_versions;
    }

    Ok(CollectionPlan {
        format: PLAN_FORMAT,
        info,
        items,
        rules,
        issues,
        order,
        has_bundles,
        has_patches,
        limited: false,
    })
}

fn json_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(value) => !*value,
        Value::Number(_) => false,
        Value::String(text) => text.trim().is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::Object(values) => values.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::collection::CollectionInfo;
    use crate::providers::{CollectionMod, CollectionRevision};

    fn revision() -> CollectionRevision {
        CollectionRevision {
            revision: 1,
            latest_revision: 1,
            info: CollectionInfo {
                revision_label: String::new(),
                provider: "nexus".into(),
                game_key: "examplegame".into(),
                slug: "example".into(),
                revision: 1,
                latest_revision: 1,
                name: "Example".into(),
                author: None,
                summary: String::new(),
                description: String::new(),
                instructions: String::new(),
                picture_url: None,
                adult: false,
                total_size_kb: 0,
                mod_count: 0,
                game_versions: Vec::new(),
                changelog: None,
                url: "https://www.nexusmods.com/games/examplegame/collections/example".into(),
            },
            mods: Vec::new(),
            external: Vec::new(),
            revisions: Vec::new(),
            archive_link: None,
            unsupported_archive_paths: Vec::new(),
            complete: false,
        }
    }

    fn manifest(text: &str) -> Manifest {
        serde_json::from_str(text).unwrap()
    }

    fn archive_with(text: &str, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = crate::test_support::scratch!(name);
        let path = dir.join("collection.7z");
        let _ = std::fs::remove_file(&path);
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        let source = dir.join("collection.json");
        std::fs::write(&source, text).unwrap();
        let entry = sevenz_rust2::ArchiveEntry::from_path(&source, "collection.json".to_string());
        writer
            .push_archive_entry(entry, Some(std::fs::File::open(&source).unwrap()))
            .unwrap();
        writer.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn a_manifest_keeps_the_phase_choices_and_a_bundle() {
        let text = r#"{
          "info": {"author":"A","name":"Example","description":"D","domainName":"examplegame"},
          "mods":[
            {"name":"Bundle","version":"1","optional":false,"domainName":"examplegame",
             "source":{"type":"bundle","fileExpression":"Bundled - Example","fileSize":2048,"tag":"t1"},
             "phase":2},
            {"name":"Fomod","version":"2","optional":true,"domainName":"examplegame",
             "source":{"type":"nexus","modId":7,"fileId":10,"md5":"aa","fileSize":1024,"logicalFilename":"Fomod"},
             "choices":{"type":"fomod","options":[{"name":"Step","groups":[{"name":"Group","choices":[{"name":"One","idx":0},{"name":"Two","idx":1}]}]}]},
             "phase":666}
          ],
          "modRules":[{"type":"before","source":{"modId":7,"fileId":10},"reference":{"tag":"t1"}}]
        }"#;
        let parsed = manifest(text);
        let (dir, path) = archive_with(text, "collection-manifest-order");
        let plan = build(
            parsed,
            &revision(),
            "examplegame",
            &Archive::open(&path).unwrap(),
        )
        .unwrap();
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.items[0].phase, 2);
        assert_eq!(plan.items[1].choices.len(), 2);
        assert_eq!(plan.items[1].choices[0].option, "One");
        assert!(plan.has_bundles);
        assert_eq!(plan.order.len(), 2);
        assert_eq!(plan.order[0], "bundle:t1");
        assert_eq!(plan.order[1], "nexus:7");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_manifest_for_another_game_is_refused() {
        let text = r#"{"info":{"name":"X","domainName":"othergame"},"mods":[]}"#;
        let parsed = manifest(text);
        let (dir, path) = archive_with(text, "collection-manifest-game");
        let error = build(
            parsed,
            &revision(),
            "examplegame",
            &Archive::open(&path).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.code, "collection.game_mismatch");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_large_plain_manifest_is_read_in_rule_order() {
        let mods: Vec<String> = (1..=11)
            .map(|id| {
                format!(
                    r#"{{"name":"Example Mod {id}","version":"1.0","optional":false,"domainName":"examplegame","source":{{"type":"nexus","modId":{id},"fileId":{}}}}}"#,
                    id + 10
                )
            })
            .collect();
        let text = format!(
            r#"{{
              "info": {{"author":"A","name":"Example Collection","description":"D","domainName":"examplegame"}},
              "mods":[{}],
              "modRules":[{{"type":"before","source":{{"modId":2,"fileId":12}},"reference":{{"modId":1,"fileId":11}}}}]
            }}"#,
            mods.join(",")
        );
        let (dir, path) = archive_with(&text, "collection-large-plain");
        let plan = read_plan(&path, &revision(), "examplegame").unwrap();
        assert_eq!(plan.items.len(), 11);
        assert_eq!(plan.order.len(), 11);
        assert_eq!(
            &plan.order[..2],
            ["nexus:1", "nexus:2"],
            "the rule orders the pair"
        );
        assert!(
            plan.items
                .iter()
                .all(|item| !item.optional && item.phase == 0)
        );
        assert!(plan.items.iter().any(|item| item.name == "Example Mod 5"));
        assert!(!plan.limited);
        assert!(!plan.has_bundles);
        assert!(!plan.has_patches);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_manifest_without_an_archive_is_a_limited_plan() {
        let revision = CollectionRevision {
            mods: vec![CollectionMod {
                mod_id: "7".into(),
                file_id: "10".into(),
                optional: false,
                name: "Mod".into(),
                version: Some("1".into()),
                author: None,
                category: None,
                summary: String::new(),
                picture_url: None,
                adult: false,
                file_name: None,
                size_kb: 1,
            }],
            ..revision()
        };
        let plan = plan_from_api(&revision, "examplegame");
        assert!(plan.limited);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.order, ["nexus:7"]);
    }
}
