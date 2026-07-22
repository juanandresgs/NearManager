#![allow(clippy::missing_errors_doc)]

use std::collections::{BTreeMap, BTreeSet};

use near_core::ResourceMetadata;
use near_search::{ResourcePredicate, TextPredicate};
use serde::Deserialize;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterMode {
    Include,
    Exclude,
    ForceInclude,
    ForceExclude,
}

impl FilterMode {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Include => "+",
            Self::Exclude => "-",
            Self::ForceInclude => "I",
            Self::ForceExclude => "X",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedMaskGroup {
    pub id: String,
    pub label: String,
    pub masks: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedFilter {
    pub id: String,
    pub label: String,
    pub mode: FilterMode,
    #[serde(default)]
    pub mask_group: Option<String>,
    #[serde(default)]
    pub predicate: ResourcePredicate,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilterDocument {
    schema: u16,
    #[serde(default)]
    mask_groups: Vec<NamedMaskGroup>,
    #[serde(default)]
    filters: Vec<SavedFilter>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FilterCatalog {
    mask_groups: Vec<NamedMaskGroup>,
    filters: Vec<SavedFilter>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FilterError {
    #[error("invalid filter TOML: {0}")]
    Toml(String),
    #[error("unsupported filter schema {0}")]
    UnsupportedSchema(u16),
    #[error("filter and mask-group IDs and labels cannot be empty")]
    EmptyIdentity,
    #[error("duplicate filter or mask-group ID {0}")]
    DuplicateId(String),
    #[error("mask group {0} has no masks")]
    EmptyMaskGroup(String),
    #[error("filter {filter} references unknown mask group {group}")]
    UnknownMaskGroup { filter: String, group: String },
    #[error("filter {0} cannot combine a mask group with a name predicate")]
    AmbiguousMask(String),
    #[error("filter {0} cannot use a content predicate on a panel")]
    ContentPredicate(String),
    #[error("filter {0} has an invalid predicate: {1}")]
    InvalidPredicate(String, String),
}

impl FilterCatalog {
    pub fn from_toml(source: &str) -> Result<Self, FilterError> {
        let mut document: FilterDocument =
            toml::from_str(source).map_err(|error| FilterError::Toml(error.to_string()))?;
        if document.schema != 1 {
            return Err(FilterError::UnsupportedSchema(document.schema));
        }
        let mut group_ids = BTreeSet::new();
        let mut filter_ids = BTreeSet::new();
        let groups = document
            .mask_groups
            .iter()
            .map(|group| (group.id.clone(), group.masks.clone()))
            .collect::<BTreeMap<_, _>>();
        for group in &document.mask_groups {
            validate_identity(&group.id, &group.label)?;
            if !group_ids.insert(group.id.clone()) {
                return Err(FilterError::DuplicateId(group.id.clone()));
            }
            if group.masks.is_empty() || group.masks.iter().any(|mask| mask.trim().is_empty()) {
                return Err(FilterError::EmptyMaskGroup(group.id.clone()));
            }
        }
        for filter in &mut document.filters {
            validate_identity(&filter.id, &filter.label)?;
            if !filter_ids.insert(filter.id.clone()) {
                return Err(FilterError::DuplicateId(filter.id.clone()));
            }
            filter.predicate.validate().map_err(|error| {
                FilterError::InvalidPredicate(filter.id.clone(), error.to_string())
            })?;
            if filter.predicate.content.is_some() {
                return Err(FilterError::ContentPredicate(filter.id.clone()));
            }
            if let Some(group) = &filter.mask_group {
                if filter.predicate.name.is_some() {
                    return Err(FilterError::AmbiguousMask(filter.id.clone()));
                }
                groups
                    .get(group)
                    .ok_or_else(|| FilterError::UnknownMaskGroup {
                        filter: filter.id.clone(),
                        group: group.clone(),
                    })?;
            }
        }
        Ok(Self {
            mask_groups: document.mask_groups,
            filters: document.filters,
        })
    }

    pub fn filters(&self) -> &[SavedFilter] {
        &self.filters
    }

    pub fn mask_groups(&self) -> &[NamedMaskGroup] {
        &self.mask_groups
    }

    pub fn contains(&self, id: &str) -> bool {
        self.filters.iter().any(|filter| filter.id == id)
    }

    pub fn matches(&self, active: &[String], metadata: &ResourceMetadata) -> bool {
        let active = self
            .filters
            .iter()
            .filter(|filter| active.iter().any(|id| id == &filter.id))
            .collect::<Vec<_>>();
        if active.is_empty() {
            return true;
        }
        if active.iter().any(|filter| {
            filter.mode == FilterMode::ForceExclude && self.filter_matches(filter, metadata)
        }) {
            return false;
        }
        if active.iter().any(|filter| {
            filter.mode == FilterMode::ForceInclude && self.filter_matches(filter, metadata)
        }) {
            return true;
        }
        if active.iter().any(|filter| {
            filter.mode == FilterMode::Exclude && self.filter_matches(filter, metadata)
        }) {
            return false;
        }
        let includes = active
            .iter()
            .filter(|filter| filter.mode == FilterMode::Include)
            .collect::<Vec<_>>();
        includes.is_empty()
            || includes
                .iter()
                .any(|filter| self.filter_matches(filter, metadata))
    }

    fn filter_matches(&self, filter: &SavedFilter, metadata: &ResourceMetadata) -> bool {
        let Some(group) = &filter.mask_group else {
            return filter.predicate.matches_metadata(metadata);
        };
        let Some(group) = self
            .mask_groups
            .iter()
            .find(|candidate| &candidate.id == group)
        else {
            return false;
        };
        group.masks.iter().any(|mask| {
            let mut predicate = filter.predicate.clone();
            predicate.name = Some(TextPredicate::Glob(mask.clone()));
            predicate.matches_metadata(metadata)
        })
    }
}

fn validate_identity(id: &str, label: &str) -> Result<(), FilterError> {
    if id.trim().is_empty() || label.trim().is_empty() {
        Err(FilterError::EmptyIdentity)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use near_core::{PermissionSummary, ResourceKind, ResourceMetadata};

    use super::*;

    #[test]
    fn named_groups_and_priority_modes_compose() {
        let catalog = FilterCatalog::from_toml(
            r#"
schema = 1
[[mask_groups]]
id = "source"
label = "Source"
masks = ["*.rs", "*.toml"]

[[filters]]
id = "source"
label = "Source files"
mode = "include"
mask_group = "source"

[[filters]]
id = "hidden"
label = "Hide hidden"
mode = "exclude"
[filters.predicate]
schema_version = 1
hidden = "only"

[[filters]]
id = "executable"
label = "Always show executable"
mode = "force-include"
[filters.predicate]
schema_version = 1
executable = true
hidden = "include"
"#,
        )
        .unwrap();
        let active = vec![
            "source".to_owned(),
            "hidden".to_owned(),
            "executable".to_owned(),
        ];
        let source = ResourceMetadata {
            name: "main.rs".to_owned(),
            kind: ResourceKind::File,
            hidden: Some(false),
            ..ResourceMetadata::default()
        };
        assert!(catalog.matches(&active, &source));
        let executable = ResourceMetadata {
            name: ".tool".to_owned(),
            kind: ResourceKind::File,
            hidden: Some(true),
            permissions: Some(PermissionSummary {
                unix_mode: Some(0o755),
                readonly: false,
                executable: true,
            }),
            ..ResourceMetadata::default()
        };
        assert!(catalog.matches(&active, &executable));
    }

    #[test]
    fn size_date_and_attribute_predicates_compose() {
        let catalog = FilterCatalog::from_toml(
            r#"
schema = 1

[[filters]]
id = "deployable"
label = "Deployable files"
mode = "include"

[filters.predicate]
schema_version = 1
minimum_size = 1024
maximum_size = 4096
modified_after_unix_ms = 1000
modified_before_unix_ms = 3000
readonly = false
executable = true
hidden = "include"
ignore = "none"
"#,
        )
        .unwrap();
        let active = vec!["deployable".to_owned()];
        let matching = ResourceMetadata {
            name: "release-tool".to_owned(),
            kind: ResourceKind::File,
            size: Some(2048),
            modified_unix_ms: Some(2000),
            permissions: Some(PermissionSummary {
                unix_mode: Some(0o755),
                readonly: false,
                executable: true,
            }),
            ..ResourceMetadata::default()
        };
        assert!(catalog.matches(&active, &matching));
        assert!(!catalog.matches(
            &active,
            &ResourceMetadata {
                size: Some(512),
                ..matching
            }
        ));
    }
}
