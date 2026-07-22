//! Schema-versioned semantic command recording and context-aware replay.

use std::path::{Path, PathBuf};

use near_core::{ActionContext, CapabilityId, CommandInvocation, ContextId, SafetyClass};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MACRO_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresenceCondition {
    #[default]
    Any,
    Present,
    Absent,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MacroCondition {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_contexts: Vec<ContextId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<CapabilityId>,
    #[serde(default)]
    pub current_resource: PresenceCondition,
    #[serde(default)]
    pub peer_surface: PresenceCondition,
}

impl MacroCondition {
    pub fn matches(&self, context: &MacroContext) -> bool {
        self.required_contexts
            .iter()
            .all(|required| context.contexts.contains(required))
            && self
                .required_capabilities
                .iter()
                .all(|required| context.action.capabilities.contains(required))
            && presence_matches(self.current_resource, context.action.current.is_some())
            && presence_matches(self.peer_surface, context.action.peer_surface.is_some())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroContext {
    pub contexts: Vec<ContextId>,
    pub action: ActionContext,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacroTrust {
    #[default]
    Untrusted,
    Trusted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroStep {
    pub invocation: CommandInvocation,
    #[serde(default)]
    pub when: MacroCondition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticMacro {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    #[serde(default)]
    pub trust: MacroTrust,
    #[serde(default)]
    pub when: MacroCondition,
    #[serde(default)]
    pub steps: Vec<MacroStep>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroDocument {
    #[serde(rename = "schema", alias = "schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub macros: Vec<SemanticMacro>,
}

impl MacroDocument {
    /// Validates schema, IDs, and non-empty semantic invocations.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas, duplicate IDs, empty IDs, or empty command IDs.
    pub fn validate(&self) -> Result<(), MacroError> {
        if !matches!(self.schema_version, 1 | MACRO_SCHEMA_VERSION) {
            return Err(MacroError::UnsupportedSchema(self.schema_version));
        }
        let mut ids = std::collections::BTreeSet::new();
        for semantic_macro in &self.macros {
            if semantic_macro.id.trim().is_empty() {
                return Err(MacroError::EmptyId);
            }
            if !ids.insert(semantic_macro.id.clone()) {
                return Err(MacroError::DuplicateId(semantic_macro.id.clone()));
            }
            if self.schema_version == 1 && semantic_macro.binding.is_some() {
                return Err(MacroError::BindingRequiresSchemaTwo(
                    semantic_macro.id.clone(),
                ));
            }
            if semantic_macro
                .binding
                .as_ref()
                .is_some_and(|binding| binding.trim().is_empty())
            {
                return Err(MacroError::EmptyBinding(semantic_macro.id.clone()));
            }
            if semantic_macro
                .steps
                .iter()
                .any(|step| step.invocation.id.as_str().is_empty())
            {
                return Err(MacroError::EmptyCommand(semantic_macro.id.clone()));
            }
        }
        Ok(())
    }
}

pub trait MacroStore: Send + Sync {
    /// Loads one complete versioned macro catalog.
    ///
    /// # Errors
    ///
    /// Returns storage, parse, or validation failures.
    fn load(&self) -> Result<MacroDocument, String>;

    /// Atomically saves one complete versioned macro catalog.
    ///
    /// # Errors
    ///
    /// Returns serialization or storage failures.
    fn save(&self, document: &MacroDocument) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TomlMacroStore {
    path: PathBuf,
}

impl TomlMacroStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl MacroStore for TomlMacroStore {
    fn load(&self) -> Result<MacroDocument, String> {
        let source = std::fs::read_to_string(&self.path).map_err(|error| error.to_string())?;
        let document =
            toml::from_str::<MacroDocument>(&source).map_err(|error| error.to_string())?;
        document.validate().map_err(|error| error.to_string())?;
        Ok(document)
    }

    fn save(&self, document: &MacroDocument) -> Result<(), String> {
        document.validate().map_err(|error| error.to_string())?;
        let source = toml::to_string_pretty(document).map_err(|error| error.to_string())?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("toml.tmp");
        std::fs::write(&temporary, source).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayPolicy {
    pub allow_trusted_confirmable: bool,
    pub allow_trusted_destructive: bool,
}

impl Default for ReplayPolicy {
    fn default() -> Self {
        Self {
            allow_trusted_confirmable: true,
            allow_trusted_destructive: false,
        }
    }
}

pub trait MacroHost {
    fn macro_context(&self) -> MacroContext;

    /// Validates command registration, typed arguments, and current availability.
    ///
    /// # Errors
    ///
    /// Returns a precise host error when the invocation cannot run in the current context.
    fn validate_macro_command(&self, invocation: &CommandInvocation)
    -> Result<SafetyClass, String>;

    /// Invokes a previously validated semantic command through the normal host dispatcher.
    ///
    /// # Errors
    ///
    /// Returns a host execution error without bypassing normal confirmation behavior.
    fn invoke_macro_command(&mut self, invocation: &CommandInvocation) -> Result<(), String>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReplayReport {
    pub completed: usize,
    pub skipped_conditions: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroStepDiagnostic {
    pub step: usize,
    pub command: String,
    pub condition_matches: bool,
    pub safety: Option<SafetyClass>,
    pub authorized: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroDiagnostic {
    pub macro_available: bool,
    pub steps: Vec<MacroStepDiagnostic>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MacroError {
    #[error("unsupported macro schema version {0}")]
    UnsupportedSchema(u16),
    #[error("macro IDs cannot be empty")]
    EmptyId,
    #[error("duplicate macro ID {0}")]
    DuplicateId(String),
    #[error("macro {0} uses a binding that requires schema version 2")]
    BindingRequiresSchemaTwo(String),
    #[error("macro {0} contains an empty binding")]
    EmptyBinding(String),
    #[error("macro {0} contains an empty command ID")]
    EmptyCommand(String),
    #[error("macro {0} is not available in the current context")]
    ContextUnavailable(String),
    #[error("macro step {step} failed validation: {reason}")]
    CommandUnavailable { step: usize, reason: String },
    #[error("macro step {step} with safety {safety:?} is not authorized")]
    SafetyDenied { step: usize, safety: SafetyClass },
    #[error("macro step {step} failed: {reason}")]
    InvocationFailed { step: usize, reason: String },
    #[error("no macro recording is active")]
    NotRecording,
    #[error("a macro recording is already active")]
    AlreadyRecording,
}

#[derive(Clone, Debug, Default)]
pub struct MacroEngine {
    policy: ReplayPolicy,
}

impl MacroEngine {
    pub fn new(policy: ReplayPolicy) -> Self {
        Self { policy }
    }

    /// Replays semantic invocations through normal host validation and dispatch.
    ///
    /// # Errors
    ///
    /// Stops at the first unavailable, unauthorized, or failed invocation.
    pub fn replay(
        &self,
        semantic_macro: &SemanticMacro,
        host: &mut impl MacroHost,
    ) -> Result<ReplayReport, MacroError> {
        if !semantic_macro.when.matches(&host.macro_context()) {
            return Err(MacroError::ContextUnavailable(semantic_macro.id.clone()));
        }
        let mut report = ReplayReport::default();
        for (index, step) in semantic_macro.steps.iter().enumerate() {
            if !step.when.matches(&host.macro_context()) {
                report.skipped_conditions = report.skipped_conditions.saturating_add(1);
                continue;
            }
            let safety = host
                .validate_macro_command(&step.invocation)
                .map_err(|reason| MacroError::CommandUnavailable {
                    step: index,
                    reason,
                })?;
            if !authorized(safety, semantic_macro.trust, self.policy) {
                return Err(MacroError::SafetyDenied {
                    step: index,
                    safety,
                });
            }
            host.invoke_macro_command(&step.invocation)
                .map_err(|reason| MacroError::InvocationFailed {
                    step: index,
                    reason,
                })?;
            report.completed = report.completed.saturating_add(1);
        }
        Ok(report)
    }

    pub fn diagnose(
        &self,
        semantic_macro: &SemanticMacro,
        host: &impl MacroHost,
    ) -> MacroDiagnostic {
        let context = host.macro_context();
        let steps = semantic_macro
            .steps
            .iter()
            .enumerate()
            .map(|(step, macro_step)| {
                let condition_matches = macro_step.when.matches(&context);
                match host.validate_macro_command(&macro_step.invocation) {
                    Ok(safety) => MacroStepDiagnostic {
                        step,
                        command: macro_step.invocation.id.to_string(),
                        condition_matches,
                        safety: Some(safety),
                        authorized: authorized(safety, semantic_macro.trust, self.policy),
                        error: None,
                    },
                    Err(error) => MacroStepDiagnostic {
                        step,
                        command: macro_step.invocation.id.to_string(),
                        condition_matches,
                        safety: None,
                        authorized: false,
                        error: Some(error),
                    },
                }
            })
            .collect();
        MacroDiagnostic {
            macro_available: semantic_macro.when.matches(&context),
            steps,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MacroRecorder {
    active: Option<SemanticMacro>,
}

impl MacroRecorder {
    /// Starts recording semantic command invocations.
    ///
    /// # Errors
    ///
    /// Returns an error when another recording is already active.
    pub fn start(&mut self, semantic_macro: SemanticMacro) -> Result<(), MacroError> {
        if self.active.is_some() {
            return Err(MacroError::AlreadyRecording);
        }
        self.active = Some(SemanticMacro {
            steps: Vec::new(),
            ..semantic_macro
        });
        Ok(())
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    pub fn record(&mut self, invocation: CommandInvocation) {
        if let Some(active) = &mut self.active {
            active.steps.push(MacroStep {
                invocation,
                when: MacroCondition::default(),
            });
        }
    }

    /// Finishes and returns the inspectable semantic macro.
    ///
    /// # Errors
    ///
    /// Returns an error when no recording is active.
    pub fn finish(&mut self) -> Result<SemanticMacro, MacroError> {
        self.active.take().ok_or(MacroError::NotRecording)
    }
}

fn presence_matches(condition: PresenceCondition, present: bool) -> bool {
    match condition {
        PresenceCondition::Any => true,
        PresenceCondition::Present => present,
        PresenceCondition::Absent => !present,
    }
}

fn authorized(safety: SafetyClass, trust: MacroTrust, policy: ReplayPolicy) -> bool {
    match safety {
        SafetyClass::ReadOnly | SafetyClass::Reversible => true,
        SafetyClass::Confirmable => {
            matches!(trust, MacroTrust::Trusted) && policy.allow_trusted_confirmable
        }
        SafetyClass::Destructive | SafetyClass::Privileged => {
            matches!(trust, MacroTrust::Trusted) && policy.allow_trusted_destructive
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use near_core::{CommandId, CommandValue};

    use super::*;

    struct TestHost {
        context: MacroContext,
        safety: SafetyClass,
        available: bool,
        invoked: Vec<CommandInvocation>,
    }

    impl MacroHost for TestHost {
        fn macro_context(&self) -> MacroContext {
            self.context.clone()
        }

        fn validate_macro_command(
            &self,
            _invocation: &CommandInvocation,
        ) -> Result<SafetyClass, String> {
            if self.available {
                Ok(self.safety)
            } else {
                Err("unavailable in test context".to_owned())
            }
        }

        fn invoke_macro_command(&mut self, invocation: &CommandInvocation) -> Result<(), String> {
            self.invoked.push(invocation.clone());
            Ok(())
        }
    }

    fn invocation() -> CommandInvocation {
        CommandInvocation {
            id: CommandId::from("near.collection.move"),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
        }
    }

    fn semantic_macro(trust: MacroTrust) -> SemanticMacro {
        SemanticMacro {
            id: "test.navigation".to_owned(),
            title: "Navigation".to_owned(),
            binding: None,
            trust,
            when: MacroCondition::default(),
            steps: vec![MacroStep {
                invocation: invocation(),
                when: MacroCondition::default(),
            }],
        }
    }

    fn host(safety: SafetyClass) -> TestHost {
        TestHost {
            context: MacroContext {
                contexts: vec![ContextId::from("workspace.panel")],
                action: ActionContext::default(),
            },
            safety,
            available: true,
            invoked: Vec::new(),
        }
    }

    #[test]
    fn recorder_stores_command_ids_and_typed_arguments_not_keys() {
        let mut recorder = MacroRecorder::default();
        recorder
            .start(SemanticMacro {
                steps: Vec::new(),
                ..semantic_macro(MacroTrust::Untrusted)
            })
            .unwrap();
        recorder.record(invocation());
        let finished_macro = recorder.finish().unwrap();
        assert_eq!(finished_macro.steps[0].invocation, invocation());
    }

    #[test]
    fn replay_is_independent_from_key_bindings() {
        let mut host = host(SafetyClass::ReadOnly);
        MacroEngine::default()
            .replay(&semantic_macro(MacroTrust::Untrusted), &mut host)
            .unwrap();
        assert_eq!(host.invoked, vec![invocation()]);
    }

    #[test]
    fn unavailable_commands_stop_replay_before_invocation() {
        let mut host = host(SafetyClass::ReadOnly);
        host.available = false;
        assert!(matches!(
            MacroEngine::default().replay(&semantic_macro(MacroTrust::Untrusted), &mut host),
            Err(MacroError::CommandUnavailable { .. })
        ));
        assert!(host.invoked.is_empty());
    }

    #[test]
    fn safety_policy_requires_explicit_trust_and_configuration() {
        let mut host = host(SafetyClass::Confirmable);
        assert!(matches!(
            MacroEngine::default().replay(&semantic_macro(MacroTrust::Untrusted), &mut host),
            Err(MacroError::SafetyDenied { .. })
        ));
        MacroEngine::default()
            .replay(&semantic_macro(MacroTrust::Trusted), &mut host)
            .unwrap();
        host.safety = SafetyClass::Destructive;
        assert!(matches!(
            MacroEngine::default().replay(&semantic_macro(MacroTrust::Trusted), &mut host),
            Err(MacroError::SafetyDenied { .. })
        ));
    }

    #[test]
    fn versioned_macro_document_is_inspectable_toml() {
        let source = include_str!("../tests/fixtures/macros-v1.toml");
        let document: MacroDocument = toml::from_str(source).unwrap();
        document.validate().unwrap();
        let encoded = toml::to_string_pretty(&document).unwrap();
        assert_eq!(toml::from_str::<MacroDocument>(&encoded).unwrap(), document);
    }

    #[test]
    fn schema_two_bindings_round_trip_through_atomic_store() {
        let root = std::env::temp_dir().join(format!("near-macros-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = TomlMacroStore::new(root.join("macros.toml"));
        let mut semantic_macro = semantic_macro(MacroTrust::Untrusted);
        semantic_macro.binding = Some("Ctrl+Alt+m".to_owned());
        let document = MacroDocument {
            schema_version: MACRO_SCHEMA_VERSION,
            macros: vec![semantic_macro],
        };
        store.save(&document).unwrap();
        assert_eq!(store.load().unwrap(), document);

        let mislabeled = MacroDocument {
            schema_version: 1,
            macros: document.macros,
        };
        assert!(matches!(
            mislabeled.validate(),
            Err(MacroError::BindingRequiresSchemaTwo(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
