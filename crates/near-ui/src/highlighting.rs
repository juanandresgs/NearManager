#![allow(clippy::assigning_clones, clippy::missing_errors_doc)]

use std::collections::{BTreeMap, BTreeSet};

use near_core::ResourceMetadata;
use near_search::ResourcePredicate;
use serde::Deserialize;
use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileDecoration {
    pub role: Option<String>,
    pub mark: Option<String>,
    pub sort_group: i32,
    pub rule_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
struct HighlightingDocument {
    schema: u32,
    #[serde(default)]
    rules: Vec<HighlightingRule>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
struct HighlightingRule {
    id: String,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    predicate: ResourcePredicate,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    mark: Option<String>,
    #[serde(default)]
    sort_group: Option<i32>,
}

const fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectiveRule {
    id: String,
    priority: i32,
    predicates: Vec<ResourcePredicate>,
    role: Option<String>,
    mark: Option<String>,
    sort_group: i32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightingCatalog {
    rules: Vec<EffectiveRule>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HighlightingError {
    #[error("invalid highlighting TOML: {0}")]
    Toml(String),
    #[error("unsupported highlighting schema {0}")]
    UnsupportedSchema(u32),
    #[error("highlighting rule IDs cannot be empty")]
    EmptyId,
    #[error("duplicate highlighting rule ID {0}")]
    DuplicateId(String),
    #[error("highlighting rule {rule} inherits unknown rule {parent}")]
    UnknownParent { rule: String, parent: String },
    #[error("highlighting inheritance cycle: {0}")]
    InheritanceCycle(String),
    #[error("highlighting rule {0} has no role mark or sort group")]
    EmptyDecoration(String),
    #[error("highlighting rule {0} has an empty semantic role")]
    EmptyRole(String),
    #[error("highlighting rule {0} mark must contain exactly one character")]
    InvalidMark(String),
    #[error("highlighting rule {0} cannot use a content predicate")]
    ContentPredicate(String),
    #[error("highlighting rule {rule} has an invalid predicate: {message}")]
    InvalidPredicate { rule: String, message: String },
}

impl HighlightingCatalog {
    pub fn from_toml(source: &str) -> Result<Self, HighlightingError> {
        let document: HighlightingDocument =
            toml::from_str(source).map_err(|error| HighlightingError::Toml(error.to_string()))?;
        if document.schema != 1 {
            return Err(HighlightingError::UnsupportedSchema(document.schema));
        }
        let mut rules = BTreeMap::new();
        for rule in document.rules {
            if rule.id.trim().is_empty() {
                return Err(HighlightingError::EmptyId);
            }
            if rule.predicate.content.is_some() {
                return Err(HighlightingError::ContentPredicate(rule.id));
            }
            if rule
                .role
                .as_ref()
                .is_some_and(|role| role.trim().is_empty())
            {
                return Err(HighlightingError::EmptyRole(rule.id));
            }
            if rule
                .mark
                .as_ref()
                .is_some_and(|mark| mark.chars().count() != 1)
            {
                return Err(HighlightingError::InvalidMark(rule.id));
            }
            rule.predicate
                .validate()
                .map_err(|error| HighlightingError::InvalidPredicate {
                    rule: rule.id.clone(),
                    message: error.to_string(),
                })?;
            let id = rule.id.clone();
            if rules.insert(id.clone(), rule).is_some() {
                return Err(HighlightingError::DuplicateId(id));
            }
        }
        let mut effective = Vec::new();
        for id in rules.keys() {
            let mut visiting = BTreeSet::new();
            if let Some(rule) = resolve_rule(id, &rules, &mut visiting)?
                && (rule.role.is_some() || rule.mark.is_some() || rule.sort_group != 0)
            {
                effective.push(rule);
            }
        }
        effective.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(Self { rules: effective })
    }

    pub fn decoration(&self, metadata: &ResourceMetadata) -> FileDecoration {
        self.rules
            .iter()
            .find(|rule| {
                rule.predicates
                    .iter()
                    .all(|predicate| predicate.matches_metadata(metadata))
            })
            .map_or_else(FileDecoration::default, |rule| FileDecoration {
                role: rule.role.clone(),
                mark: rule.mark.clone(),
                sort_group: rule.sort_group,
                rule_id: Some(rule.id.clone()),
            })
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn report(&self) -> String {
        if self.rules.is_empty() {
            return "No highlighting rules are active".to_owned();
        }
        self.rules
            .iter()
            .map(|rule| {
                format!(
                    "{}  priority={}  predicates={}  role={}  mark={}  sort-group={}",
                    rule.id,
                    rule.priority,
                    rule.predicates.len(),
                    rule.role.as_deref().unwrap_or("-"),
                    rule.mark.as_deref().unwrap_or("-"),
                    rule.sort_group
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn resolve_rule(
    id: &str,
    rules: &BTreeMap<String, HighlightingRule>,
    visiting: &mut BTreeSet<String>,
) -> Result<Option<EffectiveRule>, HighlightingError> {
    let rule = rules.get(id).expect("rule IDs originate from the same map");
    if !rule.enabled {
        return Ok(None);
    }
    if !visiting.insert(id.to_owned()) {
        return Err(HighlightingError::InheritanceCycle(
            visiting.iter().cloned().collect::<Vec<_>>().join(" -> "),
        ));
    }
    let parent = if let Some(parent) = &rule.parent {
        if !rules.contains_key(parent) {
            return Err(HighlightingError::UnknownParent {
                rule: id.to_owned(),
                parent: parent.clone(),
            });
        }
        resolve_rule(parent, rules, visiting)?
    } else {
        None
    };
    visiting.remove(id);
    let mut effective = parent.unwrap_or_else(|| EffectiveRule {
        id: id.to_owned(),
        priority: rule.priority,
        predicates: Vec::new(),
        role: None,
        mark: None,
        sort_group: 0,
    });
    effective.id = id.to_owned();
    effective.priority = rule.priority;
    effective.predicates.push(rule.predicate.clone());
    if let Some(role) = &rule.role {
        effective.role = Some(role.clone());
    }
    if let Some(mark) = &rule.mark {
        effective.mark = Some(mark.clone());
    }
    if let Some(sort_group) = rule.sort_group {
        effective.sort_group = sort_group;
    }
    if effective.role.is_none() && effective.mark.is_none() && effective.sort_group == 0 {
        return Err(HighlightingError::EmptyDecoration(id.to_owned()));
    }
    Ok(Some(effective))
}

#[cfg(test)]
mod tests {
    use near_core::{PermissionSummary, ResourceKind, ResourceMetadata};

    use super::{HighlightingCatalog, HighlightingError};

    #[test]
    fn priority_inheritance_and_attributes_resolve_one_effective_decoration() {
        let catalog = HighlightingCatalog::from_toml(
            r#"
schema = 1

[[rules]]
id = "source"
priority = 10
role = "highlight.source"
mark = "S"
sort_group = 20
[rules.predicate]
schema_version = 1
name = { match = "glob", value = "*.rs" }
hidden = "include"
ignore = "none"

[[rules]]
id = "readonly-source"
parent = "source"
priority = 30
role = "highlight.readonly"
[rules.predicate]
schema_version = 1
readonly = true
hidden = "include"
ignore = "none"
"#,
        )
        .unwrap();
        let metadata = ResourceMetadata {
            name: "main.rs".to_owned(),
            kind: ResourceKind::File,
            permissions: Some(PermissionSummary {
                unix_mode: Some(0o444),
                readonly: true,
                executable: false,
            }),
            ..ResourceMetadata::default()
        };

        let decoration = catalog.decoration(&metadata);
        assert_eq!(decoration.role.as_deref(), Some("highlight.readonly"));
        assert_eq!(decoration.mark.as_deref(), Some("S"));
        assert_eq!(decoration.sort_group, 20);
        assert_eq!(decoration.rule_id.as_deref(), Some("readonly-source"));
    }

    #[test]
    fn invalid_parents_cycles_and_empty_rules_fail_closed() {
        assert!(matches!(
            HighlightingCatalog::from_toml(
                "schema = 1\n[[rules]]\nid = \"child\"\nparent = \"missing\"\nrole = \"x\"\n"
            ),
            Err(HighlightingError::UnknownParent { .. })
        ));
        assert!(matches!(
            HighlightingCatalog::from_toml(
                "schema = 1\n[[rules]]\nid = \"a\"\nparent = \"b\"\nrole = \"x\"\n[[rules]]\nid = \"b\"\nparent = \"a\"\nmark = \"x\"\n"
            ),
            Err(HighlightingError::InheritanceCycle(_))
        ));
        assert!(matches!(
            HighlightingCatalog::from_toml("schema = 1\n[[rules]]\nid = \"empty\"\n"),
            Err(HighlightingError::EmptyDecoration(_))
        ));
        assert!(matches!(
            HighlightingCatalog::from_toml("schema = 1\n[[rules]]\nid = \"wide\"\nmark = \"XX\"\n"),
            Err(HighlightingError::InvalidMark(_))
        ));
    }

    #[test]
    fn shipped_highlighting_catalog_is_valid() {
        let catalog =
            HighlightingCatalog::from_toml(include_str!("../../../specs/highlighting.toml"))
                .unwrap();
        assert!(catalog.len() >= 6);
    }
}
