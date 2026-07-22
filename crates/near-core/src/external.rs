use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::ResourceRef;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ExternalAction {
    Open,
    View,
    Edit,
    Inspect,
    Execute,
    Shell,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum ExternalInvocationMode {
    #[default]
    StructuredArgv,
    ExplicitShell,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalInvocation {
    pub mode: ExternalInvocationMode,
    pub program: OsString,
    pub arguments: Vec<OsString>,
    pub current_directory: Option<PathBuf>,
    pub environment: BTreeMap<OsString, OsString>,
    pub clear_environment: bool,
    pub cleanup_paths: Vec<PathBuf>,
}

impl ExternalInvocation {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            mode: ExternalInvocationMode::StructuredArgv,
            program: program.into(),
            arguments: Vec::new(),
            current_directory: None,
            environment: BTreeMap::new(),
            clear_environment: false,
            cleanup_paths: Vec::new(),
        }
    }

    pub fn explicit_shell(program: impl Into<OsString>, script: impl Into<OsString>) -> Self {
        Self {
            mode: ExternalInvocationMode::ExplicitShell,
            program: program.into(),
            arguments: vec![OsString::from("-lc"), script.into()],
            current_directory: None,
            environment: BTreeMap::new(),
            clear_environment: false,
            cleanup_paths: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_argument(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    #[must_use]
    pub fn with_arguments(
        mut self,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_current_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.current_directory = Some(directory.into());
        self
    }

    #[must_use]
    pub fn with_environment(
        mut self,
        key: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_cleanup_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.cleanup_paths.push(path.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalResolution {
    pub invocation: ExternalInvocation,
    pub handler_id: String,
    pub explanation: String,
}

pub trait ExternalToolResolver: Send + Sync {
    /// Resolves a semantic action and resource to a structured process invocation.
    ///
    /// # Errors
    ///
    /// Returns a user-facing reason when no safe external invocation applies.
    fn resolve(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<ExternalInvocation, String>;

    /// Resolves an invocation together with user-facing selection diagnostics.
    ///
    /// # Errors
    ///
    /// Returns the same resolution error as [`Self::resolve`].
    fn resolve_explained(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<ExternalResolution, String> {
        self.resolve(action, resource)
            .map(|invocation| ExternalResolution {
                invocation,
                handler_id: "legacy.external-resolver".to_owned(),
                explanation: "Selected by a legacy external resolver without rule diagnostics"
                    .to_owned(),
            })
    }

    /// Resolves every matching handler in configured order.
    ///
    /// # Errors
    ///
    /// Returns a user-facing reason when no safe external invocation applies.
    fn alternatives(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<Vec<ExternalResolution>, String> {
        self.resolve_explained(action, resource)
            .map(|resolution| vec![resolution])
    }

    /// Resolves one explicitly selected matching handler.
    ///
    /// # Errors
    ///
    /// Returns a user-facing reason when the named handler is unavailable or does not match.
    fn resolve_named(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
        handler_id: &str,
    ) -> Result<ExternalResolution, String> {
        self.alternatives(action, resource)?
            .into_iter()
            .find(|resolution| resolution.handler_id == handler_id)
            .ok_or_else(|| format!("handler {handler_id} did not match the resource and action"))
    }

    /// Returns a human-readable explanation of handler selection.
    ///
    /// # Errors
    ///
    /// Returns a resolution error when no handler can be diagnosed for the request.
    fn diagnose(&self, action: ExternalAction, resource: &ResourceRef) -> Result<String, String> {
        self.resolve_explained(action, resource)
            .map(|resolution| resolution.explanation)
    }
}
