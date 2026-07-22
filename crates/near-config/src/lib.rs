//! Versioned layered TOML configuration with leaf-level provenance and reload rollback.

mod document_policy;
mod settings;

use std::{collections::BTreeMap, fmt, sync::Arc};

use serde::de::DeserializeOwned;
use thiserror::Error;

pub use document_policy::{EditorSettings, ResourceOpenPolicy, ViewerEncoding, ViewerSettings};
pub use settings::{
    ConfigurationCoordinator, CoordinatorError, NoopSettingsPersistence, SettingApplier,
    SettingApplyScope, SettingCandidate, SettingDescriptor, SettingPlatform,
    SettingPlatformAvailability, SettingProvenance, SettingState, SettingValue, SettingValueKind,
    SettingsPersistence,
};

pub const CONFIG_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfigLayerKind {
    BuiltIn,
    Platform,
    Plugin,
    User,
    Workspace,
    Cli,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigLayer {
    pub kind: ConfigLayerKind,
    pub priority: u32,
    pub source: String,
    pub text: String,
    pub trusted: bool,
}

impl ConfigLayer {
    pub fn new(kind: ConfigLayerKind, source: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            kind,
            priority: 0,
            source: source.into(),
            text: text.into(),
            trusted: !matches!(kind, ConfigLayerKind::Workspace),
        }
    }

    #[must_use]
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    #[must_use]
    pub fn trusted(mut self) -> Self {
        self.trusted = true;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigOrigin {
    pub layer: ConfigLayerKind,
    pub source: String,
    pub line: usize,
    pub column: usize,
    pub migrated_from: Option<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveConfig {
    value: toml::Value,
    origins: BTreeMap<String, ConfigOrigin>,
}

impl EffectiveConfig {
    pub fn value(&self) -> &toml::Value {
        &self.value
    }

    pub fn origin(&self, field: &str) -> Option<&ConfigOrigin> {
        self.origins.get(field)
    }

    pub fn origins(&self) -> &BTreeMap<String, ConfigOrigin> {
        &self.origins
    }

    /// Deserializes the effective merged document into an application schema.
    ///
    /// # Errors
    ///
    /// Returns a typed diagnostic when the effective value does not match the target schema.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, ConfigError> {
        self.value
            .clone()
            .try_into()
            .map_err(|error: toml::de::Error| ConfigError {
                source: "<effective configuration>".to_owned(),
                line: 1,
                column: 1,
                field: None,
                kind: ConfigErrorKind::InvalidValue,
                message: error.to_string(),
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigErrorKind {
    Parse,
    MissingSchema,
    InvalidSchema,
    UnsupportedSchema,
    MissingMigration,
    UntrustedWorkspace,
    InvalidValue,
    Migration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigError {
    pub source: String,
    pub line: usize,
    pub column: usize,
    pub field: Option<String>,
    pub kind: ConfigErrorKind,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}",
            self.source, self.line, self.column, self.message
        )?;
        if let Some(field) = &self.field {
            write!(formatter, " (field {field})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigError {}

type MigrationFn = dyn Fn(toml::Value) -> Result<toml::Value, String> + Send + Sync;

#[derive(Clone)]
pub struct ConfigMigration {
    pub from: u16,
    pub to: u16,
    migrate: Arc<MigrationFn>,
}

impl ConfigMigration {
    pub fn new(
        from: u16,
        to: u16,
        migrate: impl Fn(toml::Value) -> Result<toml::Value, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            from,
            to,
            migrate: Arc::new(migrate),
        }
    }
}

#[derive(Clone)]
pub struct ConfigEngine {
    schema_version: u16,
    migrations: BTreeMap<u16, ConfigMigration>,
}

impl Default for ConfigEngine {
    fn default() -> Self {
        Self::new(CONFIG_SCHEMA_VERSION)
    }
}

impl ConfigEngine {
    pub fn new(schema_version: u16) -> Self {
        Self {
            schema_version,
            migrations: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_migration(mut self, migration: ConfigMigration) -> Self {
        self.migrations.insert(migration.from, migration);
        self
    }

    /// Resolves all configuration layers in normative precedence order.
    ///
    /// # Errors
    ///
    /// Returns parse, trust, schema, migration, or value diagnostics with source positions.
    pub fn resolve(
        &self,
        layers: impl IntoIterator<Item = ConfigLayer>,
    ) -> Result<EffectiveConfig, ConfigError> {
        let mut layers = layers.into_iter().collect::<Vec<_>>();
        layers.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then(left.priority.cmp(&right.priority))
                .then(left.source.cmp(&right.source))
        });
        let mut effective = toml::Value::Table(toml::Table::new());
        let mut origins = BTreeMap::new();
        for layer in layers {
            if matches!(layer.kind, ConfigLayerKind::Workspace) && !layer.trusted {
                return Err(ConfigError {
                    source: layer.source,
                    line: 1,
                    column: 1,
                    field: None,
                    kind: ConfigErrorKind::UntrustedWorkspace,
                    message: "workspace configuration is not trusted".to_owned(),
                });
            }
            let positions = assignment_positions(&layer.text);
            let (value, migrated_from) = self.parse_and_migrate(&layer)?;
            merge_value(
                &mut effective,
                value,
                "",
                &layer,
                &positions,
                migrated_from,
                &mut origins,
            );
        }
        Ok(EffectiveConfig {
            value: effective,
            origins,
        })
    }

    fn parse_and_migrate(
        &self,
        layer: &ConfigLayer,
    ) -> Result<(toml::Value, Option<u16>), ConfigError> {
        let mut value = toml::from_str::<toml::Value>(&layer.text).map_err(|error| {
            let (line, column) = error
                .span()
                .map_or((1, 1), |span| line_column(&layer.text, span.start));
            ConfigError {
                source: layer.source.clone(),
                line,
                column,
                field: None,
                kind: ConfigErrorKind::Parse,
                message: error.to_string(),
            }
        })?;
        let schema = value
            .get("schema")
            .ok_or_else(|| schema_error(layer, ConfigErrorKind::MissingSchema, "missing schema"))?
            .as_integer()
            .ok_or_else(|| {
                schema_error(
                    layer,
                    ConfigErrorKind::InvalidSchema,
                    "schema must be an integer",
                )
            })?;
        let mut schema = u16::try_from(schema).map_err(|_| {
            schema_error(
                layer,
                ConfigErrorKind::InvalidSchema,
                "schema is outside the supported integer range",
            )
        })?;
        if schema > self.schema_version {
            return Err(schema_error(
                layer,
                ConfigErrorKind::UnsupportedSchema,
                &format!(
                    "schema {schema} is newer than supported schema {}",
                    self.schema_version
                ),
            ));
        }
        let migrated_from = (schema < self.schema_version).then_some(schema);
        while schema < self.schema_version {
            let migration = self.migrations.get(&schema).ok_or_else(|| {
                schema_error(
                    layer,
                    ConfigErrorKind::MissingMigration,
                    &format!("no migration registered from schema {schema}"),
                )
            })?;
            if migration.to <= schema {
                return Err(schema_error(
                    layer,
                    ConfigErrorKind::Migration,
                    "migration must advance the schema version",
                ));
            }
            value = (migration.migrate)(value).map_err(|message| ConfigError {
                source: layer.source.clone(),
                line: 1,
                column: 1,
                field: Some("schema".to_owned()),
                kind: ConfigErrorKind::Migration,
                message,
            })?;
            schema = migration.to;
            value["schema"] = toml::Value::Integer(i64::from(schema));
        }
        Ok((value, migrated_from))
    }
}

pub struct ConfigManager {
    engine: ConfigEngine,
    current: EffectiveConfig,
    last_error: Option<ConfigError>,
}

impl ConfigManager {
    /// Creates a manager from an initially valid layer set.
    ///
    /// # Errors
    ///
    /// Returns the initial resolution error when no valid configuration can be established.
    pub fn new(
        engine: ConfigEngine,
        layers: impl IntoIterator<Item = ConfigLayer>,
    ) -> Result<Self, ConfigError> {
        let current = engine.resolve(layers)?;
        Ok(Self {
            engine,
            current,
            last_error: None,
        })
    }

    pub fn current(&self) -> &EffectiveConfig {
        &self.current
    }

    pub fn last_error(&self) -> Option<&ConfigError> {
        self.last_error.as_ref()
    }

    /// Atomically replaces the effective configuration only when all new layers are valid.
    ///
    /// # Errors
    ///
    /// Returns a reload diagnostic while preserving the previous effective configuration.
    pub fn reload(
        &mut self,
        layers: impl IntoIterator<Item = ConfigLayer>,
    ) -> Result<&EffectiveConfig, ReloadError> {
        match self.engine.resolve(layers) {
            Ok(current) => {
                self.current = current;
                self.last_error = None;
                Ok(&self.current)
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                Err(ReloadError {
                    error,
                    retained_last_valid: true,
                })
            }
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("configuration reload failed; last valid configuration retained: {error}")]
pub struct ReloadError {
    pub error: ConfigError,
    pub retained_last_valid: bool,
}

fn schema_error(layer: &ConfigLayer, kind: ConfigErrorKind, message: &str) -> ConfigError {
    let positions = assignment_positions(&layer.text);
    let (line, column) = positions.get("schema").copied().unwrap_or((1, 1));
    ConfigError {
        source: layer.source.clone(),
        line,
        column,
        field: Some("schema".to_owned()),
        kind,
        message: message.to_owned(),
    }
}

fn merge_value(
    target: &mut toml::Value,
    incoming: toml::Value,
    path: &str,
    layer: &ConfigLayer,
    positions: &BTreeMap<String, (usize, usize)>,
    migrated_from: Option<u16>,
    origins: &mut BTreeMap<String, ConfigOrigin>,
) {
    let incoming = match incoming {
        toml::Value::Table(incoming) => {
            if let Some(target_table) = target.as_table_mut() {
                for (key, value) in incoming {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    if let Some(existing) = target_table.get_mut(&key) {
                        merge_value(
                            existing,
                            value,
                            &child,
                            layer,
                            positions,
                            migrated_from,
                            origins,
                        );
                    } else {
                        target_table.insert(key.clone(), value);
                        record_origins(
                            target_table.get(&key).unwrap(),
                            &child,
                            layer,
                            positions,
                            migrated_from,
                            origins,
                        );
                    }
                }
                return;
            }
            toml::Value::Table(incoming)
        }
        incoming => incoming,
    };
    *target = incoming;
    origins.retain(|field, _| field != path && !field.starts_with(&format!("{path}.")));
    record_origins(target, path, layer, positions, migrated_from, origins);
}

fn record_origins(
    value: &toml::Value,
    path: &str,
    layer: &ConfigLayer,
    positions: &BTreeMap<String, (usize, usize)>,
    migrated_from: Option<u16>,
    origins: &mut BTreeMap<String, ConfigOrigin>,
) {
    if let toml::Value::Table(table) = value {
        for (key, value) in table {
            let child = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            record_origins(value, &child, layer, positions, migrated_from, origins);
        }
        return;
    }
    let (line, column) = origin_position(path, positions);
    origins.insert(
        path.to_owned(),
        ConfigOrigin {
            layer: layer.kind,
            source: layer.source.clone(),
            line,
            column,
            migrated_from,
        },
    );
}

fn origin_position(path: &str, positions: &BTreeMap<String, (usize, usize)>) -> (usize, usize) {
    if let Some(position) = positions.get(path) {
        return *position;
    }
    let mut candidate = path;
    while let Some((parent, _)) = candidate.rsplit_once('.') {
        if let Some(position) = positions.get(parent) {
            return *position;
        }
        candidate = parent;
    }
    (1, 1)
}

fn assignment_positions(text: &str) -> BTreeMap<String, (usize, usize)> {
    let mut positions = BTreeMap::new();
    let mut section = String::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let column = line.len().saturating_sub(trimmed.len()) + 1;
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            trimmed[2..trimmed.len() - 2]
                .trim()
                .clone_into(&mut section);
            positions
                .entry(section.clone())
                .or_insert((index + 1, column));
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            trimmed[1..trimmed.len() - 1]
                .trim()
                .clone_into(&mut section);
            positions
                .entry(section.clone())
                .or_insert((index + 1, column));
        } else if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim().trim_matches('"');
            if !key.is_empty() {
                let path = if section.is_empty() {
                    key.to_owned()
                } else {
                    format!("{section}.{key}")
                };
                positions.insert(path, (index + 1, column));
            }
        }
    }
    positions
}

fn line_column(text: &str, byte: usize) -> (usize, usize) {
    let prefix = &text[..byte.min(text.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len() + 1, |(_, tail)| tail.len() + 1);
    (line, column)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    fn layer(kind: ConfigLayerKind, source: &str, value: &str) -> ConfigLayer {
        ConfigLayer::new(kind, source, format!("schema = 1\nvalue = {value}\n"))
    }

    #[test]
    fn all_six_layers_resolve_deterministically_with_winning_origin() {
        let layers = vec![
            layer(ConfigLayerKind::Cli, "cli", "6"),
            layer(ConfigLayerKind::BuiltIn, "builtin", "1"),
            layer(ConfigLayerKind::Workspace, "workspace", "5").trusted(),
            layer(ConfigLayerKind::Plugin, "plugin", "3"),
            layer(ConfigLayerKind::User, "user", "4"),
            layer(ConfigLayerKind::Platform, "platform", "2"),
        ];
        let effective = ConfigEngine::default().resolve(layers).unwrap();
        assert_eq!(effective.value()["value"].as_integer(), Some(6));
        assert_eq!(effective.origin("value").unwrap().source, "cli");
        assert_eq!(effective.origin("value").unwrap().line, 2);
    }

    #[test]
    fn plugin_priority_and_source_name_break_ties_deterministically() {
        let effective = ConfigEngine::default()
            .resolve([
                layer(ConfigLayerKind::Plugin, "z-plugin", "2").with_priority(5),
                layer(ConfigLayerKind::Plugin, "a-plugin", "1").with_priority(5),
            ])
            .unwrap();
        assert_eq!(effective.value()["value"].as_integer(), Some(2));
        assert_eq!(effective.origin("value").unwrap().source, "z-plugin");
    }

    #[test]
    fn invalid_hot_reload_retains_last_valid_configuration_and_position() {
        let mut manager = ConfigManager::new(
            ConfigEngine::default(),
            [layer(ConfigLayerKind::BuiltIn, "builtin.toml", "1")],
        )
        .unwrap();
        let error = manager
            .reload([ConfigLayer::new(
                ConfigLayerKind::User,
                "user.toml",
                "schema = 1\nvalue = [\n",
            )])
            .unwrap_err();
        assert!(error.retained_last_valid);
        assert_eq!(manager.current().value()["value"].as_integer(), Some(1));
        assert_eq!(error.error.source, "user.toml");
        assert_eq!(error.error.line, 2);
    }

    #[test]
    fn untrusted_workspace_layer_is_rejected() {
        let error = ConfigEngine::default()
            .resolve([layer(ConfigLayerKind::Workspace, ".near/config.toml", "5")])
            .unwrap_err();
        assert_eq!(error.kind, ConfigErrorKind::UntrustedWorkspace);
    }

    #[test]
    fn registered_migration_updates_old_fixture_and_records_origin() {
        let engine =
            ConfigEngine::default().with_migration(ConfigMigration::new(0, 1, |mut value| {
                let old = value
                    .get("old_name")
                    .cloned()
                    .ok_or_else(|| "old_name is required".to_owned())?;
                let table = value.as_table_mut().unwrap();
                table.remove("old_name");
                table.insert("new_name".to_owned(), old);
                Ok(value)
            }));
        let effective = engine
            .resolve([ConfigLayer::new(
                ConfigLayerKind::User,
                "v0.toml",
                include_str!("../tests/fixtures/config-v0.toml"),
            )])
            .unwrap();
        assert_eq!(effective.value()["new_name"].as_str(), Some("migrated"));
        assert_eq!(effective.origin("new_name").unwrap().migrated_from, Some(0));
    }

    #[derive(Deserialize)]
    struct TypedConfig {
        value: i64,
    }

    #[test]
    fn effective_document_deserializes_into_application_schema() {
        let effective = ConfigEngine::default()
            .resolve([layer(ConfigLayerKind::BuiltIn, "builtin", "42")])
            .unwrap();
        assert_eq!(effective.deserialize::<TypedConfig>().unwrap().value, 42);
    }
}
