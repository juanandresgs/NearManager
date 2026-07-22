use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{CapabilitySet, CommandId, Location, ResourceRef, SurfaceId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<CommandValue>),
    Table(BTreeMap<String, CommandValue>),
}

impl CommandValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::String(value) => value.parse().ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArgumentKind {
    Boolean,
    Integer,
    Float,
    String,
    Array,
    Table,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArgumentSchema {
    pub kind: ArgumentKind,
    pub required: bool,
    pub description: String,
    pub default: Option<CommandValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SafetyClass {
    ReadOnly,
    Reversible,
    Confirmable,
    Destructive,
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandDescriptor {
    pub id: CommandId,
    pub title: String,
    pub description: String,
    pub category: Vec<String>,
    pub safety: SafetyClass,
    pub arguments: BTreeMap<String, ArgumentSchema>,
}

impl CommandDescriptor {
    pub fn invokable_without_arguments(&self) -> bool {
        self.arguments.values().all(|argument| !argument.required)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    Available,
    Unavailable { reason: String },
}

impl Availability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub id: CommandId,
    pub arguments: BTreeMap<String, CommandValue>,
}

impl CommandInvocation {
    pub fn new(id: impl Into<CommandId>) -> Self {
        Self {
            id: id.into(),
            arguments: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ActionContext {
    pub focused_surface: Option<SurfaceId>,
    pub peer_surface: Option<SurfaceId>,
    pub current: Option<ResourceRef>,
    pub selected: Vec<ResourceRef>,
    pub location: Option<Location>,
    pub peer_location: Option<Location>,
    pub capabilities: CapabilitySet,
    pub peer_capabilities: CapabilitySet,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(arguments: BTreeMap<String, ArgumentSchema>) -> CommandDescriptor {
        CommandDescriptor {
            id: CommandId::from("test.command"),
            title: "Test command".to_owned(),
            description: "Test command discoverability".to_owned(),
            category: vec!["Test".to_owned()],
            safety: SafetyClass::ReadOnly,
            arguments,
        }
    }

    #[test]
    fn argument_free_invocation_requires_no_required_arguments() {
        assert!(descriptor(BTreeMap::new()).invokable_without_arguments());
        assert!(
            descriptor(BTreeMap::from([(
                "optional".to_owned(),
                ArgumentSchema {
                    kind: ArgumentKind::String,
                    required: false,
                    description: "Optional value".to_owned(),
                    default: None,
                },
            )]))
            .invokable_without_arguments()
        );
        assert!(
            !descriptor(BTreeMap::from([(
                "required".to_owned(),
                ArgumentSchema {
                    kind: ArgumentKind::String,
                    required: true,
                    description: "Required value".to_owned(),
                    default: None,
                },
            )]))
            .invokable_without_arguments()
        );
    }
}
