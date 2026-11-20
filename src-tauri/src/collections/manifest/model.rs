use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::domain::collection::{CollectionIssue, CollectionIssueSeverity};
use crate::error::Message;

pub(super) fn issue<const N: usize>(
    severity: CollectionIssueSeverity,
    item: Option<String>,
    code: &str,
    params: [(&str, String); N],
) -> CollectionIssue {
    let mut message = Message::new(code);

    for (key, value) in params {
        message = message.with(key, value);
    }

    CollectionIssue {
        severity,
        item,
        message,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Manifest {
    pub(super) info: Info,
    #[serde(default)]
    pub(super) mods: Vec<Mod>,
    #[serde(default)]
    pub(super) mod_rules: Vec<Rule>,
    #[serde(default)]
    pub(super) load_order: Value,
    #[serde(default)]
    pub(super) tools: Value,
    #[serde(default)]
    pub(super) plugins: Value,
    #[serde(default)]
    pub(super) plugin_rules: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Info {
    #[serde(default)]
    pub(super) author: Option<String>,
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "part of the manifest format; the author page is not shown yet"
    )]
    pub(super) author_url: Option<String>,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: String,
    #[serde(default)]
    pub(super) install_instructions: String,
    #[serde(default)]
    pub(super) domain_name: Option<String>,
    #[serde(default)]
    pub(super) game_versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Mod {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) optional: bool,
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "part of the manifest format; the game is checked at the plan level"
    )]
    pub(super) domain_name: Option<String>,
    pub(super) source: Source,
    #[serde(default)]
    pub(super) choices: Option<Choices>,
    #[serde(default)]
    pub(super) patches: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) instructions: Option<String>,
    #[serde(default)]
    pub(super) author: Option<String>,
    #[serde(default)]
    pub(super) details: Option<Details>,
    #[serde(default)]
    pub(super) phase: Option<u32>,
    #[serde(default)]
    pub(super) file_overrides: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Source {
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) mod_id: Option<u32>,
    #[serde(default)]
    pub(super) file_id: Option<u32>,
    #[serde(default)]
    pub(super) url: Option<String>,
    #[serde(default)]
    pub(super) file_expression: Option<String>,
    #[serde(default)]
    pub(super) md5: Option<String>,
    #[serde(default)]
    pub(super) file_size: Option<u64>,
    #[serde(default)]
    pub(super) logical_filename: Option<String>,
    #[serde(default)]
    pub(super) update_policy: Option<String>,
    #[serde(default)]
    pub(super) tag: Option<String>,
    #[serde(default)]
    pub(super) adult_content: Option<bool>,
    #[serde(default)]
    pub(super) instructions: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Details {
    #[serde(default)]
    pub(super) category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Choices {
    #[serde(rename = "type", default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) options: Vec<ChoiceStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChoiceStep {
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "the step name is part of the format and kept for a future wizard prefill"
    )]
    pub(super) name: String,
    #[serde(default)]
    pub(super) groups: Vec<ChoiceGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChoiceGroup {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) choices: Vec<ChoiceOption>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChoiceOption {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "the option index is part of the manifest format; choices match by name"
    )]
    pub(super) idx: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Rule {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) source: Reference,
    pub(super) reference: Reference,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Reference {
    #[serde(default)]
    pub(super) mod_id: Option<u32>,
    #[serde(default)]
    pub(super) file_id: Option<u32>,
    #[serde(default)]
    pub(super) file_expression: Option<String>,
    #[serde(default)]
    pub(super) file_md5: Option<String>,
    #[serde(default)]
    pub(super) logical_file_name: Option<String>,
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "part of the manifest rule format; matching uses the file fields"
    )]
    pub(super) version_match: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) tag: Option<String>,
}
