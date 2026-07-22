use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::{Duration, Instant},
};

use near_core::{CommandId, CommandInvocation, CommandValue, ContextId};
use near_terminal::{Key, KeyKind, KeyStroke, Modifiers};
use serde::Deserialize;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingOrigin {
    pub source: String,
    pub context: ContextId,
    pub ordinal: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyBinding {
    pub sequence: Vec<KeyStroke>,
    pub invocation: CommandInvocation,
    pub repeatable: bool,
    pub description: Option<String>,
    pub function_hint: Option<u8>,
    pub when: Option<String>,
    pub origin: BindingOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingConflict {
    pub sequence: Vec<KeyStroke>,
    pub first: BindingOrigin,
    pub second: BindingOrigin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeymapSettings {
    pub sequence_timeout: Duration,
    pub show_pending_sequence: bool,
    pub prefer_physical_keys: bool,
}

impl Default for KeymapSettings {
    fn default() -> Self {
        Self {
            sequence_timeout: Duration::from_millis(700),
            show_pending_sequence: true,
            prefer_physical_keys: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolveResult {
    NoMatch,
    Pending {
        sequence: Vec<KeyStroke>,
        continuations: Vec<KeyStroke>,
    },
    Matched(CommandInvocation),
}

#[derive(Debug, Error)]
pub enum KeymapError {
    #[error("invalid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid key '{key}': {reason}")]
    InvalidKey { key: String, reason: String },
    #[error("context '{0}' appears more than once")]
    DuplicateContext(String),
    #[error("context '{context}' inherits unknown context '{parent}'")]
    UnknownParent { context: String, parent: String },
    #[error("context inheritance cycle: {0}")]
    InheritanceCycle(String),
    #[error("unsupported command argument value for '{0}'")]
    UnsupportedArgument(String),
    #[error("invalid keymap settings: {0}")]
    InvalidSettings(String),
    #[error("cannot serialize keymap: {0}")]
    Serialize(String),
}

#[derive(Clone, Debug)]
struct KeyRemoval {
    sequence: Vec<KeyStroke>,
}

#[derive(Clone, Debug, Default)]
struct ContextBindings {
    inherits: Vec<ContextId>,
    bindings: Vec<KeyBinding>,
    removals: Vec<KeyRemoval>,
}

#[derive(Default)]
struct BindingTrieNode {
    invocation: Option<CommandInvocation>,
    repeatable: bool,
    children: HashMap<KeyStroke, BindingTrieNode>,
}

impl BindingTrieNode {
    fn from_bindings(bindings: &[&KeyBinding]) -> Self {
        let mut root = Self::default();
        for binding in bindings {
            let mut node = &mut root;
            for stroke in &binding.sequence {
                node = node.children.entry(stroke.clone()).or_default();
            }
            node.invocation = Some(binding.invocation.clone());
            node.repeatable = binding.repeatable;
        }
        root
    }

    fn find(&self, sequence: &[KeyStroke]) -> Option<&Self> {
        let mut node = self;
        for stroke in sequence {
            node = node.children.get(stroke)?;
        }
        Some(node)
    }
}

#[derive(Clone, Debug)]
pub struct Keymap {
    settings: KeymapSettings,
    contexts: BTreeMap<ContextId, ContextBindings>,
    conflicts: Vec<BindingConflict>,
    pending: Vec<KeyStroke>,
    pending_since: Option<Duration>,
    pending_exact: Option<CommandInvocation>,
    clock_origin: Instant,
}

impl Keymap {
    /// Parses a complete keymap document using an in-memory source label.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid TOML, contexts, inheritance, keys, or arguments.
    pub fn from_toml(source: &str) -> Result<Self, KeymapError> {
        Self::from_toml_named("<memory>", source)
    }

    /// Parses a complete keymap document and retains its configuration origin.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid TOML, contexts, inheritance, keys, or arguments.
    pub fn from_toml_named(source_name: &str, source: &str) -> Result<Self, KeymapError> {
        let file: KeymapFile = toml::from_str(source)?;
        let settings = KeymapSettings {
            sequence_timeout: Duration::from_millis(file.settings.sequence_timeout_ms),
            show_pending_sequence: file.settings.show_pending_sequence,
            prefer_physical_keys: file.settings.prefer_physical_keys,
        };
        if settings.prefer_physical_keys {
            return Err(KeymapError::InvalidSettings(
                "physical key identity is unavailable in the terminal event model".to_owned(),
            ));
        }
        let mut contexts = BTreeMap::new();
        let mut conflicts = Vec::new();
        for context in file.context {
            let id = ContextId::from(context.id.as_str());
            if contexts.contains_key(&id) {
                return Err(KeymapError::DuplicateContext(context.id));
            }
            let mut bindings: Vec<KeyBinding> = Vec::new();
            for (ordinal, binding) in context.bindings.into_iter().enumerate() {
                let mut sequences = vec![parse_sequence(binding.on)?];
                sequences.extend(
                    binding
                        .aliases
                        .into_iter()
                        .map(|alias| parse_key_stroke(&alias).map(|stroke| vec![stroke]))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                let (command, arguments) = binding.run.into_parts()?;
                let function_hint = binding.hint.map(|hint| hint.slot);
                for (alias_index, sequence) in sequences.into_iter().enumerate() {
                    let origin = BindingOrigin {
                        source: source_name.to_owned(),
                        context: id.clone(),
                        ordinal: ordinal.saturating_mul(1_000).saturating_add(alias_index),
                    };
                    if let Some(existing) = bindings.iter().find(|item| item.sequence == sequence) {
                        conflicts.push(BindingConflict {
                            sequence: sequence.clone(),
                            first: existing.origin.clone(),
                            second: origin.clone(),
                        });
                    }
                    bindings.push(KeyBinding {
                        repeatable: binding.repeatable.unwrap_or_else(|| {
                            sequence.len() == 1 && is_repeatable_navigation_key(&sequence[0].key)
                        }),
                        sequence,
                        invocation: CommandInvocation {
                            id: CommandId::from(command.clone()),
                            arguments: arguments.clone(),
                        },
                        description: binding.description.clone(),
                        function_hint,
                        when: binding.when.clone(),
                        origin,
                    });
                }
            }
            let removals = context
                .removals
                .into_iter()
                .map(|removal| parse_sequence(removal.on).map(|sequence| KeyRemoval { sequence }))
                .collect::<Result<Vec<_>, _>>()?;
            contexts.insert(
                id,
                ContextBindings {
                    inherits: context.inherits.into_iter().map(ContextId::from).collect(),
                    bindings,
                    removals,
                },
            );
        }
        validate_inheritance(&contexts)?;
        Ok(Self {
            settings,
            contexts,
            conflicts,
            pending: Vec::new(),
            pending_since: None,
            pending_exact: None,
            clock_origin: Instant::now(),
        })
    }

    pub fn settings(&self) -> &KeymapSettings {
        &self.settings
    }

    /// Rewrites only the settings table while preserving all bindings.
    ///
    /// # Errors
    ///
    /// Returns an error when the source is not a valid keymap document or cannot be serialized.
    pub fn rewrite_settings_toml(
        source: &str,
        settings: KeymapSettings,
    ) -> Result<String, KeymapError> {
        let mut document = source.parse::<toml::Table>()?;
        let table = document
            .entry("settings")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| KeymapError::InvalidSettings("settings must be a table".to_owned()))?;
        table.insert(
            "sequence_timeout_ms".to_owned(),
            toml::Value::Integer(
                i64::try_from(settings.sequence_timeout.as_millis()).unwrap_or(i64::MAX),
            ),
        );
        table.insert(
            "show_pending_sequence".to_owned(),
            toml::Value::Boolean(settings.show_pending_sequence),
        );
        table.insert(
            "prefer_physical_keys".to_owned(),
            toml::Value::Boolean(settings.prefer_physical_keys),
        );
        let source = toml::to_string_pretty(&document)
            .map_err(|error| KeymapError::Serialize(error.to_string()))?;
        Self::from_toml(&source)?;
        Ok(source)
    }

    /// Replaces this keymap with a newly parsed in-memory document.
    ///
    /// # Errors
    ///
    /// Returns an error without changing the current keymap when parsing or validation fails.
    pub fn reload_from_toml(&mut self, source: &str) -> Result<(), KeymapError> {
        self.reload_from_toml_named("<memory>", source)
    }

    /// Replaces this keymap while retaining the new configuration origin.
    ///
    /// # Errors
    ///
    /// Returns an error without changing the current keymap when parsing or validation fails.
    pub fn reload_from_toml_named(
        &mut self,
        source_name: &str,
        source: &str,
    ) -> Result<(), KeymapError> {
        let replacement = Self::from_toml_named(source_name, source)?;
        *self = replacement;
        Ok(())
    }

    pub fn conflicts(&self) -> &[BindingConflict] {
        &self.conflicts
    }

    pub fn resolve(&mut self, active_contexts: &[ContextId], stroke: KeyStroke) -> ResolveResult {
        self.resolve_at(active_contexts, stroke, self.clock_origin.elapsed())
    }

    pub fn resolve_at(
        &mut self,
        active_contexts: &[ContextId],
        mut stroke: KeyStroke,
        now: Duration,
    ) -> ResolveResult {
        if stroke.kind == KeyKind::Release {
            return ResolveResult::NoMatch;
        }
        if stroke.kind == KeyKind::Repeat {
            if !self.pending.is_empty() {
                return ResolveResult::NoMatch;
            }
            stroke.kind = KeyKind::Press;
            let bindings = self.effective_bindings(active_contexts);
            let trie = BindingTrieNode::from_bindings(&bindings);
            let Some(node) = trie.find(std::slice::from_ref(&stroke)) else {
                return ResolveResult::NoMatch;
            };
            return node
                .invocation
                .clone()
                .filter(|_| node.repeatable)
                .map_or(ResolveResult::NoMatch, ResolveResult::Matched);
        }
        if self.pending_expired(now) {
            self.clear_pending();
        }
        self.pending_since.get_or_insert(now);
        self.pending.push(stroke);
        let bindings = self.effective_bindings(active_contexts);
        let trie = BindingTrieNode::from_bindings(&bindings);
        let Some(node) = trie.find(&self.pending) else {
            if self.pending.len() > 1
                && let Some(last) = self.pending.pop()
            {
                self.clear_pending();
                return self.resolve_at(active_contexts, last, now);
            }
            self.clear_pending();
            return ResolveResult::NoMatch;
        };
        let exact = node.invocation.clone();
        let mut continuations: Vec<_> = node.children.keys().cloned().collect();
        continuations.sort_by_key(format_key_stroke);
        if !continuations.is_empty() {
            self.pending_exact = exact;
            return ResolveResult::Pending {
                sequence: self.pending.clone(),
                continuations,
            };
        }
        if let Some(invocation) = exact {
            self.clear_pending();
            return ResolveResult::Matched(invocation);
        }
        self.clear_pending();
        ResolveResult::NoMatch
    }

    pub fn expire_pending(&mut self) -> ResolveResult {
        self.expire_pending_at(self.clock_origin.elapsed())
    }

    pub fn expire_pending_at(&mut self, now: Duration) -> ResolveResult {
        if !self.pending_expired(now) {
            return ResolveResult::NoMatch;
        }
        let invocation = self.pending_exact.take();
        self.clear_pending();
        invocation.map_or(ResolveResult::NoMatch, ResolveResult::Matched)
    }

    pub fn time_until_pending_timeout(&self) -> Option<Duration> {
        self.pending_since.map(|started| {
            self.settings
                .sequence_timeout
                .saturating_sub(self.clock_origin.elapsed().saturating_sub(started))
        })
    }

    pub fn bindings_for(&self, active_contexts: &[ContextId]) -> Vec<&KeyBinding> {
        self.effective_bindings(active_contexts)
    }

    pub fn bindings_for_command(
        &self,
        active_contexts: &[ContextId],
        command: &CommandId,
    ) -> Vec<&KeyBinding> {
        self.effective_bindings(active_contexts)
            .into_iter()
            .filter(|binding| &binding.invocation.id == command)
            .collect()
    }

    pub fn function_hints_for_modifiers(
        &self,
        active_contexts: &[ContextId],
        modifiers: Modifiers,
    ) -> Vec<(u8, &KeyBinding)> {
        let mut hints = self
            .effective_bindings(active_contexts)
            .into_iter()
            .filter_map(|binding| {
                let slot = binding.function_hint?;
                let [stroke] = binding.sequence.as_slice() else {
                    return None;
                };
                (stroke.key == Key::Function(slot) && stroke.modifiers == modifiers)
                    .then_some((slot, binding))
            })
            .collect::<Vec<_>>();
        hints.sort_by_key(|(slot, _)| *slot);
        hints
    }

    pub fn pending_sequence(&self) -> &[KeyStroke] {
        &self.pending
    }

    pub fn pending_continuations(&self, active_contexts: &[ContextId]) -> Vec<KeyStroke> {
        let bindings = self.effective_bindings(active_contexts);
        let trie = BindingTrieNode::from_bindings(&bindings);
        let Some(node) = trie.find(&self.pending) else {
            return Vec::new();
        };
        let mut continuations: Vec<_> = node.children.keys().cloned().collect();
        continuations.sort_by_key(format_key_stroke);
        continuations
    }

    fn pending_expired(&self, now: Duration) -> bool {
        self.pending_since
            .is_some_and(|started| now.saturating_sub(started) >= self.settings.sequence_timeout)
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
        self.pending_exact = None;
    }

    fn effective_bindings(&self, active_contexts: &[ContextId]) -> Vec<&KeyBinding> {
        let mut result = Vec::new();
        let mut applied = HashSet::new();
        for context in active_contexts.iter().rev() {
            self.apply_context(context, &mut applied, &mut result);
        }
        result
    }

    fn apply_context<'a>(
        &'a self,
        context: &ContextId,
        applied: &mut HashSet<ContextId>,
        result: &mut Vec<&'a KeyBinding>,
    ) {
        if !applied.insert(context.clone()) {
            return;
        }
        let Some(bindings) = self.contexts.get(context) else {
            return;
        };
        for parent in &bindings.inherits {
            self.apply_context(parent, applied, result);
        }
        for removal in &bindings.removals {
            result.retain(|binding| binding.sequence != removal.sequence);
        }
        for binding in &bindings.bindings {
            result.retain(|existing| existing.sequence != binding.sequence);
            result.push(binding);
        }
    }
}

pub fn format_key_stroke(stroke: &KeyStroke) -> String {
    let mut parts = Vec::new();
    if stroke.modifiers.control {
        parts.push("Ctrl".to_owned());
    }
    if stroke.modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if stroke.modifiers.shift {
        parts.push("Shift".to_owned());
    }
    if stroke.modifiers.super_key {
        parts.push("Cmd".to_owned());
    }
    parts.push(match stroke.key {
        Key::Character(' ') => "Space".to_owned(),
        Key::Character(character) => character.to_string(),
        Key::Enter => "Enter".to_owned(),
        Key::Escape => "Esc".to_owned(),
        Key::Backspace => "Backspace".to_owned(),
        Key::Tab => "Tab".to_owned(),
        Key::BackTab => "BackTab".to_owned(),
        Key::Left => "Left".to_owned(),
        Key::Right => "Right".to_owned(),
        Key::Up => "Up".to_owned(),
        Key::Down => "Down".to_owned(),
        Key::Home => "Home".to_owned(),
        Key::End => "End".to_owned(),
        Key::PageUp => "PageUp".to_owned(),
        Key::PageDown => "PageDown".to_owned(),
        Key::Insert => "Insert".to_owned(),
        Key::Delete => "Delete".to_owned(),
        Key::Function(number) => format!("F{number}"),
        Key::Null => "Null".to_owned(),
        _ => "Unknown".to_owned(),
    });
    parts.join("+")
}

pub fn format_key_sequence(sequence: &[KeyStroke]) -> String {
    sequence
        .iter()
        .map(format_key_stroke)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses Near's human-readable key notation.
///
/// # Errors
///
/// Returns an error for unknown modifiers, key names, or malformed function keys.
pub fn parse_key_stroke(source: &str) -> Result<KeyStroke, KeymapError> {
    let parts: Vec<_> = source.split('+').collect();
    let key_name = parts.last().copied().unwrap_or_default();
    let mut modifiers = Modifiers::default();
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.control = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "super" | "cmd" | "command" => modifiers.super_key = true,
            _ => {
                return Err(KeymapError::InvalidKey {
                    key: source.to_owned(),
                    reason: format!("unknown modifier {modifier}"),
                });
            }
        }
    }
    let lower = key_name.to_ascii_lowercase();
    let key = match lower.as_str() {
        "enter" | "ret" => Key::Enter,
        "esc" | "escape" => Key::Escape,
        "backspace" => Key::Backspace,
        "tab" => Key::Tab,
        "backtab" => Key::BackTab,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "insert" | "ins" => Key::Insert,
        "delete" | "del" => Key::Delete,
        "space" => Key::Character(' '),
        _ if lower.len() > 1 && lower.starts_with('f') => {
            let number: u8 = lower[1..].parse().map_err(|_| KeymapError::InvalidKey {
                key: source.to_owned(),
                reason: "function key must be F1 through F24".to_owned(),
            })?;
            if !(1..=24).contains(&number) {
                return Err(KeymapError::InvalidKey {
                    key: source.to_owned(),
                    reason: "function key must be F1 through F24".to_owned(),
                });
            }
            Key::Function(number)
        }
        _ => {
            let mut characters = key_name.chars();
            let character = characters.next().ok_or_else(|| KeymapError::InvalidKey {
                key: source.to_owned(),
                reason: "missing key".to_owned(),
            })?;
            if characters.next().is_some() {
                return Err(KeymapError::InvalidKey {
                    key: source.to_owned(),
                    reason: "unknown key name".to_owned(),
                });
            }
            Key::Character(character.to_ascii_lowercase())
        }
    };
    Ok(KeyStroke {
        key,
        modifiers,
        kind: KeyKind::Press,
    })
}

fn parse_sequence(spec: KeySpec) -> Result<Vec<KeyStroke>, KeymapError> {
    spec.into_keys()
        .into_iter()
        .map(|key| parse_key_stroke(&key))
        .collect()
}

fn validate_inheritance(
    contexts: &BTreeMap<ContextId, ContextBindings>,
) -> Result<(), KeymapError> {
    for (id, context) in contexts {
        for parent in &context.inherits {
            if !contexts.contains_key(parent) {
                return Err(KeymapError::UnknownParent {
                    context: id.to_string(),
                    parent: parent.to_string(),
                });
            }
        }
    }
    let mut visited = HashSet::new();
    for id in contexts.keys() {
        visit_context(id, contexts, &mut Vec::new(), &mut visited)?;
    }
    Ok(())
}

fn visit_context(
    id: &ContextId,
    contexts: &BTreeMap<ContextId, ContextBindings>,
    visiting: &mut Vec<ContextId>,
    visited: &mut HashSet<ContextId>,
) -> Result<(), KeymapError> {
    if let Some(index) = visiting.iter().position(|item| item == id) {
        let mut cycle = visiting[index..]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        cycle.push(id.to_string());
        return Err(KeymapError::InheritanceCycle(cycle.join(" -> ")));
    }
    if !visited.insert(id.clone()) {
        return Ok(());
    }
    visiting.push(id.clone());
    if let Some(context) = contexts.get(id) {
        for parent in &context.inherits {
            visit_context(parent, contexts, visiting, visited)?;
        }
    }
    visiting.pop();
    Ok(())
}

#[derive(Default, Deserialize)]
struct KeymapFile {
    #[serde(default)]
    settings: SettingsFile,
    #[serde(default)]
    context: Vec<ContextFile>,
}

#[derive(Deserialize)]
struct SettingsFile {
    #[serde(default = "default_sequence_timeout")]
    sequence_timeout_ms: u64,
    #[serde(default = "default_true")]
    show_pending_sequence: bool,
    #[serde(default)]
    prefer_physical_keys: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            sequence_timeout_ms: default_sequence_timeout(),
            show_pending_sequence: true,
            prefer_physical_keys: false,
        }
    }
}

const fn default_sequence_timeout() -> u64 {
    700
}

const fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct ContextFile {
    id: String,
    #[serde(default)]
    inherits: Vec<String>,
    #[serde(default, rename = "bindings")]
    bindings: Vec<BindingFile>,
    #[serde(default, rename = "removals")]
    removals: Vec<RemovalFile>,
    #[serde(default)]
    #[allow(dead_code)]
    optional: bool,
}

#[derive(Deserialize)]
struct BindingFile {
    on: KeySpec,
    #[serde(default)]
    aliases: Vec<String>,
    run: RunSpec,
    repeatable: Option<bool>,
    description: Option<String>,
    hint: Option<HintFile>,
    when: Option<String>,
}

fn is_repeatable_navigation_key(key: &Key) -> bool {
    matches!(
        key,
        Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
    )
}

#[derive(Deserialize)]
struct RemovalFile {
    on: KeySpec,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum KeySpec {
    One(String),
    Sequence(Vec<String>),
}

impl KeySpec {
    fn into_keys(self) -> Vec<String> {
        match self {
            Self::One(key) => vec![key],
            Self::Sequence(keys) => keys,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RunSpec {
    Command(String),
    Parameterized {
        command: String,
        #[serde(default)]
        args: BTreeMap<String, toml::Value>,
    },
}

impl RunSpec {
    fn into_parts(self) -> Result<(String, BTreeMap<String, CommandValue>), KeymapError> {
        match self {
            Self::Command(command) => Ok((command, BTreeMap::new())),
            Self::Parameterized { command, args } => {
                let arguments = args
                    .into_iter()
                    .map(|(key, value)| {
                        toml_value(value)
                            .map(|value| (key.clone(), value))
                            .ok_or(KeymapError::UnsupportedArgument(key))
                    })
                    .collect::<Result<_, _>>()?;
                Ok((command, arguments))
            }
        }
    }
}

fn toml_value(value: toml::Value) -> Option<CommandValue> {
    match value {
        toml::Value::String(value) => Some(CommandValue::String(value)),
        toml::Value::Integer(value) => Some(CommandValue::Integer(value)),
        toml::Value::Float(value) => Some(CommandValue::Float(value)),
        toml::Value::Boolean(value) => Some(CommandValue::Boolean(value)),
        toml::Value::Array(values) => values
            .into_iter()
            .map(toml_value)
            .collect::<Option<Vec<_>>>()
            .map(CommandValue::Array),
        toml::Value::Table(values) => values
            .into_iter()
            .map(|(key, value)| toml_value(value).map(|value| (key, value)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(CommandValue::Table),
        toml::Value::Datetime(_) => None,
    }
}

#[derive(Deserialize)]
struct HintFile {
    #[allow(dead_code)]
    group: String,
    slot: u8,
}
