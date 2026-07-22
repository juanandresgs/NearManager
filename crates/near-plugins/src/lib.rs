//! Capability-controlled WebAssembly Component Model extension hosting.

mod process;

pub use process::{
    PROCESS_PROTOCOL_VERSION, ProcessCommandManifest, ProcessLimits, ProcessPlugin,
    ProcessPluginHost, ProcessPluginManifest,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use near_core::{
    ActionContext, CapabilitySet, CommandDescriptor, CommandExtension, CommandId,
    CommandInvocation, CommandPrefixDescriptor, ExtensionCommandPrefix, ExtensionEffect,
    ExtensionReport, ListPage, ListRequest, Location, OpenRequest, ProviderError, ProviderFuture,
    ProviderId, ResourceEntry, ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef,
    ResourceStream, SafetyClass,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wasmtime::component::{Component, Func, Instance, Linker, Val};
use wasmtime::{Config, Engine, Store, StoreContextMut, StoreLimits, StoreLimitsBuilder};

pub const PLUGIN_MANIFEST_SCHEMA: u32 = 1;
pub const PLUGIN_INTERFACE_VERSION: &str = "0.1.0";
pub const CAPABILITY_LOG_V1: &str = "near.host.log@1";
pub const CAPABILITY_NOTIFY_V1: &str = "near.host.notify@1";
pub const CAPABILITY_RESOURCE_READ_V1: &str = "near.resource.read@1";

/// Validates the published WIT package name, full `SemVer`, interfaces, and extension world.
///
/// # Errors
///
/// Returns parse errors or missing/version-mismatched contract elements.
pub fn validate_wit_contract(document: &str) -> Result<Version, PluginError> {
    let mut resolve = wit_parser::Resolve::default();
    let package_id = resolve
        .push_str("plugin.wit", document)
        .map_err(|error| PluginError::InvalidWit(error.to_string()))?;
    let package = &resolve.packages[package_id];
    if package.name.namespace != "near" || package.name.name != "plugin" {
        return Err(PluginError::InvalidWit(format!(
            "expected package near:plugin, found {}:{}",
            package.name.namespace, package.name.name
        )));
    }
    let version =
        package.name.version.clone().ok_or_else(|| {
            PluginError::InvalidWit("package must include full SemVer".to_owned())
        })?;
    let expected = Version::new(0, 1, 0);
    if version != expected {
        return Err(PluginError::InvalidWit(format!(
            "WIT package version {version} does not match host {expected}"
        )));
    }
    for interface in ["types", "host", "commands", "provider"] {
        if !package.interfaces.contains_key(interface) {
            return Err(PluginError::InvalidWit(format!(
                "missing interface {interface}"
            )));
        }
    }
    if !package.worlds.contains_key("extension") {
        return Err(PluginError::InvalidWit(
            "missing extension world".to_owned(),
        ));
    }
    Ok(version)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub version: Version,
    pub interface: VersionReq,
    #[serde(default = "default_wasm_runtime")]
    pub runtime: String,
    #[serde(default = "default_component_artifact")]
    pub component: String,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub prefixes: Vec<PluginCommandPrefix>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub limits: PluginLimits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCommandPrefix {
    pub name: String,
    pub description: String,
    pub command: String,
    pub argument: String,
}

impl PluginManifest {
    /// Parses and validates a versioned plugin manifest.
    ///
    /// # Errors
    ///
    /// Returns TOML, schema, identifier, interface, or capability validation failures.
    pub fn from_toml(document: &str) -> Result<Self, PluginError> {
        let manifest: Self = toml::from_str(document)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates stable identity, schema, interface compatibility, and capability names.
    ///
    /// # Errors
    ///
    /// Returns the first invalid manifest field.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.schema != PLUGIN_MANIFEST_SCHEMA {
            return Err(PluginError::UnsupportedManifestSchema(self.schema));
        }
        validate_id(&self.id)?;
        if self.runtime != "wasm" {
            return Err(PluginError::InvalidComponent(format!(
                "Wasm manifest runtime must be wasm, found {}",
                self.runtime
            )));
        }
        validate_artifact(&self.component)?;
        let host = Version::new(0, 1, 0);
        if !self.interface.matches(&host) {
            return Err(PluginError::IncompatibleInterface {
                required: self.interface.to_string(),
                provided: host.to_string(),
            });
        }
        for capability in &self.capabilities {
            if !matches!(
                capability.as_str(),
                CAPABILITY_LOG_V1 | CAPABILITY_NOTIFY_V1 | CAPABILITY_RESOURCE_READ_V1
            ) {
                return Err(PluginError::UnknownCapability(capability.clone()));
            }
        }
        validate_plugin_prefixes(&self.prefixes, &self.commands)?;
        Ok(())
    }
}

fn validate_plugin_prefixes(
    prefixes: &[PluginCommandPrefix],
    commands: &[String],
) -> Result<(), PluginError> {
    let mut names = BTreeSet::new();
    for prefix in prefixes {
        if prefix.name.len() < 2
            || !prefix.name.chars().enumerate().all(|(index, character)| {
                character.is_ascii_alphabetic()
                    || (index > 0
                        && (character.is_ascii_digit() || character == '-' || character == '_'))
            })
        {
            return Err(PluginError::InvalidComponent(format!(
                "invalid command prefix {}",
                prefix.name
            )));
        }
        if !names.insert(&prefix.name) {
            return Err(PluginError::InvalidComponent(format!(
                "duplicate command prefix {}",
                prefix.name
            )));
        }
        if !commands.contains(&prefix.command) {
            return Err(PluginError::InvalidComponent(format!(
                "command prefix {} targets undeclared command {}",
                prefix.name, prefix.command
            )));
        }
        if prefix.argument.trim().is_empty() {
            return Err(PluginError::InvalidComponent(format!(
                "command prefix {} has an empty argument name",
                prefix.name
            )));
        }
    }
    Ok(())
}

fn default_component_artifact() -> String {
    "component.wasm".to_owned()
}

fn default_wasm_runtime() -> String {
    "wasm".to_owned()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginLimits {
    pub memory_bytes: usize,
    pub fuel: u64,
    pub timeout_ms: u64,
}

impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            fuel: 10_000_000,
            timeout_ms: 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginOrigin {
    Installed,
    Workspace,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityGrantStore {
    grants: BTreeMap<String, BTreeSet<String>>,
    trusted_workspaces: BTreeSet<String>,
}

impl CapabilityGrantStore {
    pub fn grant(&mut self, plugin: impl Into<String>, capability: impl Into<String>) {
        self.grants
            .entry(plugin.into())
            .or_default()
            .insert(capability.into());
    }

    pub fn revoke(&mut self, plugin: &str, capability: &str) -> bool {
        self.grants
            .get_mut(plugin)
            .is_some_and(|grants| grants.remove(capability))
    }

    pub fn grants_for(&self, plugin: &str) -> BTreeSet<String> {
        self.grants.get(plugin).cloned().unwrap_or_default()
    }

    pub fn trust_workspace_plugin(&mut self, plugin: impl Into<String>) {
        self.trusted_workspaces.insert(plugin.into());
    }

    pub fn revoke_workspace_trust(&mut self, plugin: &str) -> bool {
        self.trusted_workspaces.remove(plugin)
    }

    fn permits_workspace(&self, plugin: &str) -> bool {
        self.trusted_workspaces.contains(plugin)
    }
}

pub trait PluginResourceReader: Send + Sync {
    /// Reads a bounded resource range on behalf of a granted plugin.
    ///
    /// # Errors
    ///
    /// Returns provider-specific read failures without exposing ambient filesystem access.
    fn read(&self, resource: &ResourceRef, offset: u64, length: u32) -> Result<Vec<u8>, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginEvent {
    Log { level: String, message: String },
    Notification { severity: String, message: String },
}

struct HostState {
    plugin: String,
    declared: BTreeSet<String>,
    granted: BTreeSet<String>,
    reader: Option<Arc<dyn PluginResourceReader>>,
    events: Vec<PluginEvent>,
    limits: StoreLimits,
}

impl HostState {
    fn require(&self, capability: &str) -> Result<(), String> {
        if !self.declared.contains(capability) {
            return Err(format!(
                "plugin {} did not declare capability {capability}",
                self.plugin
            ));
        }
        if !self.granted.contains(capability) {
            return Err(format!(
                "plugin {} was not granted capability {capability}",
                self.plugin
            ));
        }
        Ok(())
    }
}

pub struct WasmPluginHost {
    engine: Engine,
    grants: CapabilityGrantStore,
    reader: Option<Arc<dyn PluginResourceReader>>,
}

impl WasmPluginHost {
    /// Creates a Component Model host without WASI or ambient operating-system imports.
    ///
    /// # Errors
    ///
    /// Returns Wasmtime engine configuration failures.
    pub fn new(grants: CapabilityGrantStore) -> Result<Self, PluginError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        Ok(Self {
            engine: Engine::new(&config).map_err(PluginError::runtime)?,
            grants,
            reader: None,
        })
    }

    #[must_use]
    pub fn with_resource_reader(mut self, reader: Arc<dyn PluginResourceReader>) -> Self {
        self.reader = Some(reader);
        self
    }

    /// Loads and validates a component package.
    ///
    /// # Errors
    ///
    /// Returns manifest, trust, compilation, import, or interface failures.
    pub fn load(
        &self,
        manifest: PluginManifest,
        origin: PluginOrigin,
        component_path: &Path,
    ) -> Result<LoadedPlugin, PluginError> {
        manifest.validate()?;
        if origin == PluginOrigin::Workspace && !self.grants.permits_workspace(&manifest.id) {
            return Err(PluginError::WorkspaceTrustRequired(manifest.id));
        }
        let component =
            Component::from_file(&self.engine, component_path).map_err(PluginError::runtime)?;
        validate_component_contract(&self.engine, &component)?;
        Ok(LoadedPlugin {
            engine: self.engine.clone(),
            manifest,
            component,
            grants: self.grants.clone(),
            reader: self.reader.clone(),
        })
    }
}

pub struct LoadedPlugin {
    engine: Engine,
    manifest: PluginManifest,
    component: Component,
    grants: CapabilityGrantStore,
    reader: Option<Arc<dyn PluginResourceReader>>,
}

impl LoadedPlugin {
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Reads plugin command descriptors through the versioned WIT interface.
    ///
    /// # Errors
    ///
    /// Returns linker, instantiation, resource-limit, trap, or descriptor errors.
    pub fn commands(&self) -> Result<Vec<CommandDescriptor>, PluginError> {
        let mut runtime = self.instantiate()?;
        let function = runtime.exported_function("near:plugin/commands@0.1.0", "list-commands")?;
        let mut results = [Val::List(Vec::new())];
        function
            .call(&mut runtime.store, &[], &mut results)
            .map_err(PluginError::runtime)?;
        function
            .post_return(&mut runtime.store)
            .map_err(PluginError::runtime)?;
        let Val::List(commands) = results.into_iter().next().unwrap_or(Val::Bool(false)) else {
            return Err(PluginError::InvalidComponent(
                "list-commands returned a non-list value".to_owned(),
            ));
        };
        commands.into_iter().map(convert_descriptor).collect()
    }

    /// Invokes a plugin command through typed context and JSON arguments.
    ///
    /// # Errors
    ///
    /// Returns resource-limit, capability, trap, serialization, or plugin failures.
    pub fn invoke(
        &self,
        invocation: &CommandInvocation,
        context: &ActionContext,
    ) -> Result<PluginInvocation, PluginError> {
        let mut runtime = self.instantiate()?;
        let function = runtime.exported_function("near:plugin/commands@0.1.0", "invoke")?;
        let arguments = serde_json::to_string(&invocation.arguments)
            .map_err(|error| PluginError::Serialization(error.to_string()))?;
        let parameters = [
            Val::Record(vec![(
                "value".to_owned(),
                Val::String(invocation.id.to_string()),
            )]),
            convert_context(context),
            Val::String(arguments),
        ];
        let mut results = [Val::Result(Ok(None))];
        function
            .call(&mut runtime.store, &parameters, &mut results)
            .map_err(PluginError::runtime)?;
        function
            .post_return(&mut runtime.store)
            .map_err(PluginError::runtime)?;
        let result = decode_guest_result(results.into_iter().next().unwrap_or(Val::Bool(false)))?;
        Ok(PluginInvocation {
            result,
            events: runtime.store.data().events.clone(),
        })
    }

    fn provider_identity(&self) -> Result<(ProviderId, Vec<String>), PluginError> {
        let mut runtime = self.instantiate()?;
        let id = call_string_export(&mut runtime, "near:plugin/provider@0.1.0", "id")?;
        let schemes =
            call_string_list_export(&mut runtime, "near:plugin/provider@0.1.0", "schemes")?;
        Ok((ProviderId::from(id), schemes))
    }

    fn list_resources(
        &self,
        location: &Location,
        request: &ListRequest,
    ) -> Result<ListPage, PluginError> {
        let mut runtime = self.instantiate()?;
        let function = runtime.exported_function("near:plugin/provider@0.1.0", "list-resources")?;
        let limit = u32::try_from(request.page_size).unwrap_or(u32::MAX);
        let parameters = [
            Val::String(location.as_str().to_owned()),
            optional_string(request.continuation.as_deref()),
            Val::U32(limit),
        ];
        let mut results = [Val::Result(Ok(None))];
        function
            .call(&mut runtime.store, &parameters, &mut results)
            .map_err(PluginError::runtime)?;
        function
            .post_return(&mut runtime.store)
            .map_err(PluginError::runtime)?;
        decode_list_page(
            results.into_iter().next().unwrap_or(Val::Bool(false)),
            request,
        )
    }

    fn instantiate(&self) -> Result<PluginRuntime, PluginError> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.manifest.limits.memory_bytes)
            .instances(16)
            .memories(8)
            .tables(8)
            .build();
        let mut store = Store::new(
            &self.engine,
            HostState {
                plugin: self.manifest.id.clone(),
                declared: self.manifest.capabilities.clone(),
                granted: self.grants.grants_for(&self.manifest.id),
                reader: self.reader.clone(),
                events: Vec::new(),
                limits,
            },
        );
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.manifest.limits.fuel)
            .map_err(PluginError::runtime)?;
        store.set_epoch_deadline(1);
        let deadline = DeadlineGuard(Arc::new(AtomicBool::new(true)));
        let timer_active = Arc::clone(&deadline.0);
        let timer_engine = self.engine.clone();
        let timeout = Duration::from_millis(self.manifest.limits.timeout_ms.max(1));
        thread::spawn(move || {
            thread::sleep(timeout);
            if timer_active.load(Ordering::Acquire) {
                timer_engine.increment_epoch();
            }
        });
        let mut linker = Linker::new(&self.engine);
        add_host_interface(&mut linker)?;
        let instance = linker
            .instantiate(&mut store, &self.component)
            .map_err(PluginError::runtime)?;
        Ok(PluginRuntime {
            instance,
            store,
            _deadline: deadline,
        })
    }
}

pub struct PluginProviderAdapter {
    plugin: Arc<LoadedPlugin>,
    id: ProviderId,
    schemes: Box<[&'static str]>,
}

impl PluginProviderAdapter {
    /// Creates a provider adapter from the component's versioned provider exports.
    ///
    /// # Errors
    ///
    /// Returns component traps, invalid identifiers, empty schemes, or interface failures.
    pub fn new(plugin: Arc<LoadedPlugin>) -> Result<Self, PluginError> {
        let (id, schemes) = plugin.provider_identity()?;
        if schemes.is_empty() {
            return Err(PluginError::InvalidComponent(
                "provider exported no schemes".to_owned(),
            ));
        }
        let schemes = schemes
            .into_iter()
            .map(|scheme| Box::leak(scheme.into_boxed_str()) as &'static str)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            plugin,
            id,
            schemes,
        })
    }
}

impl ResourceProvider for PluginProviderAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn schemes(&self) -> &[&str] {
        &self.schemes
    }

    fn list<'a>(
        &'a self,
        location: &'a Location,
        request: ListRequest,
    ) -> ProviderFuture<'a, ListPage> {
        Box::pin(async move {
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let page = self
                .plugin
                .list_resources(location, &request)
                .map_err(|error| ProviderError::Failed(error.to_string()))?;
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            Ok(page)
        })
    }

    fn stat<'a>(&'a self, _resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async {
            Err(ProviderError::Unsupported(
                "plugin provider stat is not in near:plugin@0.1.0".to_owned(),
            ))
        })
    }

    fn open<'a>(
        &'a self,
        _resource: &'a ResourceRef,
        _request: OpenRequest,
    ) -> ProviderFuture<'a, ResourceStream> {
        Box::pin(async {
            Err(ProviderError::Unsupported(
                "plugin provider open is not in near:plugin@0.1.0".to_owned(),
            ))
        })
    }

    fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
        CapabilitySet::default()
    }
}

impl CommandExtension for LoadedPlugin {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn commands(&self) -> Result<Vec<CommandDescriptor>, String> {
        Self::commands(self).map_err(|error| error.to_string())
    }

    fn command_prefixes(&self) -> Result<Vec<ExtensionCommandPrefix>, String> {
        Ok(self
            .manifest
            .prefixes
            .iter()
            .map(|prefix| ExtensionCommandPrefix {
                prefix: CommandPrefixDescriptor {
                    name: prefix.name.clone(),
                    description: prefix.description.clone(),
                },
                command: CommandId::from(prefix.command.clone()),
                argument: prefix.argument.clone(),
            })
            .collect())
    }

    fn invoke(
        &self,
        invocation: &CommandInvocation,
        context: &ActionContext,
    ) -> Result<ExtensionReport, String> {
        let invocation =
            Self::invoke(self, invocation, context).map_err(|error| error.to_string())?;
        let effect = match invocation.result {
            PluginCommandResult::Message(message) => ExtensionEffect::Message(message),
            PluginCommandResult::Navigate(location) => {
                ExtensionEffect::Navigate(Location::new(location))
            }
            PluginCommandResult::Open(resources) => ExtensionEffect::Open(resources),
            PluginCommandResult::Task(task) => ExtensionEffect::Task(task),
        };
        let diagnostics = invocation
            .events
            .into_iter()
            .map(|event| match event {
                PluginEvent::Log { level, message } => format!("{level}: {message}"),
                PluginEvent::Notification { severity, message } => {
                    format!("{severity}: {message}")
                }
            })
            .collect();
        Ok(ExtensionReport {
            effect,
            diagnostics,
        })
    }
}

struct DeadlineGuard(Arc<AtomicBool>);

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct PluginRuntime {
    instance: Instance,
    store: Store<HostState>,
    _deadline: DeadlineGuard,
}

impl PluginRuntime {
    fn exported_function(&mut self, interface: &str, function: &str) -> Result<Func, PluginError> {
        let interface_index = self
            .instance
            .get_export_index(&mut self.store, None, interface)
            .ok_or_else(|| PluginError::InvalidComponent(format!("missing {interface}")))?;
        let function_index = self
            .instance
            .get_export_index(&mut self.store, Some(&interface_index), function)
            .ok_or_else(|| PluginError::InvalidComponent(format!("missing {function}")))?;
        self.instance
            .get_func(&mut self.store, function_index)
            .ok_or_else(|| PluginError::InvalidComponent(format!("{function} is not a function")))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginInvocation {
    pub result: PluginCommandResult,
    pub events: Vec<PluginEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginCommandResult {
    Message(String),
    Navigate(String),
    Open(Vec<ResourceRef>),
    Task(String),
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin manifest TOML is invalid: {0}")]
    ManifestToml(#[from] toml::de::Error),
    #[error("unsupported plugin manifest schema {0}")]
    UnsupportedManifestSchema(u32),
    #[error("invalid plugin identifier {0:?}")]
    InvalidIdentifier(String),
    #[error("invalid plugin component artifact {0:?}")]
    InvalidArtifact(String),
    #[error("plugin requires interface {required}, host provides {provided}")]
    IncompatibleInterface { required: String, provided: String },
    #[error("unknown plugin capability {0}")]
    UnknownCapability(String),
    #[error("workspace plugin {0} requires explicit trust")]
    WorkspaceTrustRequired(String),
    #[error("component contract is invalid: {0}")]
    InvalidComponent(String),
    #[error("plugin runtime failed: {0}")]
    Runtime(String),
    #[error("plugin returned an error: {0}")]
    Guest(String),
    #[error("plugin value serialization failed: {0}")]
    Serialization(String),
    #[error("plugin WIT contract is invalid: {0}")]
    InvalidWit(String),
}

impl PluginError {
    fn runtime(error: impl std::fmt::Display) -> Self {
        Self::Runtime(error.to_string())
    }
}

fn validate_id(id: &str) -> Result<(), PluginError> {
    if id.is_empty()
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte)
        })
    {
        return Err(PluginError::InvalidIdentifier(id.to_owned()));
    }
    Ok(())
}

fn validate_artifact(artifact: &str) -> Result<(), PluginError> {
    let path = Path::new(artifact);
    if artifact.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(PluginError::InvalidArtifact(artifact.to_owned()));
    }
    Ok(())
}

fn validate_component_contract(engine: &Engine, component: &Component) -> Result<(), PluginError> {
    let ty = component.component_type();
    let imports = ty
        .imports(engine)
        .map(|(name, _)| name)
        .collect::<BTreeSet<_>>();
    if imports != BTreeSet::from(["near:plugin/host@0.1.0"]) {
        return Err(PluginError::InvalidComponent(format!(
            "expected only near:plugin/host@0.1.0 import, found {imports:?}"
        )));
    }
    let exports = ty
        .exports(engine)
        .map(|(name, _)| name)
        .collect::<BTreeSet<_>>();
    for required in ["near:plugin/commands@0.1.0", "near:plugin/provider@0.1.0"] {
        if !exports.contains(required) {
            return Err(PluginError::InvalidComponent(format!(
                "missing required export {required}"
            )));
        }
    }
    Ok(())
}

fn add_host_interface(linker: &mut Linker<HostState>) -> Result<(), PluginError> {
    let mut host = linker
        .instance("near:plugin/host@0.1.0")
        .map_err(PluginError::runtime)?;
    host.func_new("log", |mut store, parameters, results| {
        let level = parameter_string(parameters, 0, "level")?;
        let message = parameter_string(parameters, 1, "message")?;
        let permission = store.data().require(CAPABILITY_LOG_V1);
        if permission.is_ok() {
            store
                .data_mut()
                .events
                .push(PluginEvent::Log { level, message });
        }
        results[0] = capability_result(permission);
        Ok(())
    })
    .map_err(PluginError::runtime)?;
    host.func_new("notify", |mut store, parameters, results| {
        let severity = parameter_string(parameters, 0, "severity")?;
        let message = parameter_string(parameters, 1, "message")?;
        let permission = store.data().require(CAPABILITY_NOTIFY_V1);
        if permission.is_ok() {
            store
                .data_mut()
                .events
                .push(PluginEvent::Notification { severity, message });
        }
        results[0] = capability_result(permission);
        Ok(())
    })
    .map_err(PluginError::runtime)?;
    host.func_new("read", |mut store, parameters, results| {
        let outcome = read_resource(&mut store, parameters);
        results[0] = match outcome {
            Ok(bytes) => Val::Result(Ok(Some(Box::new(Val::List(
                bytes.into_iter().map(Val::U8).collect(),
            ))))),
            Err(error) => Val::Result(Err(Some(Box::new(Val::String(error))))),
        };
        Ok(())
    })
    .map_err(PluginError::runtime)?;
    Ok(())
}

fn read_resource(
    store: &mut StoreContextMut<'_, HostState>,
    parameters: &[Val],
) -> Result<Vec<u8>, String> {
    store.data().require(CAPABILITY_RESOURCE_READ_V1)?;
    let fields = parameters
        .first()
        .cloned()
        .ok_or_else(|| "missing target".to_owned())
        .and_then(|value| record(value, "resource target").map_err(|error| error.to_string()))?;
    let provider = string_field(&fields, "provider").map_err(|error| error.to_string())?;
    let uri = string_field(&fields, "uri").map_err(|error| error.to_string())?;
    let offset = match parameters.get(1) {
        Some(Val::U64(offset)) => *offset,
        _ => return Err("invalid offset".to_owned()),
    };
    let length = match parameters.get(2) {
        Some(Val::U32(length)) => *length,
        _ => return Err("invalid length".to_owned()),
    };
    let reader = store
        .data()
        .reader
        .as_ref()
        .ok_or_else(|| "resource reader is unavailable".to_owned())?;
    reader.read(
        &ResourceRef {
            provider: ProviderId::from(provider),
            location: Location::new(uri),
        },
        offset,
        length,
    )
}

fn convert_descriptor(value: Val) -> Result<CommandDescriptor, PluginError> {
    let fields = record(value, "command descriptor")?;
    let id_fields = record(field(&fields, "id")?.clone(), "command id")?;
    let safety_name = string_field(&fields, "safety")?;
    let safety = match safety_name.as_str() {
        "read-only" => SafetyClass::ReadOnly,
        "confirmable" => SafetyClass::Confirmable,
        "destructive" => SafetyClass::Destructive,
        other => {
            return Err(PluginError::InvalidComponent(format!(
                "unknown safety class {other}"
            )));
        }
    };
    Ok(CommandDescriptor {
        id: CommandId::from(string_field(&id_fields, "value")?),
        title: string_field(&fields, "title")?,
        description: string_field(&fields, "description")?,
        category: string_list_field(&fields, "category")?,
        safety,
        arguments: BTreeMap::new(),
    })
}

fn convert_context(context: &ActionContext) -> Val {
    Val::Record(vec![
        (
            "focused-location".to_owned(),
            optional_string(context.location.as_ref().map(Location::as_str)),
        ),
        (
            "peer-location".to_owned(),
            optional_string(context.peer_location.as_ref().map(Location::as_str)),
        ),
        (
            "current".to_owned(),
            Val::Option(context.current.as_ref().map(convert_resource).map(Box::new)),
        ),
        (
            "selected".to_owned(),
            Val::List(context.selected.iter().map(convert_resource).collect()),
        ),
        (
            "capabilities".to_owned(),
            Val::List(
                context
                    .capabilities
                    .iter()
                    .map(|capability| Val::String(capability.as_str().to_owned()))
                    .collect(),
            ),
        ),
    ])
}

fn convert_resource(resource: &ResourceRef) -> Val {
    Val::Record(vec![
        (
            "uri".to_owned(),
            Val::String(resource.location.as_str().to_owned()),
        ),
        (
            "provider".to_owned(),
            Val::String(resource.provider.as_str().to_owned()),
        ),
    ])
}

fn decode_guest_result(value: Val) -> Result<PluginCommandResult, PluginError> {
    let Val::Result(result) = value else {
        return Err(PluginError::InvalidComponent(
            "invoke returned a non-result value".to_owned(),
        ));
    };
    match result {
        Ok(Some(value)) => convert_result(*value),
        Ok(None) => Err(PluginError::InvalidComponent(
            "invoke returned an empty success result".to_owned(),
        )),
        Err(Some(error)) => Err(PluginError::Guest(expect_string(*error, "guest error")?)),
        Err(None) => Err(PluginError::Guest("plugin invocation failed".to_owned())),
    }
}

fn convert_result(result: Val) -> Result<PluginCommandResult, PluginError> {
    match result {
        Val::Variant(name, Some(value)) if name == "message" => Ok(PluginCommandResult::Message(
            expect_string(*value, "message")?,
        )),
        Val::Variant(name, Some(value)) if name == "navigate" => Ok(PluginCommandResult::Navigate(
            expect_string(*value, "navigate")?,
        )),
        Val::Variant(name, Some(value)) if name == "task" => {
            Ok(PluginCommandResult::Task(expect_string(*value, "task")?))
        }
        Val::Variant(name, Some(value)) if name == "open" => {
            let Val::List(resources) = *value else {
                return Err(PluginError::InvalidComponent(
                    "open result is not a resource list".to_owned(),
                ));
            };
            let resources = resources
                .into_iter()
                .map(|resource| {
                    let fields = record(resource, "resource reference")?;
                    Ok(ResourceRef {
                        provider: ProviderId::from(string_field(&fields, "provider")?),
                        location: Location::new(string_field(&fields, "uri")?),
                    })
                })
                .collect::<Result<_, PluginError>>()?;
            Ok(PluginCommandResult::Open(resources))
        }
        _ => Err(PluginError::InvalidComponent(
            "unknown command-result variant".to_owned(),
        )),
    }
}

fn call_string_export(
    runtime: &mut PluginRuntime,
    interface: &str,
    name: &str,
) -> Result<String, PluginError> {
    let function = runtime.exported_function(interface, name)?;
    let mut results = [Val::String(String::new())];
    function
        .call(&mut runtime.store, &[], &mut results)
        .map_err(PluginError::runtime)?;
    function
        .post_return(&mut runtime.store)
        .map_err(PluginError::runtime)?;
    expect_string(results.into_iter().next().unwrap_or(Val::Bool(false)), name)
}

fn call_string_list_export(
    runtime: &mut PluginRuntime,
    interface: &str,
    name: &str,
) -> Result<Vec<String>, PluginError> {
    let function = runtime.exported_function(interface, name)?;
    let mut results = [Val::List(Vec::new())];
    function
        .call(&mut runtime.store, &[], &mut results)
        .map_err(PluginError::runtime)?;
    function
        .post_return(&mut runtime.store)
        .map_err(PluginError::runtime)?;
    let Val::List(values) = results.into_iter().next().unwrap_or(Val::Bool(false)) else {
        return Err(PluginError::InvalidComponent(format!(
            "{name} returned a non-list value"
        )));
    };
    values
        .into_iter()
        .map(|value| expect_string(value, name))
        .collect()
}

fn decode_list_page(value: Val, request: &ListRequest) -> Result<ListPage, PluginError> {
    let Val::Result(result) = value else {
        return Err(PluginError::InvalidComponent(
            "list-resources returned a non-result value".to_owned(),
        ));
    };
    let tuple = match result {
        Ok(Some(value)) => *value,
        Ok(None) => {
            return Err(PluginError::InvalidComponent(
                "list-resources returned an empty success".to_owned(),
            ));
        }
        Err(Some(error)) => return Err(PluginError::Guest(expect_string(*error, "list error")?)),
        Err(None) => return Err(PluginError::Guest("provider listing failed".to_owned())),
    };
    let Val::Tuple(mut values) = tuple else {
        return Err(PluginError::InvalidComponent(
            "list-resources success is not a tuple".to_owned(),
        ));
    };
    if values.len() != 2 {
        return Err(PluginError::InvalidComponent(
            "list-resources tuple must contain entries and continuation".to_owned(),
        ));
    }
    let continuation_value = values.pop().ok_or_else(|| {
        PluginError::InvalidComponent("provider continuation is missing".to_owned())
    })?;
    let continuation = match continuation_value {
        Val::Option(Some(value)) => Some(expect_string(*value, "continuation")?),
        Val::Option(None) => None,
        _ => {
            return Err(PluginError::InvalidComponent(
                "provider continuation is not an option<string>".to_owned(),
            ));
        }
    };
    let entries_value = values
        .pop()
        .ok_or_else(|| PluginError::InvalidComponent("provider entries are missing".to_owned()))?;
    let Val::List(items) = entries_value else {
        return Err(PluginError::InvalidComponent(
            "provider entries are not a list".to_owned(),
        ));
    };
    let entries = items
        .into_iter()
        .map(decode_resource_entry)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ListPage {
        generation: request.generation,
        complete: continuation.is_none(),
        entries,
        continuation,
    })
}

fn decode_resource_entry(value: Val) -> Result<ResourceEntry, PluginError> {
    let fields = record(value, "resource item")?;
    let reference = record(field(&fields, "reference")?.clone(), "resource reference")?;
    let name = string_field(&fields, "name")?;
    let kind_name = string_field(&fields, "kind")?;
    let kind = match kind_name.as_str() {
        "file" => ResourceKind::File,
        "directory" => ResourceKind::Directory,
        "package" => ResourceKind::Package,
        "symlink" => ResourceKind::Symlink,
        "virtual" => ResourceKind::Virtual,
        _ => ResourceKind::Other,
    };
    let size = optional_u64(field(&fields, "size")?, "size")?;
    let modified_unix_ms = optional_i64(field(&fields, "modified-unix-ms")?, "modified")?;
    let capabilities = string_list_field(&fields, "capabilities")?;
    let resource = ResourceRef {
        provider: ProviderId::from(string_field(&reference, "provider")?),
        location: Location::new(string_field(&reference, "uri")?),
    };
    Ok(ResourceEntry {
        resource,
        details: format!(
            "{kind_name}{}",
            size.map_or(String::new(), |size| format!(" {size}"))
        ),
        metadata: ResourceMetadata {
            name,
            kind,
            size,
            modified_unix_ms,
            extensions: BTreeMap::from([(
                "near.plugin.capabilities".to_owned(),
                near_core::MetadataValue::Strings(capabilities),
            )]),
            ..ResourceMetadata::default()
        },
    })
}

fn optional_u64(value: &Val, name: &str) -> Result<Option<u64>, PluginError> {
    match value {
        Val::Option(Some(value)) => match value.as_ref() {
            Val::U64(value) => Ok(Some(*value)),
            _ => Err(PluginError::InvalidComponent(format!(
                "{name} is not option<u64>"
            ))),
        },
        Val::Option(None) => Ok(None),
        _ => Err(PluginError::InvalidComponent(format!(
            "{name} is not an option"
        ))),
    }
}

fn optional_i64(value: &Val, name: &str) -> Result<Option<i64>, PluginError> {
    match value {
        Val::Option(Some(value)) => match value.as_ref() {
            Val::S64(value) => Ok(Some(*value)),
            _ => Err(PluginError::InvalidComponent(format!(
                "{name} is not option<s64>"
            ))),
        },
        Val::Option(None) => Ok(None),
        _ => Err(PluginError::InvalidComponent(format!(
            "{name} is not an option"
        ))),
    }
}

fn optional_string(value: Option<&str>) -> Val {
    Val::Option(value.map(|value| Box::new(Val::String(value.to_owned()))))
}

fn capability_result(result: Result<(), String>) -> Val {
    match result {
        Ok(()) => Val::Result(Ok(None)),
        Err(error) => Val::Result(Err(Some(Box::new(Val::String(error))))),
    }
}

fn parameter_string(parameters: &[Val], index: usize, name: &str) -> wasmtime::Result<String> {
    match parameters.get(index) {
        Some(Val::String(value)) => Ok(value.clone()),
        _ => Err(wasmtime::Error::msg(format!("invalid {name}"))),
    }
}

fn record(value: Val, description: &str) -> Result<Vec<(String, Val)>, PluginError> {
    match value {
        Val::Record(fields) => Ok(fields),
        _ => Err(PluginError::InvalidComponent(format!(
            "{description} is not a record"
        ))),
    }
}

fn field<'a>(fields: &'a [(String, Val)], name: &str) -> Result<&'a Val, PluginError> {
    fields
        .iter()
        .find_map(|(field, value)| (field == name).then_some(value))
        .ok_or_else(|| PluginError::InvalidComponent(format!("missing field {name}")))
}

fn string_field(fields: &[(String, Val)], name: &str) -> Result<String, PluginError> {
    expect_string(field(fields, name)?.clone(), name)
}

fn string_list_field(fields: &[(String, Val)], name: &str) -> Result<Vec<String>, PluginError> {
    let Val::List(values) = field(fields, name)?.clone() else {
        return Err(PluginError::InvalidComponent(format!(
            "field {name} is not a list"
        )));
    };
    values
        .into_iter()
        .map(|value| expect_string(value, name))
        .collect()
}

fn expect_string(value: Val, description: &str) -> Result<String, PluginError> {
    match value {
        Val::String(value) => Ok(value),
        _ => Err(PluginError::InvalidComponent(format!(
            "{description} is not a string"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    const WIT: &str = include_str!("../../../specs/plugin.wit");

    fn manifest(limits: PluginLimits) -> PluginManifest {
        PluginManifest {
            schema: PLUGIN_MANIFEST_SCHEMA,
            id: "test.component".to_owned(),
            name: "Test Component".to_owned(),
            version: Version::new(1, 0, 0),
            interface: VersionReq::parse("^0.1.0").unwrap(),
            runtime: "wasm".to_owned(),
            component: "component.wasm".to_owned(),
            capabilities: BTreeSet::new(),
            commands: vec!["test.command".to_owned()],
            prefixes: Vec::new(),
            providers: vec!["test".to_owned()],
            limits,
        }
    }

    #[test]
    fn manifests_validate_command_prefix_ownership() {
        let mut valid = manifest(PluginLimits::default());
        valid.prefixes.push(PluginCommandPrefix {
            name: "query".to_owned(),
            description: "Query the component".to_owned(),
            command: "test.command".to_owned(),
            argument: "text".to_owned(),
        });
        valid.validate().unwrap();

        let mut invalid = valid.clone();
        invalid.prefixes[0].command = "missing.command".to_owned();
        assert!(matches!(
            invalid.validate(),
            Err(PluginError::InvalidComponent(message))
                if message.contains("undeclared command")
        ));
    }

    fn component_with_command(function: &str, body: &str) -> String {
        format!(
            r#"(component
                (type $host (instance))
                (import "near:plugin/host@0.1.0" (instance $host-import (type $host)))
                (core module $m
                    (func (export "{function}") {body})
                )
                (core instance $i (instantiate $m))
                (func $command (canon lift (core func $i "{function}")))
                (instance (export "near:plugin/commands@0.1.0")
                    (export "{function}" (func $command)))
                (instance (export "near:plugin/provider@0.1.0"))
            )"#
        )
    }

    fn component_with_memory(pages: u32) -> String {
        format!(
            r#"(component
                (type $host (instance))
                (import "near:plugin/host@0.1.0" (instance $host-import (type $host)))
                (core module $m (memory {pages}))
                (core instance $i (instantiate $m))
                (instance (export "near:plugin/commands@0.1.0"))
                (instance (export "near:plugin/provider@0.1.0"))
            )"#
        )
    }

    fn write_component(source: &str, name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "near-plugin-{}-{name}-{nonce}.wat",
            std::process::id()
        ));
        fs::write(&path, source).unwrap();
        path
    }

    fn call_test_function(plugin: &LoadedPlugin, function: &str) -> Result<(), PluginError> {
        let mut runtime = plugin.instantiate()?;
        let function = runtime.exported_function("near:plugin/commands@0.1.0", function)?;
        function
            .call(&mut runtime.store, &[], &mut [])
            .map_err(PluginError::runtime)?;
        function
            .post_return(&mut runtime.store)
            .map_err(PluginError::runtime)
    }

    fn call_test_u32(
        plugin: &LoadedPlugin,
        function: &str,
    ) -> Result<(u32, Vec<PluginEvent>), PluginError> {
        let mut runtime = plugin.instantiate()?;
        let function = runtime.exported_function("near:plugin/commands@0.1.0", function)?;
        let mut results = [Val::U32(0)];
        function
            .call(&mut runtime.store, &[], &mut results)
            .map_err(PluginError::runtime)?;
        function
            .post_return(&mut runtime.store)
            .map_err(PluginError::runtime)?;
        match results[0] {
            Val::U32(value) => Ok((value, runtime.store.data().events.clone())),
            ref value => Err(PluginError::InvalidComponent(format!(
                "probe returned {value:?} instead of u32"
            ))),
        }
    }

    #[test]
    fn published_wit_is_versioned_and_parseable() {
        assert_eq!(validate_wit_contract(WIT).unwrap(), Version::new(0, 1, 0));
    }

    #[test]
    fn manifests_and_grants_are_versioned_inspectable_and_revocable() {
        let document = r#"
schema = 1
id = "example.archive"
name = "Archive"
version = "1.2.3"
interface = "^0.1.0"
capabilities = ["near.resource.read@1"]
commands = ["archive.open"]
providers = ["archive"]

[limits]
memory_bytes = 1048576
fuel = 50000
timeout_ms = 200
"#;
        let manifest = PluginManifest::from_toml(document).unwrap();
        assert_eq!(manifest.version, Version::new(1, 2, 3));
        let mut grants = CapabilityGrantStore::default();
        grants.grant(&manifest.id, CAPABILITY_RESOURCE_READ_V1);
        assert!(
            grants
                .grants_for(&manifest.id)
                .contains(CAPABILITY_RESOURCE_READ_V1)
        );
        assert!(grants.revoke(&manifest.id, CAPABILITY_RESOURCE_READ_V1));
        assert!(grants.grants_for(&manifest.id).is_empty());
    }

    #[test]
    fn incompatible_manifest_and_component_interfaces_are_rejected() {
        let mut incompatible_manifest = manifest(PluginLimits::default());
        incompatible_manifest.interface = VersionReq::parse("^0.2.0").unwrap();
        assert!(matches!(
            incompatible_manifest.validate(),
            Err(PluginError::IncompatibleInterface { .. })
        ));

        let source = include_str!("../tests/fixtures/host-calls-component.wat")
            .replace("near:plugin/host@0.1.0", "near:plugin/host@0.2.0");
        let path = write_component(&source, "future-host");
        let host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        let error = host
            .load(
                manifest(PluginLimits::default()),
                PluginOrigin::Installed,
                &path,
            )
            .err()
            .unwrap();
        assert!(matches!(error, PluginError::InvalidComponent(_)));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn workspace_components_require_explicit_trust() {
        let path = write_component(&component_with_command("ok", ""), "trust");
        let host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        let error = host
            .load(
                manifest(PluginLimits::default()),
                PluginOrigin::Workspace,
                &path,
            )
            .err()
            .unwrap();
        assert!(matches!(error, PluginError::WorkspaceTrustRequired(_)));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_guest_trap_is_isolated_and_a_later_component_still_runs() {
        let host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        let crash_path = write_component(&component_with_command("crash", "unreachable"), "crash");
        let crash = host
            .load(
                manifest(PluginLimits::default()),
                PluginOrigin::Installed,
                &crash_path,
            )
            .unwrap();
        assert!(call_test_function(&crash, "crash").is_err());

        let healthy_path = write_component(&component_with_command("ok", ""), "healthy");
        let healthy = host
            .load(
                manifest(PluginLimits::default()),
                PluginOrigin::Installed,
                &healthy_path,
            )
            .unwrap();
        call_test_function(&healthy, "ok").unwrap();
        fs::remove_file(crash_path).unwrap();
        fs::remove_file(healthy_path).unwrap();
    }

    #[test]
    fn fuel_and_memory_limits_are_enforced() {
        let host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        let spin_path = write_component(&component_with_command("spin", "(loop $l br $l)"), "spin");
        let spin = host
            .load(
                manifest(PluginLimits {
                    fuel: 1_000,
                    timeout_ms: 2_000,
                    ..PluginLimits::default()
                }),
                PluginOrigin::Installed,
                &spin_path,
            )
            .unwrap();
        assert!(call_test_function(&spin, "spin").is_err());

        let memory_path = write_component(&component_with_memory(1), "memory");
        let memory = host
            .load(
                manifest(PluginLimits {
                    memory_bytes: 1_024,
                    ..PluginLimits::default()
                }),
                PluginOrigin::Installed,
                &memory_path,
            )
            .unwrap();
        assert!(memory.instantiate().is_err());
        fs::remove_file(spin_path).unwrap();
        fs::remove_file(memory_path).unwrap();
    }

    #[test]
    fn undeclared_or_ungranted_host_authority_is_denied_structurally() {
        let state = HostState {
            plugin: "test.component".to_owned(),
            declared: BTreeSet::new(),
            granted: BTreeSet::new(),
            reader: None,
            events: Vec::new(),
            limits: StoreLimitsBuilder::new().build(),
        };
        assert_eq!(
            state.require(CAPABILITY_RESOURCE_READ_V1).unwrap_err(),
            "plugin test.component did not declare capability near.resource.read@1"
        );
    }

    struct FixtureReader;

    impl PluginResourceReader for FixtureReader {
        fn read(
            &self,
            resource: &ResourceRef,
            offset: u64,
            length: u32,
        ) -> Result<Vec<u8>, String> {
            assert_eq!(resource.provider.as_str(), "fixture");
            assert_eq!(resource.location.as_str(), "fixture://item");
            assert_eq!(offset, 2);
            assert_eq!(length, 3);
            Ok(vec![2, 3, 4])
        }
    }

    #[test]
    fn checked_in_guest_calls_host_imports_through_the_canonical_abi() {
        let source = include_str!("../tests/fixtures/host-calls-component.wat");
        let diagnostic_host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        Component::new(&diagnostic_host.engine, source).unwrap();
        let path = write_component(source, "host-calls");
        let mut manifest = manifest(PluginLimits::default());
        let host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        let plugin = host
            .load(manifest.clone(), PluginOrigin::Installed, &path)
            .unwrap();
        for function in ["probe-log", "probe-notify", "probe-read"] {
            let (result, events) = call_test_u32(&plugin, function).unwrap();
            assert_eq!(result, 1, "{function} should reject undeclared authority");
            assert!(events.is_empty());
        }

        manifest.capabilities = BTreeSet::from([
            CAPABILITY_LOG_V1.to_owned(),
            CAPABILITY_NOTIFY_V1.to_owned(),
            CAPABILITY_RESOURCE_READ_V1.to_owned(),
        ]);
        let host = WasmPluginHost::new(CapabilityGrantStore::default()).unwrap();
        let plugin = host
            .load(manifest.clone(), PluginOrigin::Installed, &path)
            .unwrap();
        for function in ["probe-log", "probe-notify", "probe-read"] {
            let (result, events) = call_test_u32(&plugin, function).unwrap();
            assert_eq!(result, 1, "{function} should be denied");
            assert!(events.is_empty());
        }

        let mut grants = CapabilityGrantStore::default();
        for capability in [
            CAPABILITY_LOG_V1,
            CAPABILITY_NOTIFY_V1,
            CAPABILITY_RESOURCE_READ_V1,
        ] {
            grants.grant(&manifest.id, capability);
        }
        let host = WasmPluginHost::new(grants)
            .unwrap()
            .with_resource_reader(Arc::new(FixtureReader));
        let plugin = host.load(manifest, PluginOrigin::Installed, &path).unwrap();
        let (result, events) = call_test_u32(&plugin, "probe-log").unwrap();
        assert_eq!(result, 0);
        assert_eq!(
            events,
            [PluginEvent::Log {
                level: "info".to_owned(),
                message: "canonical host call".to_owned(),
            }]
        );
        let (result, events) = call_test_u32(&plugin, "probe-notify").unwrap();
        assert_eq!(result, 0);
        assert_eq!(
            events,
            [PluginEvent::Notification {
                severity: "warning".to_owned(),
                message: "canonical notification".to_owned(),
            }]
        );
        let (result, events) = call_test_u32(&plugin, "probe-read").unwrap();
        assert_eq!(result, 0);
        assert!(events.is_empty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn provider_results_adapt_to_universal_resource_pages() {
        let request = ListRequest {
            generation: near_core::ListingGeneration(7),
            continuation: None,
            page_size: 25,
            cancellation: near_core::CancellationToken::default(),
        };
        let item = Val::Record(vec![
            (
                "reference".to_owned(),
                Val::Record(vec![
                    (
                        "uri".to_owned(),
                        Val::String("archive://root/a.txt".to_owned()),
                    ),
                    (
                        "provider".to_owned(),
                        Val::String("archive.test".to_owned()),
                    ),
                ]),
            ),
            ("name".to_owned(), Val::String("a.txt".to_owned())),
            ("kind".to_owned(), Val::String("file".to_owned())),
            ("size".to_owned(), Val::Option(Some(Box::new(Val::U64(12))))),
            (
                "modified-unix-ms".to_owned(),
                Val::Option(Some(Box::new(Val::S64(34)))),
            ),
            (
                "capabilities".to_owned(),
                Val::List(vec![Val::String("resource.read".to_owned())]),
            ),
        ]);
        let page = decode_list_page(
            Val::Result(Ok(Some(Box::new(Val::Tuple(vec![
                Val::List(vec![item]),
                Val::Option(None),
            ]))))),
            &request,
        )
        .unwrap();
        assert_eq!(page.generation, near_core::ListingGeneration(7));
        assert!(page.complete);
        assert_eq!(page.entries[0].metadata.name, "a.txt");
        assert_eq!(page.entries[0].metadata.kind, ResourceKind::File);
        assert_eq!(page.entries[0].metadata.size, Some(12));
    }
}
