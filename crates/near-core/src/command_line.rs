use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Location, ResourceRef};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPrefixDescriptor {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionCommandPrefix {
    pub prefix: CommandPrefixDescriptor,
    pub command: crate::CommandId,
    pub argument: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandLineOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandLineExecutor: Send + Sync {
    /// Executes a command at a provider location.
    ///
    /// # Errors
    ///
    /// Returns launch, execution, or provider-location failures.
    fn execute(&self, location: &Location, command: &str) -> Result<CommandLineOutput, String>;
}

pub trait CommandLineArgumentResolver: Send + Sync {
    fn quote_text(&self, value: &str) -> String;
    /// Resolves and quotes a provider location for the command shell.
    ///
    /// # Errors
    ///
    /// Returns an error when the location has no representable shell argument.
    fn location_argument(&self, location: &Location) -> Result<String, String>;
    /// Resolves and quotes a resource for the command shell.
    ///
    /// # Errors
    ///
    /// Returns an error when the resource has no representable shell argument.
    fn resource_argument(&self, resource: &ResourceRef) -> Result<String, String>;

    /// Resolves a provider location to a native shell working directory.
    ///
    /// `None` means the provider location cannot host a local interactive shell.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider owns the location but cannot resolve it safely.
    fn native_working_directory(&self, _location: &Location) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHistoryEntry {
    pub command: String,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_use_count")]
    pub use_count: u64,
}

impl CommandHistoryEntry {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            locked: false,
            use_count: 1,
        }
    }
}

const fn default_use_count() -> u64 {
    1
}

pub trait CommandHistoryStore: Send + Sync {
    /// Loads persistent command history.
    ///
    /// # Errors
    ///
    /// Returns storage, decoding, or migration failures.
    fn load(&self) -> Result<Vec<CommandHistoryEntry>, String>;
    /// Atomically saves persistent command history.
    ///
    /// # Errors
    ///
    /// Returns storage or serialization failures.
    fn save(&self, entries: &[CommandHistoryEntry]) -> Result<(), String>;
}
