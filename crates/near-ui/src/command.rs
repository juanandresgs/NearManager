use std::collections::BTreeMap;

use near_core::{
    ActionContext, ArgumentKind, Availability, CommandDescriptor, CommandId, CommandInvocation,
    CommandValue,
};
use thiserror::Error;

pub trait Command: Send + Sync {
    fn descriptor(&self) -> &CommandDescriptor;

    fn availability(&self, _context: &ActionContext) -> Availability {
        Availability::Available
    }
}

#[derive(Debug, Error)]
pub enum CommandRegistryError {
    #[error("command {0} is already registered")]
    Duplicate(CommandId),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CommandCheckError {
    #[error("unknown command: {0}")]
    Unknown(CommandId),
    #[error("command {id} is unavailable: {reason}")]
    Unavailable { id: CommandId, reason: String },
    #[error("command {id} requires argument '{argument}'")]
    MissingArgument { id: CommandId, argument: String },
    #[error("command {id} does not declare argument '{argument}'")]
    UnknownArgument { id: CommandId, argument: String },
    #[error("command {id} argument '{argument}' must be {expected:?}")]
    InvalidArgument {
        id: CommandId,
        argument: String,
        expected: ArgumentKind,
    },
}

#[derive(Default)]
pub struct CommandRegistry {
    commands: BTreeMap<CommandId, Box<dyn Command>>,
}

impl CommandRegistry {
    /// Registers a command under its descriptor ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the command ID is already registered.
    pub fn register(
        &mut self,
        command: impl Command + 'static,
    ) -> Result<(), CommandRegistryError> {
        let id = command.descriptor().id.clone();
        if self.commands.contains_key(&id) {
            return Err(CommandRegistryError::Duplicate(id));
        }
        self.commands.insert(id, Box::new(command));
        Ok(())
    }

    pub fn get(&self, id: &CommandId) -> Option<&dyn Command> {
        self.commands.get(id).map(Box::as_ref)
    }

    /// Validates registration, argument schema, and contextual availability.
    ///
    /// # Errors
    ///
    /// Returns a precise error when the command is unknown, malformed, or unavailable.
    pub fn check(
        &self,
        invocation: &CommandInvocation,
        context: &ActionContext,
    ) -> Result<&dyn Command, CommandCheckError> {
        let command = self
            .get(&invocation.id)
            .ok_or_else(|| CommandCheckError::Unknown(invocation.id.clone()))?;
        validate_arguments(invocation, command.descriptor())?;
        match command.availability(context) {
            Availability::Available => Ok(command),
            Availability::Unavailable { reason } => Err(CommandCheckError::Unavailable {
                id: invocation.id.clone(),
                reason,
            }),
        }
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &CommandDescriptor> {
        self.commands.values().map(|command| command.descriptor())
    }

    pub fn available<'a>(
        &'a self,
        context: &'a ActionContext,
    ) -> impl Iterator<Item = &'a CommandDescriptor> {
        self.commands
            .values()
            .filter(move |command| command.availability(context).is_available())
            .map(|command| command.descriptor())
    }
}

fn validate_arguments(
    invocation: &CommandInvocation,
    descriptor: &CommandDescriptor,
) -> Result<(), CommandCheckError> {
    for (name, schema) in &descriptor.arguments {
        if schema.required && !invocation.arguments.contains_key(name) {
            return Err(CommandCheckError::MissingArgument {
                id: invocation.id.clone(),
                argument: name.clone(),
            });
        }
    }
    for (name, value) in &invocation.arguments {
        let Some(schema) = descriptor.arguments.get(name) else {
            return Err(CommandCheckError::UnknownArgument {
                id: invocation.id.clone(),
                argument: name.clone(),
            });
        };
        if !argument_matches(value, schema.kind) {
            return Err(CommandCheckError::InvalidArgument {
                id: invocation.id.clone(),
                argument: name.clone(),
                expected: schema.kind,
            });
        }
    }
    Ok(())
}

fn argument_matches(value: &CommandValue, kind: ArgumentKind) -> bool {
    matches!(
        (value, kind),
        (CommandValue::Boolean(_), ArgumentKind::Boolean)
            | (CommandValue::Integer(_), ArgumentKind::Integer)
            | (CommandValue::Float(_), ArgumentKind::Float)
            | (CommandValue::String(_), ArgumentKind::String)
            | (CommandValue::Array(_), ArgumentKind::Array)
            | (CommandValue::Table(_), ArgumentKind::Table)
    )
}

#[cfg(test)]
mod tests {
    use near_core::{
        ArgumentSchema, CapabilitySet, CommandDescriptor, CommandInvocation, SafetyClass,
    };

    use super::*;

    struct TestCommand(CommandDescriptor, Availability);

    impl Command for TestCommand {
        fn descriptor(&self) -> &CommandDescriptor {
            &self.0
        }

        fn availability(&self, _context: &ActionContext) -> Availability {
            self.1.clone()
        }
    }

    fn descriptor() -> CommandDescriptor {
        CommandDescriptor {
            id: CommandId::from("test.run"),
            title: "Run".to_owned(),
            description: "Run test".to_owned(),
            category: vec!["Test".to_owned()],
            safety: SafetyClass::ReadOnly,
            arguments: [(
                "count".to_owned(),
                ArgumentSchema {
                    kind: ArgumentKind::Integer,
                    required: true,
                    description: "Count".to_owned(),
                    default: None,
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn check_reports_argument_and_availability_failures() {
        let mut registry = CommandRegistry::default();
        registry
            .register(TestCommand(
                descriptor(),
                Availability::Unavailable {
                    reason: "no target".to_owned(),
                },
            ))
            .unwrap();
        let context = ActionContext {
            capabilities: CapabilitySet::default(),
            ..ActionContext::default()
        };
        let missing = CommandInvocation {
            id: CommandId::from("test.run"),
            arguments: BTreeMap::default(),
        };
        assert!(matches!(
            registry.check(&missing, &context),
            Err(CommandCheckError::MissingArgument { .. })
        ));
        let valid = CommandInvocation {
            id: CommandId::from("test.run"),
            arguments: [("count".to_owned(), CommandValue::Integer(2))]
                .into_iter()
                .collect(),
        };
        assert!(matches!(
            registry.check(&valid, &context),
            Err(CommandCheckError::Unavailable {
                id,
                reason,
            }) if id == CommandId::from("test.run") && reason == "no target"
        ));
    }
}
