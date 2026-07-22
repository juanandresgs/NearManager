use std::{collections::BTreeSet, fmt::Write as _};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelColumnKind {
    #[default]
    Name,
    Extension,
    Size,
    Modified,
    Created,
    Accessed,
    Kind,
    Owner,
    Permissions,
    Description,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColumnAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PanelColumn {
    pub kind: PanelColumnKind,
    #[serde(default)]
    pub width: Option<u16>,
    #[serde(default)]
    pub alignment: ColumnAlignment,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PanelViewMode {
    pub id: String,
    pub label: String,
    pub columns: Vec<PanelColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PanelModeDefaults {
    #[serde(default = "default_medium")]
    pub left: String,
    #[serde(default = "default_medium")]
    pub right: String,
}

impl Default for PanelModeDefaults {
    fn default() -> Self {
        Self {
            left: default_medium(),
            right: default_medium(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PanelModesDocument {
    pub schema: u32,
    #[serde(default)]
    pub defaults: PanelModeDefaults,
    #[serde(default)]
    pub modes: Vec<PanelViewMode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanelModeCatalog {
    modes: Vec<PanelViewMode>,
    left: String,
    right: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PanelModeError {
    #[error("unsupported panel-mode schema {0}")]
    UnsupportedSchema(u32),
    #[error("panel mode ID cannot be empty")]
    EmptyId,
    #[error("panel mode {0} has no columns")]
    EmptyColumns(String),
    #[error("duplicate panel mode ID {0}")]
    DuplicateId(String),
    #[error("unknown default panel mode {0}")]
    UnknownDefault(String),
    #[error("panel-mode document is invalid: {0}")]
    Parse(String),
}

impl PanelModeCatalog {
    pub fn built_in() -> Self {
        Self {
            modes: built_in_modes(),
            left: default_medium(),
            right: default_medium(),
        }
    }

    /// Parses and validates a layered panel-mode document.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas, duplicate or empty definitions, and unknown
    /// default mode IDs.
    pub fn from_toml(source: &str) -> Result<Self, PanelModeError> {
        let document: PanelModesDocument =
            toml::from_str(source).map_err(|error| PanelModeError::Parse(error.to_string()))?;
        if document.schema != 1 {
            return Err(PanelModeError::UnsupportedSchema(document.schema));
        }
        let mut seen = BTreeSet::new();
        for mode in &document.modes {
            if mode.id.trim().is_empty() {
                return Err(PanelModeError::EmptyId);
            }
            if mode.columns.is_empty() {
                return Err(PanelModeError::EmptyColumns(mode.id.clone()));
            }
            if !seen.insert(mode.id.clone()) {
                return Err(PanelModeError::DuplicateId(mode.id.clone()));
            }
        }
        let mut modes = built_in_modes();
        for mode in document.modes {
            if let Some(existing) = modes.iter_mut().find(|existing| existing.id == mode.id) {
                *existing = mode;
            } else {
                modes.push(mode);
            }
        }
        modes.sort_by_key(|mode| canonical_mode_rank(&mode.id));
        for mode in [&document.defaults.left, &document.defaults.right] {
            if !modes.iter().any(|candidate| &candidate.id == mode) {
                return Err(PanelModeError::UnknownDefault(mode.clone()));
            }
        }
        Ok(Self {
            modes,
            left: document.defaults.left,
            right: document.defaults.right,
        })
    }

    pub fn modes(&self) -> &[PanelViewMode] {
        &self.modes
    }

    pub fn mode(&self, id: &str) -> Option<&PanelViewMode> {
        self.modes.iter().find(|mode| mode.id == id)
    }

    pub fn left_default(&self) -> &str {
        &self.left
    }

    pub fn right_default(&self) -> &str {
        &self.right
    }

    /// Changes both panel defaults after validating their mode IDs.
    ///
    /// # Errors
    /// Returns [`PanelModeError::UnknownDefault`] when either ID is absent.
    pub fn set_defaults(&mut self, left: &str, right: &str) -> Result<(), PanelModeError> {
        for mode in [left, right] {
            if self.mode(mode).is_none() {
                return Err(PanelModeError::UnknownDefault(mode.to_owned()));
            }
        }
        left.clone_into(&mut self.left);
        right.clone_into(&mut self.right);
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        let quoted = |value: &str| toml::Value::String(value.to_owned()).to_string();
        let mut output = format!(
            "schema = 1\n\n[defaults]\nleft = {}\nright = {}\n",
            quoted(&self.left),
            quoted(&self.right)
        );
        for mode in &self.modes {
            write!(
                output,
                "\n[[modes]]\nid = {}\nlabel = {}\n",
                quoted(&mode.id),
                quoted(&mode.label)
            )
            .expect("writing to a string cannot fail");
            for column in &mode.columns {
                write!(
                    output,
                    "\n[[modes.columns]]\nkind = {}\nalignment = {}\n",
                    quoted(&format!("{:?}", column.kind).to_ascii_lowercase()),
                    quoted(&format!("{:?}", column.alignment).to_ascii_lowercase())
                )
                .expect("writing to a string cannot fail");
                if let Some(width) = column.width {
                    writeln!(output, "width = {width}").expect("writing to a string cannot fail");
                }
            }
        }
        output
    }
}

fn default_medium() -> String {
    "medium".to_owned()
}

fn canonical_mode_rank(id: &str) -> usize {
    match id {
        "compact" => 0,
        "medium" => 1,
        "full" => 2,
        "wide" => 3,
        "metadata" => 4,
        _ => usize::MAX,
    }
}

fn column(kind: PanelColumnKind, width: Option<u16>, alignment: ColumnAlignment) -> PanelColumn {
    PanelColumn {
        kind,
        width,
        alignment,
    }
}

fn built_in_modes() -> Vec<PanelViewMode> {
    vec![
        PanelViewMode {
            id: "compact".to_owned(),
            label: "Compact".to_owned(),
            columns: vec![column(PanelColumnKind::Name, None, ColumnAlignment::Left)],
        },
        PanelViewMode {
            id: "medium".to_owned(),
            label: "Medium".to_owned(),
            columns: vec![
                column(PanelColumnKind::Name, None, ColumnAlignment::Left),
                column(PanelColumnKind::Size, Some(10), ColumnAlignment::Right),
            ],
        },
        PanelViewMode {
            id: "full".to_owned(),
            label: "Full".to_owned(),
            columns: vec![
                column(PanelColumnKind::Name, None, ColumnAlignment::Left),
                column(PanelColumnKind::Size, Some(10), ColumnAlignment::Right),
                column(PanelColumnKind::Modified, Some(14), ColumnAlignment::Right),
            ],
        },
        PanelViewMode {
            id: "wide".to_owned(),
            label: "Wide".to_owned(),
            columns: vec![
                column(PanelColumnKind::Name, None, ColumnAlignment::Left),
                column(PanelColumnKind::Size, Some(10), ColumnAlignment::Right),
            ],
        },
        PanelViewMode {
            id: "metadata".to_owned(),
            label: "Metadata".to_owned(),
            columns: vec![
                column(PanelColumnKind::Name, None, ColumnAlignment::Left),
                column(PanelColumnKind::Kind, Some(9), ColumnAlignment::Left),
                column(PanelColumnKind::Owner, Some(10), ColumnAlignment::Left),
                column(
                    PanelColumnKind::Permissions,
                    Some(10),
                    ColumnAlignment::Right,
                ),
            ],
        },
    ]
}
