use std::collections::{BTreeMap, BTreeSet};

use crate::domain::collection::{
    CollectionIssue, CollectionIssueSeverity, CollectionItem, CollectionRule, CollectionRuleKind,
    CollectionSource,
};

use super::model::{Reference, Rule, issue};

pub(super) fn unique_key(base: String, suffix: String, seen: &mut BTreeSet<String>) -> String {
    // The first variant is the base, the second adds the suffix, and the rest number it from 2. An empty suffix makes
    // the second variant the base again, which is taken and so moves on to `base#:2`.
    crate::domain::naming::first_free(
        |number| match number {
            1 => base.clone(),
            2 if suffix.is_empty() => base.clone(),
            2 => format!("{base}#{suffix}"),
            _ => format!("{base}#{suffix}:{}", number - 1),
        },
        |candidate| seen.insert(candidate.to_string()),
    )
}

pub(super) fn manifest_rules(
    rules: &[Rule],
    items: &[CollectionItem],
    issues: &mut Vec<CollectionIssue>,
) -> Vec<CollectionRule> {
    let matcher = ReferenceMatcher::new(items);
    let mut out = Vec::new();

    for rule in rules {
        let Some(kind) = rule_kind(&rule.kind) else {
            issues.push(issue(
                CollectionIssueSeverity::Warning,
                None,
                "collection.rule_unknown",
                [("kind", rule.kind.clone())],
            ));
            continue;
        };
        let (Some(source), Some(reference)) =
            (matcher.find(&rule.source), matcher.find(&rule.reference))
        else {
            issues.push(issue(
                CollectionIssueSeverity::Warning,
                None,
                "collection.rule_unmapped",
                [("kind", rule.kind.clone())],
            ));
            continue;
        };
        out.push(CollectionRule {
            kind,
            source: source.to_string(),
            reference: reference.to_string(),
        });
    }

    out
}

fn rule_kind(text: &str) -> Option<CollectionRuleKind> {
    match text {
        "before" => Some(CollectionRuleKind::Before),
        "after" => Some(CollectionRuleKind::After),
        "requires" => Some(CollectionRuleKind::Requires),
        "conflicts" => Some(CollectionRuleKind::Conflicts),
        "recommends" => Some(CollectionRuleKind::Recommends),
        "provides" => Some(CollectionRuleKind::Provides),
        _ => None,
    }
}

struct ReferenceMatcher<'a> {
    by_file: BTreeMap<(String, String), &'a str>,
    by_md5: BTreeMap<String, &'a str>,
    by_logical: BTreeMap<String, &'a str>,
    by_name: BTreeMap<String, &'a str>,
}

impl<'a> ReferenceMatcher<'a> {
    fn new(items: &'a [CollectionItem]) -> Self {
        let mut by_file = BTreeMap::new();
        let mut by_md5 = BTreeMap::new();
        let mut by_logical = BTreeMap::new();
        let mut by_name = BTreeMap::new();

        for item in items {
            if let Some((_, mod_id, file_id)) = item.provider_file() {
                by_file.insert((mod_id.to_string(), file_id.to_string()), item.key.as_str());
            }

            if let Some(md5) = &item.md5 {
                by_md5
                    .entry(md5.to_ascii_lowercase())
                    .or_insert(item.key.as_str());
            }

            if let Some(name) = &item.file_name {
                by_logical
                    .entry(name.to_ascii_lowercase())
                    .or_insert(item.key.as_str());
            }

            by_name
                .entry(item.name.to_ascii_lowercase())
                .or_insert(item.key.as_str());
            if let CollectionSource::Bundle { path, .. } = &item.source {
                by_logical
                    .entry(path.to_ascii_lowercase())
                    .or_insert(item.key.as_str());
            }
        }

        Self {
            by_file,
            by_md5,
            by_logical,
            by_name,
        }
    }

    /// Resolves a collection rule's textual reference to an item key, through this manifest's own lowercased md5,
    /// logical-file-name/path and name keys. It only reads the strings written in the collection; it is not a library
    /// match — `crate::domain::library::LibraryView::find` is the one rule for resolving a wanted mod against families.
    fn find(&self, reference: &Reference) -> Option<&'a str> {
        if let (Some(mod_id), Some(file_id)) = (reference.mod_id, reference.file_id)
            && let Some(key) = self.by_file.get(&(mod_id.to_string(), file_id.to_string()))
        {
            return Some(*key);
        }

        if let Some(md5) = &reference.file_md5
            && let Some(key) = self.by_md5.get(&md5.to_ascii_lowercase())
        {
            return Some(*key);
        }

        if let Some(name) = &reference.logical_file_name
            && let Some(key) = self.by_logical.get(&name.to_ascii_lowercase())
        {
            return Some(*key);
        }

        if let Some(expression) = &reference.file_expression
            && let Some(key) = self.by_logical.get(&expression.to_ascii_lowercase())
        {
            return Some(*key);
        }

        if let Some(description) = &reference.description
            && let Some(key) = self.by_name.get(&description.to_ascii_lowercase())
        {
            return Some(*key);
        }

        if let Some(tag) = &reference.tag
            && let Some(key) = self.by_name.get(&tag.to_ascii_lowercase())
        {
            return Some(*key);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_keys_follow_the_suffix_then_number_sequence() {
        let mut seen = BTreeSet::new();
        assert_eq!(unique_key("base".into(), "tag".into(), &mut seen), "base");
        assert_eq!(
            unique_key("base".into(), "tag".into(), &mut seen),
            "base#tag"
        );
        assert_eq!(
            unique_key("base".into(), "tag".into(), &mut seen),
            "base#tag:2"
        );
        assert_eq!(
            unique_key("base".into(), "tag".into(), &mut seen),
            "base#tag:3"
        );

        let mut seen = BTreeSet::new();
        assert_eq!(unique_key("base".into(), String::new(), &mut seen), "base");
        assert_eq!(
            unique_key("base".into(), String::new(), &mut seen),
            "base#:2"
        );
        assert_eq!(
            unique_key("base".into(), String::new(), &mut seen),
            "base#:3"
        );
    }
}
