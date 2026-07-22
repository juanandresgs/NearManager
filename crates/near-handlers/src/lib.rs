//! Typed, explainable external handler selection and invocation templates.

use std::{ffi::OsString, path::PathBuf};

use near_core::{
    ExternalAction, ExternalInvocation, ExternalInvocationMode, ExternalResolution,
    ResourceMetadata, ResourceRef,
};
use near_search::ResourcePredicate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HANDLER_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerContext {
    pub resource: ResourceRef,
    pub metadata: ResourceMetadata,
    pub native_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "value", content = "literal", rename_all = "kebab-case")]
pub enum HandlerValue {
    Literal(String),
    ResourceUri,
    ResourceName,
    NativePath,
    NativeParent,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum HandlerInvocationTemplate {
    Argv {
        program: String,
        #[serde(default)]
        arguments: Vec<HandlerValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_directory: Option<HandlerValue>,
    },
    Shell {
        shell: String,
        script: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_directory: Option<HandlerValue>,
    },
}

impl HandlerInvocationTemplate {
    pub fn mode(&self) -> ExternalInvocationMode {
        match self {
            Self::Argv { .. } => ExternalInvocationMode::StructuredArgv,
            Self::Shell { .. } => ExternalInvocationMode::ExplicitShell,
        }
    }

    fn render(&self, context: &HandlerContext) -> Result<ExternalInvocation, HandlerError> {
        match self {
            Self::Argv {
                program,
                arguments,
                current_directory,
            } => {
                let mut invocation = ExternalInvocation::new(program).with_arguments(
                    arguments
                        .iter()
                        .map(|value| render_os(value, context))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                if let Some(directory) = current_directory {
                    invocation =
                        invocation.with_current_directory(render_path(directory, context)?);
                }
                Ok(invocation)
            }
            Self::Shell {
                shell,
                script,
                current_directory,
            } => {
                let mut invocation = ExternalInvocation::explicit_shell(
                    shell,
                    render_shell_script(script, context)?,
                );
                if let Some(directory) = current_directory {
                    invocation =
                        invocation.with_current_directory(render_path(directory, context)?);
                }
                Ok(invocation)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HandlerRule {
    pub id: String,
    pub actions: Vec<ExternalAction>,
    pub predicate: ResourcePredicate,
    pub invocation: HandlerInvocationTemplate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HandlerDocument {
    #[serde(rename = "schema", alias = "schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub handlers: Vec<HandlerRule>,
}

impl HandlerDocument {
    /// Validates the handler configuration schema and identifiers.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas, empty IDs, duplicate IDs, or invalid predicates.
    pub fn validate(&self) -> Result<(), HandlerError> {
        if self.schema_version != HANDLER_SCHEMA_VERSION {
            return Err(HandlerError::UnsupportedSchema(self.schema_version));
        }
        let mut ids = std::collections::BTreeSet::new();
        for rule in &self.handlers {
            if rule.id.trim().is_empty() {
                return Err(HandlerError::EmptyId);
            }
            if !ids.insert(rule.id.clone()) {
                return Err(HandlerError::DuplicateId(rule.id.clone()));
            }
            rule.predicate
                .validate()
                .map_err(|error| HandlerError::InvalidPredicate(error.to_string()))?;
            if rule.predicate.content.is_some() {
                return Err(HandlerError::ContentPredicateUnsupported(rule.id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleDiagnostic {
    pub handler_id: String,
    pub action_matches: bool,
    pub predicate_matches: bool,
    pub selected: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerDiagnostic {
    pub action: ExternalAction,
    pub resource: ResourceRef,
    pub evaluations: Vec<RuleDiagnostic>,
    pub selected_handler: Option<String>,
}

impl HandlerDiagnostic {
    pub fn explanation(&self) -> String {
        self.selected_handler.as_ref().map_or_else(
            || "No handler matched the action and resource predicate".to_owned(),
            |selected| format!("Selected handler {selected} as the first matching rule"),
        )
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HandlerError {
    #[error("unsupported handler schema version {0}")]
    UnsupportedSchema(u16),
    #[error("handler IDs cannot be empty")]
    EmptyId,
    #[error("duplicate handler ID {0}")]
    DuplicateId(String),
    #[error("invalid handler predicate: {0}")]
    InvalidPredicate(String),
    #[error("handler {0} uses a content predicate, which requires resource reads")]
    ContentPredicateUnsupported(String),
    #[error("no handler matched the requested action and resource")]
    NoMatch,
    #[error("template value {0:?} is unavailable for this resource")]
    MissingValue(HandlerValue),
    #[error("shell mode cannot represent non-Unicode value {0:?}")]
    NonUnicodeShellValue(HandlerValue),
    #[error("unknown shell template placeholder {0}")]
    UnknownShellPlaceholder(String),
    #[error("current-directory template must resolve to a native path")]
    InvalidCurrentDirectory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerRegistry {
    rules: Vec<HandlerRule>,
}

impl HandlerRegistry {
    /// Builds an ordered handler registry from a validated document.
    ///
    /// # Errors
    ///
    /// Returns document validation errors.
    pub fn new(document: HandlerDocument) -> Result<Self, HandlerError> {
        document.validate()?;
        Ok(Self {
            rules: document.handlers,
        })
    }

    pub fn diagnose(&self, action: ExternalAction, context: &HandlerContext) -> HandlerDiagnostic {
        let mut selected = false;
        let evaluations = self
            .rules
            .iter()
            .map(|rule| {
                let action_matches = rule.actions.contains(&action);
                let predicate_matches = rule.predicate.matches_metadata(&context.metadata);
                let this_selected = !selected && action_matches && predicate_matches;
                selected |= this_selected;
                RuleDiagnostic {
                    handler_id: rule.id.clone(),
                    action_matches,
                    predicate_matches,
                    selected: this_selected,
                    reason: if !action_matches {
                        "action did not match".to_owned()
                    } else if !predicate_matches {
                        "resource predicate did not match".to_owned()
                    } else if this_selected {
                        "first matching rule".to_owned()
                    } else {
                        "matched after an earlier rule".to_owned()
                    },
                }
            })
            .collect::<Vec<_>>();
        let selected_handler = evaluations
            .iter()
            .find(|evaluation| evaluation.selected)
            .map(|evaluation| evaluation.handler_id.clone());
        HandlerDiagnostic {
            action,
            resource: context.resource.clone(),
            evaluations,
            selected_handler,
        }
    }

    /// Selects the first matching rule and renders its typed invocation.
    ///
    /// # Errors
    ///
    /// Returns no-match or template-rendering errors.
    pub fn resolve(
        &self,
        action: ExternalAction,
        context: &HandlerContext,
    ) -> Result<ExternalResolution, HandlerError> {
        let diagnostic = self.diagnose(action, context);
        let selected = diagnostic
            .selected_handler
            .as_ref()
            .ok_or(HandlerError::NoMatch)?;
        self.resolve_named(action, context, selected)
    }

    /// Resolves every matching handler in configured order.
    ///
    /// # Errors
    ///
    /// Returns no-match or template-rendering errors.
    pub fn resolve_all(
        &self,
        action: ExternalAction,
        context: &HandlerContext,
    ) -> Result<Vec<ExternalResolution>, HandlerError> {
        let diagnostic = self.diagnose(action, context);
        let matching = diagnostic
            .evaluations
            .iter()
            .filter(|evaluation| evaluation.action_matches && evaluation.predicate_matches)
            .map(|evaluation| evaluation.handler_id.as_str())
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return Err(HandlerError::NoMatch);
        }
        matching
            .into_iter()
            .map(|handler_id| self.resolve_named(action, context, handler_id))
            .collect()
    }

    /// Resolves one named matching handler.
    ///
    /// # Errors
    ///
    /// Returns no-match or template-rendering errors.
    pub fn resolve_named(
        &self,
        action: ExternalAction,
        context: &HandlerContext,
        handler_id: &str,
    ) -> Result<ExternalResolution, HandlerError> {
        let rule = self
            .rules
            .iter()
            .find(|rule| rule.id == handler_id)
            .filter(|rule| {
                rule.actions.contains(&action) && rule.predicate.matches_metadata(&context.metadata)
            })
            .ok_or(HandlerError::NoMatch)?;
        let invocation = rule.invocation.render(context)?;
        let mode = match invocation.mode {
            ExternalInvocationMode::StructuredArgv => "structured argv",
            ExternalInvocationMode::ExplicitShell => "explicit shell",
            _ => "future external mode",
        };
        Ok(ExternalResolution {
            invocation,
            handler_id: rule.id.clone(),
            explanation: format!(
                "Selected matching handler {} in configured order; invocation mode: {mode}",
                rule.id
            ),
        })
    }
}

fn render_os(value: &HandlerValue, context: &HandlerContext) -> Result<OsString, HandlerError> {
    match value {
        HandlerValue::Literal(value) => Ok(OsString::from(value)),
        HandlerValue::ResourceUri => Ok(OsString::from(context.resource.location.as_str())),
        HandlerValue::ResourceName => Ok(OsString::from(&context.metadata.name)),
        HandlerValue::NativePath => context
            .native_path
            .as_ref()
            .map(|path| path.as_os_str().to_owned())
            .ok_or_else(|| HandlerError::MissingValue(value.clone())),
        HandlerValue::NativeParent => context
            .native_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|path| path.as_os_str().to_owned())
            .ok_or_else(|| HandlerError::MissingValue(value.clone())),
    }
}

fn render_path(value: &HandlerValue, context: &HandlerContext) -> Result<PathBuf, HandlerError> {
    match value {
        HandlerValue::NativePath => context
            .native_path
            .clone()
            .ok_or_else(|| HandlerError::MissingValue(value.clone())),
        HandlerValue::NativeParent => context
            .native_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .ok_or_else(|| HandlerError::MissingValue(value.clone())),
        _ => Err(HandlerError::InvalidCurrentDirectory),
    }
}

fn render_shell_script(template: &str, context: &HandlerContext) -> Result<String, HandlerError> {
    let mut rendered = template.to_owned();
    for (placeholder, value) in [
        ("${resource.uri}", HandlerValue::ResourceUri),
        ("${resource.name}", HandlerValue::ResourceName),
        ("${resource.path}", HandlerValue::NativePath),
        ("${resource.parent}", HandlerValue::NativeParent),
    ] {
        if rendered.contains(placeholder) {
            let os_value = render_os(&value, context)?;
            let text = os_value
                .to_str()
                .ok_or_else(|| HandlerError::NonUnicodeShellValue(value.clone()))?;
            rendered = rendered.replace(placeholder, &shell_quote(text));
        }
    }
    if let Some(start) = rendered.find("${") {
        let unknown = rendered[start..].split_once('}').map_or_else(
            || rendered[start..].to_owned(),
            |(value, _)| format!("{value}}}"),
        );
        return Err(HandlerError::UnknownShellPlaceholder(unknown));
    }
    Ok(rendered)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    use near_core::{Location, ProviderId, ResourceKind};
    use near_search::{HiddenPolicy, TextPredicate};

    use super::*;

    fn context(path: PathBuf, name: &str) -> HandlerContext {
        HandlerContext {
            resource: ResourceRef {
                provider: ProviderId::from("near.local-fs"),
                location: Location::new("file:///fixture"),
            },
            metadata: ResourceMetadata {
                name: name.to_owned(),
                kind: ResourceKind::File,
                hidden: Some(false),
                ..ResourceMetadata::default()
            },
            native_path: Some(path),
        }
    }

    fn registry(invocation: HandlerInvocationTemplate) -> HandlerRegistry {
        HandlerRegistry::new(HandlerDocument {
            schema_version: HANDLER_SCHEMA_VERSION,
            handlers: vec![HandlerRule {
                id: "test.editor".to_owned(),
                actions: vec![ExternalAction::Edit],
                predicate: ResourcePredicate {
                    name: Some(TextPredicate::Glob("*.txt".to_owned())),
                    hidden: HiddenPolicy::Include,
                    ..ResourcePredicate::default()
                },
                invocation,
            }],
        })
        .unwrap()
    }

    #[test]
    #[cfg(unix)]
    fn structured_templates_keep_hostile_paths_as_one_exact_argument() {
        let path = PathBuf::from(OsString::from_vec(b"/tmp/spaces $()[]\n\xff.txt".to_vec()));
        let resolution = registry(HandlerInvocationTemplate::Argv {
            program: "editor".to_owned(),
            arguments: vec![
                HandlerValue::Literal("--wait".to_owned()),
                HandlerValue::NativePath,
            ],
            current_directory: Some(HandlerValue::NativeParent),
        })
        .resolve(ExternalAction::Edit, &context(path.clone(), "hostile.txt"))
        .unwrap();
        assert_eq!(resolution.invocation.arguments.len(), 2);
        assert_eq!(resolution.invocation.arguments[1], path.as_os_str());
        assert_eq!(
            resolution.invocation.mode,
            ExternalInvocationMode::StructuredArgv
        );
    }

    #[test]
    fn shell_mode_is_explicit_quoted_and_explained() {
        let path = PathBuf::from("/tmp/name; echo injected.txt");
        let resolution = registry(HandlerInvocationTemplate::Shell {
            shell: "/bin/zsh".to_owned(),
            script: "tool ${resource.path}".to_owned(),
            current_directory: Some(HandlerValue::NativeParent),
        })
        .resolve(ExternalAction::Edit, &context(path, "name.txt"))
        .unwrap();
        assert_eq!(
            resolution.invocation.mode,
            ExternalInvocationMode::ExplicitShell
        );
        assert_eq!(
            resolution.invocation.arguments[1],
            OsString::from("tool '/tmp/name; echo injected.txt'")
        );
        assert!(resolution.explanation.contains("explicit shell"));
    }

    #[test]
    fn diagnostics_explain_rejected_and_selected_rules() {
        let registry = registry(HandlerInvocationTemplate::Argv {
            program: "editor".to_owned(),
            arguments: vec![HandlerValue::NativePath],
            current_directory: None,
        });
        let diagnostic = registry.diagnose(
            ExternalAction::View,
            &context(PathBuf::from("/tmp/file.txt"), "file.txt"),
        );
        assert!(diagnostic.selected_handler.is_none());
        assert_eq!(diagnostic.evaluations[0].reason, "action did not match");
    }

    #[test]
    fn execute_alternatives_preserve_configured_order_and_named_selection() {
        let document = HandlerDocument {
            schema_version: HANDLER_SCHEMA_VERSION,
            handlers: ["primary", "alternative"]
                .into_iter()
                .map(|id| HandlerRule {
                    id: id.to_owned(),
                    actions: vec![ExternalAction::Execute],
                    predicate: ResourcePredicate {
                        name: Some(TextPredicate::Glob("*.sh".to_owned())),
                        hidden: HiddenPolicy::Include,
                        ..ResourcePredicate::default()
                    },
                    invocation: HandlerInvocationTemplate::Argv {
                        program: id.to_owned(),
                        arguments: vec![HandlerValue::NativePath],
                        current_directory: Some(HandlerValue::NativeParent),
                    },
                })
                .collect(),
        };
        let registry = HandlerRegistry::new(document).unwrap();
        let context = context(PathBuf::from("/tmp/run.sh"), "run.sh");

        let alternatives = registry
            .resolve_all(ExternalAction::Execute, &context)
            .unwrap();
        assert_eq!(
            alternatives
                .iter()
                .map(|resolution| resolution.handler_id.as_str())
                .collect::<Vec<_>>(),
            ["primary", "alternative"]
        );
        let selected = registry
            .resolve_named(ExternalAction::Execute, &context, "alternative")
            .unwrap();
        assert_eq!(selected.invocation.program, OsString::from("alternative"));
        assert_eq!(
            selected.invocation.mode,
            ExternalInvocationMode::StructuredArgv
        );
    }

    #[test]
    fn unknown_shell_placeholders_fail_closed() {
        let error = registry(HandlerInvocationTemplate::Shell {
            shell: "/bin/zsh".to_owned(),
            script: "tool ${resource.unknown}".to_owned(),
            current_directory: None,
        })
        .resolve(
            ExternalAction::Edit,
            &context(PathBuf::from("/tmp/file.txt"), "file.txt"),
        )
        .unwrap_err();
        assert_eq!(
            error,
            HandlerError::UnknownShellPlaceholder("${resource.unknown}".to_owned())
        );
    }

    #[test]
    fn versioned_handler_document_round_trips() {
        let source = include_str!("../tests/fixtures/handlers-v1.toml");
        let document: HandlerDocument = toml::from_str(source).unwrap();
        document.validate().unwrap();
        let encoded = toml::to_string_pretty(&document).unwrap();
        assert_eq!(
            toml::from_str::<HandlerDocument>(&encoded).unwrap(),
            document
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserMenuScope {
    Global,
    Local,
}

impl UserMenuScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "value", content = "literal", rename_all = "kebab-case")]
pub enum UserMenuValue {
    Literal(String),
    FocusedUri,
    FocusedName,
    FocusedLocation,
    PeerUri,
    PeerName,
    PeerLocation,
    SelectedUris,
    SelectedNames,
    TemporaryList,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum UserMenuInvocationTemplate {
    Argv {
        program: String,
        #[serde(default)]
        arguments: Vec<UserMenuValue>,
    },
    Shell {
        shell: String,
        script: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UserMenuEntry {
    pub id: String,
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub predicate: ResourcePredicate,
    pub invocation: UserMenuInvocationTemplate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UserMenuDocument {
    pub schema: u16,
    #[serde(default)]
    pub global: Vec<UserMenuEntry>,
    #[serde(default)]
    pub local: Vec<UserMenuEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMenuResource {
    pub resource: ResourceRef,
    pub metadata: ResourceMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMenuContext {
    pub focused: UserMenuResource,
    pub focused_location: String,
    pub peer: Option<UserMenuResource>,
    pub peer_location: Option<String>,
    pub selected: Vec<UserMenuResource>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserMenuCatalog {
    global: Vec<UserMenuEntry>,
    local: Vec<UserMenuEntry>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum UserMenuError {
    #[error("invalid user-menu TOML: {0}")]
    Toml(String),
    #[error("unsupported user-menu schema {0}")]
    UnsupportedSchema(u16),
    #[error("user-menu entry IDs keys and labels cannot be empty")]
    EmptyIdentity,
    #[error("duplicate {scope} user-menu entry ID {id}")]
    DuplicateId { scope: String, id: String },
    #[error("user-menu entry {0} has an invalid predicate: {1}")]
    InvalidPredicate(String, String),
    #[error("user-menu entry {0} cannot use a content predicate")]
    ContentPredicate(String),
    #[error("user-menu entry {0} is unavailable in the current context")]
    PredicateMismatch(String),
    #[error("user-menu entry {0} does not exist")]
    UnknownEntry(String),
    #[error("metasymbol {0:?} is unavailable in the current context")]
    MissingValue(UserMenuValue),
    #[error("unknown shell metasymbol {0}")]
    UnknownShellMetasymbol(String),
    #[error("cannot create temporary resource list: {0}")]
    TemporaryList(String),
}

impl UserMenuCatalog {
    /// Parses and validates a versioned user-menu catalog.
    ///
    /// # Errors
    ///
    /// Returns TOML, schema, identity, predicate, or duplicate-entry failures.
    pub fn from_toml(source: &str) -> Result<Self, UserMenuError> {
        let document: UserMenuDocument =
            toml::from_str(source).map_err(|error| UserMenuError::Toml(error.to_string()))?;
        if document.schema != 1 {
            return Err(UserMenuError::UnsupportedSchema(document.schema));
        }
        validate_user_menu_entries(UserMenuScope::Global, &document.global)?;
        validate_user_menu_entries(UserMenuScope::Local, &document.local)?;
        Ok(Self {
            global: document.global,
            local: document.local,
        })
    }

    pub fn entries(&self, scope: UserMenuScope) -> &[UserMenuEntry] {
        match scope {
            UserMenuScope::Global => &self.global,
            UserMenuScope::Local => &self.local,
        }
    }

    /// Resolves one user-menu entry against the current resource context.
    ///
    /// # Errors
    ///
    /// Returns unknown-entry, predicate, metasymbol, or temporary-list failures.
    pub fn resolve(
        &self,
        scope: UserMenuScope,
        id: &str,
        context: &UserMenuContext,
    ) -> Result<ExternalResolution, UserMenuError> {
        let entry = self
            .entries(scope)
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| UserMenuError::UnknownEntry(id.to_owned()))?;
        if !entry.predicate.matches_metadata(&context.focused.metadata) {
            return Err(UserMenuError::PredicateMismatch(id.to_owned()));
        }
        let invocation = render_user_menu_invocation(&entry.invocation, context)?;
        let mode = match invocation.mode {
            ExternalInvocationMode::StructuredArgv => "structured argv",
            ExternalInvocationMode::ExplicitShell => "EXPLICIT SHELL",
            _ => "external mode",
        };
        Ok(ExternalResolution {
            invocation,
            handler_id: format!("user-menu.{}.{}", scope.as_str(), entry.id),
            explanation: format!(
                "{} user menu entry {}; invocation mode: {mode}",
                scope.as_str(),
                entry.key
            ),
        })
    }
}

fn validate_user_menu_entries(
    scope: UserMenuScope,
    entries: &[UserMenuEntry],
) -> Result<(), UserMenuError> {
    let mut ids = std::collections::BTreeSet::new();
    for entry in entries {
        if entry.id.trim().is_empty()
            || entry.key.trim().is_empty()
            || entry.label.trim().is_empty()
        {
            return Err(UserMenuError::EmptyIdentity);
        }
        if !ids.insert(entry.id.clone()) {
            return Err(UserMenuError::DuplicateId {
                scope: scope.as_str().to_owned(),
                id: entry.id.clone(),
            });
        }
        entry.predicate.validate().map_err(|error| {
            UserMenuError::InvalidPredicate(entry.id.clone(), error.to_string())
        })?;
        if entry.predicate.content.is_some() {
            return Err(UserMenuError::ContentPredicate(entry.id.clone()));
        }
    }
    Ok(())
}

fn render_user_menu_invocation(
    template: &UserMenuInvocationTemplate,
    context: &UserMenuContext,
) -> Result<ExternalInvocation, UserMenuError> {
    match template {
        UserMenuInvocationTemplate::Argv { program, arguments } => {
            validate_user_menu_values(arguments.iter(), context)?;
            let mut invocation = ExternalInvocation::new(program);
            let mut temporary_list = None;
            for value in arguments {
                let values = render_user_menu_value(value, context, &mut temporary_list)?;
                invocation
                    .arguments
                    .extend(values.into_iter().map(OsString::from));
            }
            if let Some(path) = temporary_list {
                invocation = invocation.with_cleanup_path(path);
            }
            Ok(invocation)
        }
        UserMenuInvocationTemplate::Shell { shell, script } => {
            let values = shell_user_menu_values(script);
            validate_user_menu_values(values.iter(), context)?;
            let mut temporary_list = None;
            let mut rendered = script.clone();
            for value in values {
                let placeholder = user_menu_placeholder(&value);
                let replacement = render_user_menu_value(&value, context, &mut temporary_list)?
                    .into_iter()
                    .map(|value| shell_quote(&value))
                    .collect::<Vec<_>>()
                    .join(" ");
                rendered = rendered.replace(placeholder, &replacement);
            }
            if let Some(start) = rendered.find("${") {
                let unknown = rendered[start..].split_once('}').map_or_else(
                    || rendered[start..].to_owned(),
                    |(value, _)| format!("{value}}}"),
                );
                if let Some(path) = temporary_list {
                    let _ = std::fs::remove_file(path);
                }
                return Err(UserMenuError::UnknownShellMetasymbol(unknown));
            }
            let mut invocation = ExternalInvocation::explicit_shell(shell, rendered);
            if let Some(path) = temporary_list {
                invocation = invocation.with_cleanup_path(path);
            }
            Ok(invocation)
        }
    }
}

fn validate_user_menu_values<'a>(
    values: impl IntoIterator<Item = &'a UserMenuValue>,
    context: &UserMenuContext,
) -> Result<(), UserMenuError> {
    for value in values {
        match value {
            UserMenuValue::PeerUri | UserMenuValue::PeerName if context.peer.is_none() => {
                return Err(UserMenuError::MissingValue(value.clone()));
            }
            UserMenuValue::PeerLocation if context.peer_location.is_none() => {
                return Err(UserMenuError::MissingValue(value.clone()));
            }
            UserMenuValue::SelectedUris
            | UserMenuValue::SelectedNames
            | UserMenuValue::TemporaryList
                if context.selected.is_empty() =>
            {
                return Err(UserMenuError::MissingValue(value.clone()));
            }
            _ => {}
        }
    }
    Ok(())
}

fn render_user_menu_value(
    value: &UserMenuValue,
    context: &UserMenuContext,
    temporary_list: &mut Option<PathBuf>,
) -> Result<Vec<String>, UserMenuError> {
    match value {
        UserMenuValue::Literal(value) => Ok(vec![value.clone()]),
        UserMenuValue::FocusedUri => {
            Ok(vec![context.focused.resource.location.as_str().to_owned()])
        }
        UserMenuValue::FocusedName => Ok(vec![context.focused.metadata.name.clone()]),
        UserMenuValue::FocusedLocation => Ok(vec![context.focused_location.clone()]),
        UserMenuValue::PeerUri => context
            .peer
            .as_ref()
            .map(|peer| vec![peer.resource.location.as_str().to_owned()])
            .ok_or_else(|| UserMenuError::MissingValue(value.clone())),
        UserMenuValue::PeerName => context
            .peer
            .as_ref()
            .map(|peer| vec![peer.metadata.name.clone()])
            .ok_or_else(|| UserMenuError::MissingValue(value.clone())),
        UserMenuValue::PeerLocation => context
            .peer_location
            .as_ref()
            .map(|location| vec![location.clone()])
            .ok_or_else(|| UserMenuError::MissingValue(value.clone())),
        UserMenuValue::SelectedUris => Ok(context
            .selected
            .iter()
            .map(|resource| resource.resource.location.as_str().to_owned())
            .collect()),
        UserMenuValue::SelectedNames => Ok(context
            .selected
            .iter()
            .map(|resource| resource.metadata.name.clone())
            .collect()),
        UserMenuValue::TemporaryList => {
            if temporary_list.is_none() {
                *temporary_list = Some(write_user_menu_temporary_list(&context.selected)?);
            }
            Ok(vec![
                temporary_list
                    .as_ref()
                    .expect("temporary list was initialized")
                    .to_string_lossy()
                    .into_owned(),
            ])
        }
    }
}

fn write_user_menu_temporary_list(
    resources: &[UserMenuResource],
) -> Result<PathBuf, UserMenuError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_LIST: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_LIST.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("near-user-menu-{}-{id}.txt", std::process::id()));
    let mut body = resources
        .iter()
        .map(|resource| resource.resource.location.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    body.push('\n');
    std::fs::write(&path, body).map_err(|error| UserMenuError::TemporaryList(error.to_string()))?;
    Ok(path)
}

fn shell_user_menu_values(script: &str) -> Vec<UserMenuValue> {
    [
        UserMenuValue::FocusedUri,
        UserMenuValue::FocusedName,
        UserMenuValue::FocusedLocation,
        UserMenuValue::PeerUri,
        UserMenuValue::PeerName,
        UserMenuValue::PeerLocation,
        UserMenuValue::SelectedUris,
        UserMenuValue::SelectedNames,
        UserMenuValue::TemporaryList,
    ]
    .into_iter()
    .filter(|value| script.contains(user_menu_placeholder(value)))
    .collect()
}

fn user_menu_placeholder(value: &UserMenuValue) -> &'static str {
    match value {
        UserMenuValue::FocusedUri => "${focused.uri}",
        UserMenuValue::FocusedName => "${focused.name}",
        UserMenuValue::FocusedLocation => "${focused.location}",
        UserMenuValue::PeerUri => "${peer.uri}",
        UserMenuValue::PeerName => "${peer.name}",
        UserMenuValue::PeerLocation => "${peer.location}",
        UserMenuValue::SelectedUris => "${selected.uris}",
        UserMenuValue::SelectedNames => "${selected.names}",
        UserMenuValue::TemporaryList => "${temp.list}",
        UserMenuValue::Literal(_) => "",
    }
}

#[cfg(test)]
mod user_menu_tests {
    use near_core::{Location, ProviderId, ResourceKind};

    use super::*;

    fn resource(uri: &str, name: &str) -> UserMenuResource {
        UserMenuResource {
            resource: ResourceRef {
                provider: ProviderId::from("test.provider"),
                location: Location::new(uri),
            },
            metadata: ResourceMetadata {
                name: name.to_owned(),
                kind: ResourceKind::File,
                hidden: Some(false),
                ..ResourceMetadata::default()
            },
        }
    }

    fn context() -> UserMenuContext {
        UserMenuContext {
            focused: resource("test:///focused", "focused name.txt"),
            focused_location: "test:///folder".to_owned(),
            peer: Some(resource("test:///peer", "peer.txt")),
            peer_location: Some("test:///other".to_owned()),
            selected: vec![
                resource("test:///one", "one.txt"),
                resource("test:///two", "two.txt"),
            ],
        }
    }

    #[test]
    fn structured_metasymbols_expand_to_exact_arguments() {
        let catalog = UserMenuCatalog::from_toml(
            r#"
schema = 1
[[global]]
id = "inspect"
key = "I"
label = "Inspect"
[global.invocation]
mode = "argv"
program = "tool"
arguments = [
  { value = "focused-uri" },
  { value = "peer-name" },
  { value = "selected-names" },
]
"#,
        )
        .unwrap();
        let resolution = catalog
            .resolve(UserMenuScope::Global, "inspect", &context())
            .unwrap();
        assert_eq!(
            resolution.invocation.mode,
            ExternalInvocationMode::StructuredArgv
        );
        assert_eq!(
            resolution.invocation.arguments,
            ["test:///focused", "peer.txt", "one.txt", "two.txt"]
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn temporary_lists_are_provider_neutral_and_registered_for_cleanup() {
        let catalog = UserMenuCatalog::from_toml(
            r#"
schema = 1
[[local]]
id = "list"
key = "L"
label = "List"
[local.invocation]
mode = "argv"
program = "cat"
arguments = [{ value = "temporary-list" }]
"#,
        )
        .unwrap();
        let resolution = catalog
            .resolve(UserMenuScope::Local, "list", &context())
            .unwrap();
        let path = &resolution.invocation.cleanup_paths[0];
        assert_eq!(resolution.invocation.arguments[0], path.as_os_str());
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "test:///one\ntest:///two\n"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn shell_entries_are_explicit_and_quote_hostile_values() {
        let catalog = UserMenuCatalog::from_toml(
            r#"
schema = 1
[[global]]
id = "shell"
key = "S"
label = "Shell"
[global.invocation]
mode = "shell"
shell = "/bin/sh"
script = "printf %s ${focused.name}"
"#,
        )
        .unwrap();
        let resolution = catalog
            .resolve(UserMenuScope::Global, "shell", &context())
            .unwrap();
        assert_eq!(
            resolution.invocation.mode,
            ExternalInvocationMode::ExplicitShell
        );
        assert!(resolution.explanation.contains("EXPLICIT SHELL"));
        assert_eq!(
            resolution.invocation.arguments[1],
            OsString::from("printf %s 'focused name.txt'")
        );
    }
}
