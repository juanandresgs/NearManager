use std::collections::BTreeMap;

use crate::{
    ActionContext, CommandDescriptor, CommandId, CommandInvocation, ExtensionCommandPrefix,
    Location, ResourceRef,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionMenuItem {
    pub label: String,
    pub description: String,
    pub command: CommandId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionSetting {
    pub id: String,
    pub label: String,
    pub description: String,
    pub value: String,
    pub required: bool,
    pub secret: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionHelpTopic {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ExtensionEffect {
    Message(String),
    Navigate(Location),
    Open(Vec<ResourceRef>),
    Task(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionReport {
    pub effect: ExtensionEffect,
    pub diagnostics: Vec<String>,
}

pub trait CommandExtension: Send + Sync {
    fn id(&self) -> &str;

    /// Returns semantic commands supplied by the extension.
    ///
    /// # Errors
    ///
    /// Returns extension loading, compatibility, or descriptor failures.
    fn commands(&self) -> Result<Vec<CommandDescriptor>, String>;

    /// Returns command-line prefixes mapped to this extension's semantic commands.
    ///
    /// # Errors
    ///
    /// Returns descriptor or compatibility failures.
    fn command_prefixes(&self) -> Result<Vec<ExtensionCommandPrefix>, String> {
        Ok(Vec::new())
    }

    /// Returns actions shown in the extension menu. Commands remain the fallback when empty.
    ///
    /// # Errors
    ///
    /// Returns contribution loading, compatibility, or validation failures.
    fn menu_items(&self) -> Result<Vec<ExtensionMenuItem>, String> {
        Ok(Vec::new())
    }

    /// Returns editable extension settings.
    ///
    /// # Errors
    ///
    /// Returns settings loading, compatibility, or validation failures.
    fn settings(&self) -> Result<Vec<ExtensionSetting>, String> {
        Ok(Vec::new())
    }

    /// Applies settings collected by the host.
    ///
    /// # Errors
    ///
    /// Returns validation, persistence, capability, or guest failures.
    fn update_settings(&self, settings: &BTreeMap<String, String>) -> Result<(), String> {
        if settings.is_empty() {
            Ok(())
        } else {
            Err("extension settings are read-only".to_owned())
        }
    }

    /// Returns extension-authored help topics in addition to generated command help.
    ///
    /// # Errors
    ///
    /// Returns help loading, compatibility, or validation failures.
    fn help_topics(&self) -> Result<Vec<ExtensionHelpTopic>, String> {
        Ok(Vec::new())
    }

    /// Invokes one semantic extension command.
    ///
    /// # Errors
    ///
    /// Returns availability, capability, resource-limit, trap, or guest failures.
    fn invoke(
        &self,
        invocation: &CommandInvocation,
        context: &ActionContext,
    ) -> Result<ExtensionReport, String>;
}
