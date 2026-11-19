//! The parts of a mod page that v3 does not expose: its summary, its creation and update dates, and the requirements
//! the mod lists for itself. One aliased query asks for a whole chunk of mods at once.

use super::client::NexusClient;
use super::models::{Envelope, unix_time};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt::Write;

/// How many mods one GraphQL query names. The query is aliases, so a smaller chunk keeps it well below the complexity
/// limit and still removes almost all per-mod requests.
const FACET_BATCH: usize = 20;

/// How many own requirements one alias asks for. Nexus mods rarely list more than a handful; the cap keeps the
/// aliased query cheap.
const FACET_REQUIREMENTS: usize = 50;

/// What the GraphQL API adds to a v3 mod page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModFacet {
    pub summary: Option<String>,
    pub created_at: Option<u32>,
    pub updated_at: Option<u32>,
    /// The requirements the mod lists for itself, before they are merged with its newest file's.
    pub requirements: Vec<FacetRequirement>,
}

/// One requirement as GraphQL names it: a game-scoped mod id and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacetRequirement {
    pub mod_id: u32,
    pub name: String,
}

/// The facets of every mod that answered. A chunk that fails is left out; the caller keeps the values it already
/// has and still reads the file requirements from v3.
pub fn facets(client: &NexusClient, game_id: u32, mod_ids: &[u32]) -> BTreeMap<u32, ModFacet> {
    let mut found = BTreeMap::new();
    for chunk in mod_ids.chunks(FACET_BATCH) {
        let mut query = String::from("{");
        for (index, mod_id) in chunk.iter().enumerate() {
            // The values are u32, so they are safe in the query text.
            let _ = write!(
                query,
                "m{index}: mod(modId: {mod_id}, gameId: {game_id}) {{ modId summary createdAt updatedAt \
                 modRequirements {{ nexusRequirements(count: {FACET_REQUIREMENTS}) {{ nodes {{ modId modName }} }} }} }}"
            );
        }
        query.push('}');
        let body = json!({ "query": query }).to_string();
        let Ok(text) = client.post_graphql(&body) else {
            continue;
        };
        found.extend(parse_facets(&text, chunk));
    }
    found
}

/// Reads the aliased answer. A missing alias or a `null` node means "no data", not a failure of the whole check.
fn parse_facets(text: &str, ids: &[u32]) -> BTreeMap<u32, ModFacet> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Node {
        #[serde(default)]
        mod_id: Option<Id>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        created_at: Option<String>,
        #[serde(default)]
        updated_at: Option<String>,
        #[serde(default)]
        mod_requirements: Option<Requirements>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Requirements {
        #[serde(default)]
        nexus_requirements: Option<Page>,
    }
    #[derive(Deserialize)]
    struct Page {
        #[serde(default)]
        nodes: Vec<RequirementNode>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RequirementNode {
        #[serde(default)]
        mod_id: Option<Id>,
        #[serde(default)]
        mod_name: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Number(u32),
        Text(String),
    }
    impl Id {
        fn number(&self) -> Option<u32> {
            match self {
                Self::Number(id) => Some(*id),
                Self::Text(text) => text.parse().ok(),
            }
        }
    }

    let response: Envelope<Option<BTreeMap<String, Option<Node>>>> =
        match serde_json::from_str(text) {
            Ok(response) => response,
            Err(_) => return BTreeMap::new(),
        };
    let mut found = BTreeMap::new();
    for (alias, node) in response.data.unwrap_or_default() {
        let Some(node) = node else {
            continue;
        };
        let index = alias
            .strip_prefix('m')
            .and_then(|index| index.parse::<usize>().ok());
        let mod_id = node
            .mod_id
            .as_ref()
            .and_then(Id::number)
            .or_else(|| index.and_then(|index| ids.get(index).copied()));
        let Some(mod_id) = mod_id else {
            continue;
        };
        let summary = node
            .summary
            .map(|summary| summary.trim().to_string())
            .filter(|summary| !summary.is_empty());
        let mut requirements: Vec<FacetRequirement> = Vec::new();
        for requirement in node
            .mod_requirements
            .and_then(|requirements| requirements.nexus_requirements)
            .map_or_else(Vec::new, |page| page.nodes)
        {
            let Some(mod_id) = requirement.mod_id.as_ref().and_then(Id::number) else {
                continue;
            };
            let Some(name) = requirement.mod_name.filter(|name| !name.trim().is_empty()) else {
                continue;
            };
            if !requirements.iter().any(|known| known.mod_id == mod_id) {
                requirements.push(FacetRequirement { mod_id, name });
            }
        }
        found.insert(
            mod_id,
            ModFacet {
                summary,
                created_at: node.created_at.as_deref().and_then(unix_time),
                updated_at: node.updated_at.as_deref().and_then(unix_time),
                requirements,
            },
        );
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_gives_the_summary_dates_and_own_requirements() {
        let text = r#"{"data":{
            "m0":{"modId":107,"summary":"  Scripting  ","createdAt":"2020-12-12T22:39:34Z",
                  "updatedAt":"2026-10-26T01:28:58Z",
                  "modRequirements":{"nexusRequirements":{"nodes":[
                      {"modId":"2380","modName":"RED4ext"},
                      {"modId":2380,"modName":"RED4ext"},
                      {"modId":"","modName":"Broken"}
                  ]}}},
            "m1":{"modId":2380,"summary":"","createdAt":null,"updatedAt":"not a date",
                  "modRequirements":null},
            "m2":null
        }}"#;
        let found = parse_facets(text, &[107, 2380, 6880]);
        assert_eq!(found[&107].summary.as_deref(), Some("Scripting"));
        assert_eq!(found[&107].created_at, Some(1_607_812_774));
        assert_eq!(found[&107].updated_at, Some(1_792_978_138));
        assert_eq!(
            found[&107].requirements,
            [FacetRequirement {
                mod_id: 2380,
                name: "RED4ext".into()
            }]
        );
        assert_eq!(found[&2380], ModFacet::default());
        assert!(!found.contains_key(&6880));
    }

    #[test]
    fn a_broken_answer_keeps_the_old_values() {
        assert!(parse_facets(r#"{"errors":[{"message":"x"}]}"#, &[1]).is_empty());
        assert!(parse_facets(r#"{"data":null}"#, &[1]).is_empty());
    }
}
