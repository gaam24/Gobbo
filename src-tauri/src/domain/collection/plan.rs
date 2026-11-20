use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::Message;

use super::info::CollectionInfo;
use super::item::CollectionItem;

/// The plan format written now. A newer format is not read; the collection is fetched again instead.
pub const PLAN_FORMAT: u32 = 1;

/// One rule between two collection members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionRule {
    pub kind: CollectionRuleKind,
    /// The item key of the source.
    pub source: String,
    /// The item key of the reference.
    pub reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CollectionRuleKind {
    Before,
    After,
    Requires,
    Conflicts,
    Recommends,
    Provides,
}

/// Something the user should know before or during the install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionIssue {
    pub severity: CollectionIssueSeverity,
    /// The member the issue belongs to, when it belongs to one.
    pub item: Option<String>,
    pub message: Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CollectionIssueSeverity {
    Warning,
    Unsupported,
}

/// The full recipe of one collection revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionPlan {
    pub format: u32,
    pub info: CollectionInfo,
    pub items: Vec<CollectionItem>,
    pub rules: Vec<CollectionRule>,
    pub issues: Vec<CollectionIssue>,
    /// Item keys in the order they should enter the profile (first wins a file conflict).
    pub order: Vec<String>,
    pub has_bundles: bool,
    pub has_patches: bool,
    /// The plan came from the API metadata only: the archive was not read, so choices and rules are missing.
    pub limited: bool,
}

impl CollectionPlan {
    /// The item with `key`.
    pub fn item(&self, key: &str) -> Option<&CollectionItem> {
        self.items.iter().find(|item| item.key == key)
    }

    /// The items in install/list order. Keys the order does not name are appended in their plan order, so a damaged
    /// cache still lists every member.
    pub fn ordered(&self) -> Vec<&CollectionItem> {
        let mut seen = BTreeSet::new();
        let mut ordered = Vec::with_capacity(self.items.len());

        for key in &self.order {
            if let Some(item) = self.item(key)
                && seen.insert(key.as_str())
            {
                ordered.push(item);
            }
        }

        for item in &self.items {
            if seen.insert(item.key.as_str()) {
                ordered.push(item);
            }
        }

        ordered
    }

    /// Whether the plan names `key`.
    pub fn has_item(&self, key: &str) -> bool {
        self.items.iter().any(|item| item.key == key)
    }
}

/// The order the profile list uses: first wins a file conflict, so a rule that makes `source` load before `reference`
/// in Vortex puts `reference` first here.
pub fn order_items(
    items: &[CollectionItem],
    rules: &[CollectionRule],
) -> (Vec<String>, Vec<Message>) {
    let mut issues = Vec::new();
    let index: BTreeMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(at, item)| (item.key.as_str(), at))
        .collect();
    let mut ordered = Vec::with_capacity(items.len());
    let phases: BTreeSet<u32> = items.iter().map(|item| item.phase).collect();

    for phase in phases {
        let members: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.phase == phase)
            .map(|(at, _)| at)
            .collect();
        ordered.extend(order_phase(items, rules, &index, &members, &mut issues));
    }

    (ordered, issues)
}

fn order_phase(
    items: &[CollectionItem],
    rules: &[CollectionRule],
    index: &BTreeMap<&str, usize>,
    members: &[usize],
    issues: &mut Vec<Message>,
) -> Vec<String> {
    let member_set: BTreeSet<usize> = members.iter().copied().collect();
    let mut edges: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut incoming: BTreeMap<usize, usize> = members.iter().map(|&at| (at, 0)).collect();

    for rule in rules {
        let (Some(&source), Some(&reference)) = (
            index.get(rule.source.as_str()),
            index.get(rule.reference.as_str()),
        ) else {
            continue;
        };

        if !member_set.contains(&source) || !member_set.contains(&reference) {
            // Phase gating wins; an edge across phases would fight the order the curator chose.
            continue;
        }

        let (before, after) = match rule.kind {
            CollectionRuleKind::Before => (reference, source),
            CollectionRuleKind::After => (source, reference),
            _ => continue,
        };

        if before == after || !edges.entry(before).or_default().insert(after) {
            continue;
        }

        *incoming.entry(after).or_default() += 1;
    }

    let mut ready: BTreeSet<usize> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(&at, _)| at)
        .collect();
    let mut ordered = Vec::with_capacity(members.len());

    while let Some(at) = ready.pop_first() {
        ordered.push(items[at].key.clone());

        if let Some(next) = edges.get(&at) {
            for &after in next {
                if let Some(count) = incoming.get_mut(&after) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        ready.insert(after);
                    }
                }
            }
        }
    }

    if ordered.len() != members.len() {
        issues.push(Message::new("collection.rule_cycle"));
        let emitted: BTreeSet<String> = ordered.iter().cloned().collect();

        for &at in members {
            if !emitted.contains(&items[at].key) {
                ordered.push(items[at].key.clone());
            }
        }
    }

    ordered
}

#[cfg(test)]
mod tests {
    use crate::domain::collection::{
        CollectionItem, CollectionSource, CollectionUpdatePolicy, OPTIONAL_PHASE,
    };

    use super::*;

    fn item(key: &str, phase: u32) -> CollectionItem {
        CollectionItem {
            key: key.to_string(),
            name: key.to_string(),
            version: None,
            author: None,
            category: None,
            summary: String::new(),
            picture_url: None,
            optional: phase == OPTIONAL_PHASE,
            phase,
            source: CollectionSource::Manual,
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: String::new(),
            adult: false,
        }
    }

    fn rule(kind: CollectionRuleKind, source: &str, reference: &str) -> CollectionRule {
        CollectionRule {
            kind,
            source: source.to_string(),
            reference: reference.to_string(),
        }
    }

    #[test]
    fn a_before_rule_puts_the_reference_first_because_first_wins() {
        let items = [item("a", 0), item("b", 0)];
        let (order, issues) = order_items(&items, &[rule(CollectionRuleKind::Before, "a", "b")]);
        assert_eq!(order, ["b", "a"]);
        assert!(issues.is_empty());
    }

    #[test]
    fn an_after_rule_puts_the_source_first() {
        let items = [item("a", 0), item("b", 0)];
        let (order, _) = order_items(&items, &[rule(CollectionRuleKind::After, "a", "b")]);
        assert_eq!(order, ["a", "b"]);
    }

    #[test]
    fn phases_win_over_rules_and_optional_members_go_last() {
        let items = [
            item("core", 0),
            item("content", 2),
            item("optional", OPTIONAL_PHASE),
        ];
        let (order, _) = order_items(
            &items,
            &[rule(CollectionRuleKind::Before, "content", "core")],
        );
        assert_eq!(order, ["core", "content", "optional"]);
    }

    #[test]
    fn a_cycle_keeps_every_member_and_reports_it() {
        let items = [item("a", 0), item("b", 0), item("c", 0)];
        let (order, issues) = order_items(
            &items,
            &[
                rule(CollectionRuleKind::Before, "a", "b"),
                rule(CollectionRuleKind::Before, "b", "a"),
            ],
        );
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], "c");
        assert_eq!(issues[0].code, "collection.rule_cycle");
    }
}
