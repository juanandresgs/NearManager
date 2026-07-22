use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::ConfigLayerKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingValue {
    Boolean(bool),
    Integer(i64),
    String(String),
    Strings(Vec<String>),
}

impl SettingValue {
    pub fn kind(&self) -> SettingValueKind {
        match self {
            Self::Boolean(_) => SettingValueKind::Boolean,
            Self::Integer(_) => SettingValueKind::Integer,
            Self::String(_) => SettingValueKind::String,
            Self::Strings(_) => SettingValueKind::Strings,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingValueKind {
    Boolean,
    Integer,
    String,
    Strings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingApplyScope {
    Live,
    NewSurface,
    Restart,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SettingPlatform {
    MacOs,
    Linux,
    Windows,
}

impl SettingPlatform {
    pub fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MacOs)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(windows) {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingPlatformAvailability {
    All,
    Only(BTreeSet<SettingPlatform>),
}

impl SettingPlatformAvailability {
    pub fn supports(&self, platform: SettingPlatform) -> bool {
        match self {
            Self::All => true,
            Self::Only(platforms) => platforms.contains(&platform),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingDescriptor {
    pub id: String,
    pub document: String,
    pub path: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub advanced: bool,
    pub value_kind: SettingValueKind,
    pub default_value: SettingValue,
    pub apply_scope: SettingApplyScope,
    pub apply_order: u32,
    pub availability: SettingPlatformAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingProvenance {
    pub layer: ConfigLayerKind,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingState {
    pub value: SettingValue,
    pub provenance: SettingProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingCandidate {
    pub id: String,
    pub value: SettingValue,
}

pub trait SettingApplier: Send {
    /// Validates a candidate without changing runtime state.
    ///
    /// # Errors
    ///
    /// Returns a descriptor-specific validation failure.
    fn validate(&self, _value: &SettingValue) -> Result<(), String> {
        Ok(())
    }

    /// Applies a validated candidate to runtime state.
    ///
    /// # Errors
    ///
    /// Returns an apply failure that triggers coordinator rollback.
    fn apply(&mut self, _previous: &SettingValue, _candidate: &SettingValue) -> Result<(), String> {
        Ok(())
    }

    fn rollback(&mut self, _candidate: &SettingValue, _previous: &SettingValue) {}
}

impl SettingApplier for () {}

pub trait SettingsPersistence {
    /// Atomically persists a complete candidate settings snapshot.
    ///
    /// # Errors
    ///
    /// Returns storage or serialization failures.
    fn persist(&mut self, values: &BTreeMap<String, SettingValue>) -> Result<(), String>;
}

#[derive(Default)]
pub struct NoopSettingsPersistence;

impl SettingsPersistence for NoopSettingsPersistence {
    fn persist(&mut self, _values: &BTreeMap<String, SettingValue>) -> Result<(), String> {
        Ok(())
    }
}

struct RegisteredSetting {
    descriptor: SettingDescriptor,
    state: SettingState,
    applier: Box<dyn SettingApplier>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CoordinatorError {
    #[error("setting is already registered: {0}")]
    Duplicate(String),
    #[error("unknown setting: {0}")]
    Unknown(String),
    #[error("setting {id} expects {expected:?}, received {actual:?}")]
    TypeMismatch {
        id: String,
        expected: SettingValueKind,
        actual: SettingValueKind,
    },
    #[error("setting is unavailable on this platform: {0}")]
    Unavailable(String),
    #[error("setting validation failed for {id}: {message}")]
    Validation { id: String, message: String },
    #[error("settings persistence failed: {0}")]
    Persistence(String),
    #[error("setting apply failed for {id}: {message}")]
    Apply { id: String, message: String },
}

pub struct ConfigurationCoordinator<P = NoopSettingsPersistence> {
    settings: BTreeMap<String, RegisteredSetting>,
    persistence: P,
    platform: SettingPlatform,
    last_error: Option<CoordinatorError>,
}

impl ConfigurationCoordinator<NoopSettingsPersistence> {
    pub fn new(platform: SettingPlatform) -> Self {
        Self::with_persistence(platform, NoopSettingsPersistence)
    }
}

impl<P: SettingsPersistence> ConfigurationCoordinator<P> {
    pub fn with_persistence(platform: SettingPlatform, persistence: P) -> Self {
        Self {
            settings: BTreeMap::new(),
            persistence,
            platform,
            last_error: None,
        }
    }

    /// Registers a typed setting and validates its initial state.
    ///
    /// # Errors
    ///
    /// Returns duplicate, type, or descriptor-specific validation failures.
    pub fn register(
        &mut self,
        descriptor: SettingDescriptor,
        value: SettingValue,
        provenance: SettingProvenance,
        applier: impl SettingApplier + 'static,
    ) -> Result<(), CoordinatorError> {
        if self.settings.contains_key(&descriptor.id) {
            return Err(CoordinatorError::Duplicate(descriptor.id));
        }
        validate_type(&descriptor, &descriptor.default_value)?;
        validate_type(&descriptor, &value)?;
        applier
            .validate(&value)
            .map_err(|message| CoordinatorError::Validation {
                id: descriptor.id.clone(),
                message,
            })?;
        self.settings.insert(
            descriptor.id.clone(),
            RegisteredSetting {
                descriptor,
                state: SettingState { value, provenance },
                applier: Box::new(applier),
            },
        );
        Ok(())
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &SettingDescriptor> {
        self.settings.values().map(|setting| &setting.descriptor)
    }

    pub fn state(&self, id: &str) -> Option<&SettingState> {
        self.settings.get(id).map(|setting| &setting.state)
    }

    pub fn last_error(&self) -> Option<&CoordinatorError> {
        self.last_error.as_ref()
    }

    pub fn restart_required(&self, candidates: &[SettingCandidate]) -> bool {
        candidates.iter().any(|candidate| {
            self.settings.get(&candidate.id).is_some_and(|setting| {
                setting.descriptor.apply_scope == SettingApplyScope::Restart
                    && setting.state.value != candidate.value
            })
        })
    }

    /// Validates, persists, and applies one atomic candidate set.
    ///
    /// # Errors
    ///
    /// Returns validation, persistence, or apply failures after rollback.
    pub fn apply(&mut self, candidates: &[SettingCandidate]) -> Result<(), CoordinatorError> {
        let provenance = SettingProvenance {
            layer: ConfigLayerKind::User,
            source: "settings coordinator".to_owned(),
        };
        self.apply_internal(candidates, Some(&provenance), true)
    }

    /// Resets one setting to its declared default.
    ///
    /// # Errors
    ///
    /// Returns unknown-setting, persistence, validation, or apply failures.
    pub fn reset(&mut self, id: &str) -> Result<(), CoordinatorError> {
        let value = self
            .settings
            .get(id)
            .ok_or_else(|| CoordinatorError::Unknown(id.to_owned()))?
            .descriptor
            .default_value
            .clone();
        self.apply(&[SettingCandidate {
            id: id.to_owned(),
            value,
        }])
    }

    /// Applies an externally reloaded candidate while preserving the last valid state on failure.
    ///
    /// # Errors
    ///
    /// Returns unknown-setting, validation, availability, or apply failures.
    pub fn reload_external(
        &mut self,
        values: &BTreeMap<String, SettingValue>,
        provenance: &SettingProvenance,
    ) -> Result<(), CoordinatorError> {
        let candidates = values
            .iter()
            .map(|(id, value)| SettingCandidate {
                id: id.clone(),
                value: value.clone(),
            })
            .collect::<Vec<_>>();
        self.apply_internal(&candidates, Some(provenance), false)
    }

    fn apply_internal(
        &mut self,
        candidates: &[SettingCandidate],
        provenance: Option<&SettingProvenance>,
        persist: bool,
    ) -> Result<(), CoordinatorError> {
        match self.try_apply(candidates, provenance, persist) {
            Ok(()) => {
                self.last_error = None;
                Ok(())
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn try_apply(
        &mut self,
        candidates: &[SettingCandidate],
        provenance: Option<&SettingProvenance>,
        persist: bool,
    ) -> Result<(), CoordinatorError> {
        let mut changes = BTreeMap::new();
        for candidate in candidates {
            let setting = self
                .settings
                .get(&candidate.id)
                .ok_or_else(|| CoordinatorError::Unknown(candidate.id.clone()))?;
            validate_type(&setting.descriptor, &candidate.value)?;
            if !setting.descriptor.availability.supports(self.platform) {
                return Err(CoordinatorError::Unavailable(candidate.id.clone()));
            }
            setting
                .applier
                .validate(&candidate.value)
                .map_err(|message| CoordinatorError::Validation {
                    id: candidate.id.clone(),
                    message,
                })?;
            changes.insert(candidate.id.clone(), candidate.value.clone());
        }

        let previous_values = self.values();
        let mut candidate_values = previous_values.clone();
        candidate_values.extend(changes.clone());
        if persist {
            self.persistence
                .persist(&candidate_values)
                .map_err(CoordinatorError::Persistence)?;
        }

        let mut ordered = changes.keys().cloned().collect::<Vec<_>>();
        ordered.sort_by_key(|id| {
            let descriptor = &self.settings[id].descriptor;
            (
                descriptor.apply_order,
                descriptor.document.clone(),
                id.clone(),
            )
        });
        let mut applied = Vec::new();
        for id in &ordered {
            let setting = self.settings.get_mut(id).expect("validated setting");
            let previous = setting.state.value.clone();
            let candidate = &changes[id];
            if let Err(message) = setting.applier.apply(&previous, candidate) {
                for applied_id in applied.iter().rev() {
                    let applied_setting =
                        self.settings.get_mut(applied_id).expect("applied setting");
                    applied_setting
                        .applier
                        .rollback(&changes[applied_id], &previous_values[applied_id]);
                }
                if persist {
                    let _ = self.persistence.persist(&previous_values);
                }
                return Err(CoordinatorError::Apply {
                    id: id.clone(),
                    message,
                });
            }
            applied.push(id.clone());
        }

        for (id, value) in changes {
            let setting = self.settings.get_mut(&id).expect("validated setting");
            setting.state.value = value;
            if let Some(provenance) = provenance {
                setting.state.provenance = provenance.clone();
            }
        }
        Ok(())
    }

    fn values(&self) -> BTreeMap<String, SettingValue> {
        self.settings
            .iter()
            .map(|(id, setting)| (id.clone(), setting.state.value.clone()))
            .collect()
    }
}

fn validate_type(
    descriptor: &SettingDescriptor,
    value: &SettingValue,
) -> Result<(), CoordinatorError> {
    let actual = value.kind();
    if actual == descriptor.value_kind {
        Ok(())
    } else {
        Err(CoordinatorError::TypeMismatch {
            id: descriptor.id.clone(),
            expected: descriptor.value_kind,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct MemoryPersistence {
        writes: Arc<Mutex<Vec<BTreeMap<String, SettingValue>>>>,
    }

    impl SettingsPersistence for MemoryPersistence {
        fn persist(&mut self, values: &BTreeMap<String, SettingValue>) -> Result<(), String> {
            self.writes.lock().unwrap().push(values.clone());
            Ok(())
        }
    }

    struct RecordingApplier {
        name: &'static str,
        events: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    impl SettingApplier for RecordingApplier {
        fn validate(&self, value: &SettingValue) -> Result<(), String> {
            if matches!(value, SettingValue::Integer(value) if *value < 0) {
                Err("must be non-negative".to_owned())
            } else {
                Ok(())
            }
        }

        fn apply(
            &mut self,
            _previous: &SettingValue,
            _candidate: &SettingValue,
        ) -> Result<(), String> {
            self.events
                .lock()
                .unwrap()
                .push(format!("apply:{}", self.name));
            if self.fail {
                Err("injected failure".to_owned())
            } else {
                Ok(())
            }
        }

        fn rollback(&mut self, _candidate: &SettingValue, _previous: &SettingValue) {
            self.events
                .lock()
                .unwrap()
                .push(format!("rollback:{}", self.name));
        }
    }

    fn descriptor(id: &str, order: u32, scope: SettingApplyScope) -> SettingDescriptor {
        SettingDescriptor {
            id: id.to_owned(),
            document: "near.toml".to_owned(),
            path: id.to_owned(),
            category: "Test".to_owned(),
            title: id.to_owned(),
            description: "test setting".to_owned(),
            advanced: false,
            value_kind: SettingValueKind::Integer,
            default_value: SettingValue::Integer(1),
            apply_scope: scope,
            apply_order: order,
            availability: SettingPlatformAvailability::All,
        }
    }

    fn provenance(source: &str) -> SettingProvenance {
        SettingProvenance {
            layer: ConfigLayerKind::BuiltIn,
            source: source.to_owned(),
        }
    }

    #[test]
    fn candidates_persist_once_and_apply_in_declared_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let persistence = MemoryPersistence::default();
        let writes = Arc::clone(&persistence.writes);
        let mut coordinator =
            ConfigurationCoordinator::with_persistence(SettingPlatform::Linux, persistence);
        for (id, order) in [("later", 20), ("first", 10)] {
            coordinator
                .register(
                    descriptor(id, order, SettingApplyScope::Live),
                    SettingValue::Integer(1),
                    provenance("builtin"),
                    RecordingApplier {
                        name: id,
                        events: Arc::clone(&events),
                        fail: false,
                    },
                )
                .unwrap();
        }
        coordinator
            .apply(&[
                SettingCandidate {
                    id: "later".to_owned(),
                    value: SettingValue::Integer(2),
                },
                SettingCandidate {
                    id: "first".to_owned(),
                    value: SettingValue::Integer(3),
                },
            ])
            .unwrap();
        assert_eq!(&*events.lock().unwrap(), &["apply:first", "apply:later"]);
        assert_eq!(writes.lock().unwrap().len(), 1);
        assert_eq!(
            coordinator.state("first").unwrap().value,
            SettingValue::Integer(3)
        );
    }

    #[test]
    fn failed_apply_rolls_back_and_restores_persisted_snapshot() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let persistence = MemoryPersistence::default();
        let writes = Arc::clone(&persistence.writes);
        let mut coordinator =
            ConfigurationCoordinator::with_persistence(SettingPlatform::MacOs, persistence);
        coordinator
            .register(
                descriptor("first", 10, SettingApplyScope::Live),
                SettingValue::Integer(1),
                provenance("builtin"),
                RecordingApplier {
                    name: "first",
                    events: Arc::clone(&events),
                    fail: false,
                },
            )
            .unwrap();
        coordinator
            .register(
                descriptor("broken", 20, SettingApplyScope::Restart),
                SettingValue::Integer(1),
                provenance("builtin"),
                RecordingApplier {
                    name: "broken",
                    events: Arc::clone(&events),
                    fail: true,
                },
            )
            .unwrap();
        let candidates = [
            SettingCandidate {
                id: "first".to_owned(),
                value: SettingValue::Integer(2),
            },
            SettingCandidate {
                id: "broken".to_owned(),
                value: SettingValue::Integer(2),
            },
        ];
        assert!(coordinator.restart_required(&candidates));
        assert!(matches!(
            coordinator.apply(&candidates),
            Err(CoordinatorError::Apply { .. })
        ));
        assert_eq!(
            &*events.lock().unwrap(),
            &["apply:first", "apply:broken", "rollback:first"]
        );
        assert_eq!(writes.lock().unwrap().len(), 2);
        assert_eq!(
            coordinator.state("first").unwrap().value,
            SettingValue::Integer(1)
        );
    }

    #[test]
    fn invalid_external_reload_preserves_last_valid_state_and_provenance() {
        let mut coordinator = ConfigurationCoordinator::new(SettingPlatform::Windows);
        coordinator
            .register(
                descriptor("count", 1, SettingApplyScope::NewSurface),
                SettingValue::Integer(4),
                provenance("builtin"),
                RecordingApplier {
                    name: "count",
                    events: Arc::default(),
                    fail: false,
                },
            )
            .unwrap();
        let error = coordinator
            .reload_external(
                &BTreeMap::from([("count".to_owned(), SettingValue::Integer(-1))]),
                &SettingProvenance {
                    layer: ConfigLayerKind::User,
                    source: "user.toml".to_owned(),
                },
            )
            .unwrap_err();
        assert!(matches!(error, CoordinatorError::Validation { .. }));
        assert_eq!(
            coordinator.state("count").unwrap().value,
            SettingValue::Integer(4)
        );
        assert_eq!(
            coordinator.state("count").unwrap().provenance.source,
            "builtin"
        );
    }
}
