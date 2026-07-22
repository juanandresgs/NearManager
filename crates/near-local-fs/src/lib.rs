//! Cross-platform local filesystem provider with reversible path identities.

#![allow(
    clippy::blocks_in_conditions,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::format_collect,
    clippy::missing_errors_doc
)]

mod device;

pub use device::PlatformRemovableDeviceService;

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt::Write,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "macos")]
use std::thread;

#[cfg(unix)]
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
};
#[cfg(windows)]
use std::os::windows::{
    ffi::{OsStrExt, OsStringExt},
    fs::MetadataExt,
};

use near_core::MetadataValue;
#[cfg(unix)]
use near_core::OwnerSummary;
use near_core::{
    CapabilitySet, Clipboard, CommandHistoryEntry, CommandHistoryStore,
    CommandLineArgumentResolver, CommandLineExecutor, CommandLineOutput, EditorPositionEntry,
    EditorPositionStore, ExternalAction, ExternalInvocation, ExternalResolution,
    ExternalToolResolver, FolderNavigationState, FolderNavigationStore, ListPage, ListRequest,
    Location, MutationAlternative, MutationDenial, MutationEligibility, MutationKind, OpenRequest,
    PermissionSummary, ProviderError, ProviderFuture, ProviderId, ProviderLocation,
    RESOURCE_DESCRIPTION_KEY, ResourceClassification, ResourceEntry, ResourceHistoryState,
    ResourceHistoryStore, ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef,
    ResourceStream, StateDocumentStore, ViewerStateEntry, ViewerStateStore, WriteRequest,
};
use near_handlers::{
    HANDLER_SCHEMA_VERSION, HandlerContext, HandlerDiagnostic, HandlerDocument,
    HandlerInvocationTemplate, HandlerRegistry, HandlerRule, HandlerValue,
};
use near_ops::{
    AttributeUpdate, ConflictAction, CrossDeviceBehavior, ElevatedOperationRequest,
    ExecutionAuthorization, ExecutionEffect, ExecutionSummary, LinkKind, MetadataPolicy,
    OperationBackend, OperationEngine, OperationIntent, OperationJournal, OperationKind,
    OperationPlan, OperationPlanner, OperationService, PlanError, PlanPolicies, PlanRequest,
    PlannedItem, RecoveryPolicy, SymlinkPolicy, VerificationPolicy,
};
use near_search::{HiddenPolicy, ResourcePredicate};
use sha2::{Digest, Sha256};

const SCHEMES: &[&str] = &["file"];
const MAX_READ_LENGTH: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct LocalFileProvider;

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalCommandLineExecutor;

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalCommandLineArgumentResolver;

impl CommandLineArgumentResolver for LocalCommandLineArgumentResolver {
    fn quote_text(&self, value: &str) -> String {
        quote_command_argument(value)
    }

    fn location_argument(&self, location: &Location) -> Result<String, String> {
        let path = LocalFileProvider::path(location).map_err(|_| {
            format!(
                "provider location {} has no native command-line path",
                location.as_str()
            )
        })?;
        quote_native_path(&path)
    }

    fn resource_argument(&self, resource: &ResourceRef) -> Result<String, String> {
        if resource.provider.as_str() != "near.local-fs" {
            return Err(format!(
                "provider {} has no native command-line path",
                resource.provider
            ));
        }
        let path =
            LocalFileProvider::path(&resource.location).map_err(|error| error.to_string())?;
        quote_native_path(&path)
    }

    fn native_working_directory(&self, location: &Location) -> Result<Option<PathBuf>, String> {
        LocalFileProvider::path(location)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

fn quote_native_path(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(quote_command_argument)
        .ok_or_else(|| "native path cannot be represented in the command shell encoding".to_owned())
}

#[cfg(not(windows))]
fn quote_command_argument(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-/".contains(character))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn quote_command_argument(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-/:\\".contains(character))
    {
        return value.to_owned();
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[derive(Clone, Debug)]
pub struct LocalCommandHistoryStore {
    path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LocalFolderNavigationStore {
    path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LocalEditorPositionStore {
    path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LocalViewerStateStore {
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalClipboard;

#[derive(Clone, Debug)]
pub struct LocalResourceHistoryStore {
    path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LocalStateDocumentStore {
    root: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CommandHistoryDocument {
    schema_version: u32,
    #[serde(default)]
    entries: Vec<CommandHistoryEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FolderNavigationDocument {
    schema_version: u32,
    #[serde(default)]
    history: Vec<near_core::FolderLocationEntry>,
    #[serde(default)]
    shortcuts: Vec<FolderShortcutDocument>,
    #[serde(default = "default_folder_history_limit")]
    max_unlocked: usize,
}

const fn default_folder_history_limit() -> usize {
    200
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FolderShortcutDocument {
    slot: usize,
    entry: near_core::FolderLocationEntry,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EditorPositionDocument {
    schema_version: u32,
    #[serde(default)]
    entries: Vec<EditorPositionEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ViewerStateDocument {
    schema_version: u32,
    #[serde(default)]
    entries: Vec<ViewerStateEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ResourceHistoryDocument {
    schema_version: u32,
    #[serde(flatten)]
    state: ResourceHistoryState,
}

impl LocalCommandHistoryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalFolderNavigationStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalEditorPositionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalViewerStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalResourceHistoryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalStateDocumentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, document: &str) -> Result<PathBuf, String> {
        let path = Path::new(document);
        if path.components().count() != 1 || path.file_name().is_none() {
            return Err(format!("invalid state document name: {document}"));
        }
        Ok(self.root.join(path))
    }
}

impl StateDocumentStore for LocalStateDocumentStore {
    fn load(&self, document: &str) -> Result<Option<String>, String> {
        let path = self.path(document)?;
        match fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let backup = path.with_extension("toml.bak");
                match fs::read_to_string(backup) {
                    Ok(contents) => Ok(Some(contents)),
                    Err(backup_error) if backup_error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(None)
                    }
                    Err(backup_error) => Err(backup_error.to_string()),
                }
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn persist(&self, document: &str, contents: &str) -> Result<(), String> {
        use std::io::Write;

        let path = self.path(document)?;
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let temporary = self.root.join(format!(
            ".{}.near-tmp-{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("state"),
            std::process::id()
        ));
        let backup = path.with_extension("toml.bak");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(contents.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        if path.exists() {
            if backup.exists() {
                fs::remove_file(&backup).map_err(|error| error.to_string())?;
            }
            fs::rename(&path, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&temporary, &path) {
            if backup.exists() && !path.exists() {
                let _ = fs::rename(&backup, &path);
            }
            return Err(error.to_string());
        }
        Ok(())
    }
}

impl CommandHistoryStore for LocalCommandHistoryStore {
    fn load(&self) -> Result<Vec<CommandHistoryEntry>, String> {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let document: CommandHistoryDocument =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        if document.schema_version != 1 {
            return Err(format!(
                "unsupported command history schema {}",
                document.schema_version
            ));
        }
        Ok(document.entries)
    }

    fn save(&self, entries: &[CommandHistoryEntry]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let document = CommandHistoryDocument {
            schema_version: 1,
            entries: entries.to_vec(),
        };
        let source = toml::to_string_pretty(&document).map_err(|error| error.to_string())?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temporary, source).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

impl ResourceHistoryStore for LocalResourceHistoryStore {
    fn load(&self) -> Result<ResourceHistoryState, String> {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ResourceHistoryState::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        let document: ResourceHistoryDocument =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        if document.schema_version != 1 {
            return Err(format!(
                "unsupported resource history schema {}",
                document.schema_version
            ));
        }
        Ok(document.state)
    }

    fn save(&self, state: &ResourceHistoryState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let document = ResourceHistoryDocument {
            schema_version: 1,
            state: state.clone(),
        };
        let source = toml::to_string_pretty(&document).map_err(|error| error.to_string())?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temporary, source).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

impl FolderNavigationStore for LocalFolderNavigationStore {
    fn load(&self) -> Result<FolderNavigationState, String> {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(FolderNavigationState::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        let document: FolderNavigationDocument =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        if document.schema_version != 1 {
            return Err(format!(
                "unsupported folder navigation schema {}",
                document.schema_version
            ));
        }
        let mut shortcuts = vec![None; 10];
        for shortcut in document.shortcuts {
            if shortcut.slot < shortcuts.len() {
                shortcuts[shortcut.slot] = Some(shortcut.entry);
            }
        }
        Ok(FolderNavigationState {
            history: document.history,
            shortcuts,
            max_unlocked: document.max_unlocked,
        })
    }

    fn save(&self, state: &FolderNavigationState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let source = toml::to_string_pretty(&FolderNavigationDocument {
            schema_version: 1,
            history: state.history.clone(),
            shortcuts: state
                .shortcuts
                .iter()
                .enumerate()
                .filter_map(|(slot, entry)| {
                    entry
                        .clone()
                        .map(|entry| FolderShortcutDocument { slot, entry })
                })
                .collect(),
            max_unlocked: state.max_unlocked,
        })
        .map_err(|error| error.to_string())?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temporary, source).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

impl EditorPositionStore for LocalEditorPositionStore {
    fn load(&self) -> Result<Vec<EditorPositionEntry>, String> {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let document: EditorPositionDocument =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        if document.schema_version != 1 {
            return Err(format!(
                "unsupported editor position schema {}",
                document.schema_version
            ));
        }
        Ok(document.entries)
    }

    fn save(&self, entries: &[EditorPositionEntry]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let source = toml::to_string_pretty(&EditorPositionDocument {
            schema_version: 1,
            entries: entries.to_vec(),
        })
        .map_err(|error| error.to_string())?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temporary, source).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

impl ViewerStateStore for LocalViewerStateStore {
    fn load(&self) -> Result<Vec<ViewerStateEntry>, String> {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let document: ViewerStateDocument =
            toml::from_str(&source).map_err(|error| error.to_string())?;
        if document.schema_version != 1 {
            return Err(format!(
                "unsupported viewer state schema {}",
                document.schema_version
            ));
        }
        Ok(document.entries)
    }

    fn save(&self, entries: &[ViewerStateEntry]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let source = toml::to_string_pretty(&ViewerStateDocument {
            schema_version: 1,
            entries: entries.to_vec(),
        })
        .map_err(|error| error.to_string())?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temporary, source).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

impl Clipboard for LocalClipboard {
    fn set_text(&self, text: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            return write_process_stdin(&mut Command::new("pbcopy"), text);
        }
        #[cfg(windows)]
        {
            return write_process_stdin(&mut Command::new("clip.exe"), text);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let candidates: [(&str, &[&str]); 3] = [
                ("wl-copy", &[]),
                ("xclip", &["-selection", "clipboard"]),
                ("xsel", &["--clipboard", "--input"]),
            ];
            let mut diagnostics = Vec::new();
            for (program, arguments) in candidates {
                let mut command = Command::new(program);
                command.args(arguments);
                match write_process_stdin(&mut command, text) {
                    Ok(()) => return Ok(()),
                    Err(error) => diagnostics.push(format!("{program}: {error}")),
                }
            }
            return Err(format!(
                "no usable platform clipboard command ({})",
                diagnostics.join("; ")
            ));
        }
        #[allow(unreachable_code)]
        Err("platform clipboard is unsupported".to_owned())
    }
}

fn write_process_stdin(command: &mut Command, text: &str) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    std::io::Write::write_all(
        &mut child
            .stdin
            .take()
            .ok_or_else(|| "clipboard process has no stdin".to_owned())?,
        text.as_bytes(),
    )
    .map_err(|error| error.to_string())?;
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

impl CommandLineExecutor for LocalCommandLineExecutor {
    fn execute(&self, location: &Location, command: &str) -> Result<CommandLineOutput, String> {
        let directory = LocalFileProvider::path(location).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        let mut process = {
            let mut process =
                Command::new(std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into()));
            process.args(["/D", "/S", "/C", command]);
            process
        };
        #[cfg(not(windows))]
        let mut process = {
            let shell = std::env::var_os("NEAR_SHELL")
                .or_else(|| std::env::var_os("SHELL"))
                .unwrap_or_else(|| "/bin/sh".into());
            let mut process = Command::new(shell);
            process.args(["-lc", command]);
            process
        };
        let output = process
            .current_dir(directory)
            .output()
            .map_err(|error| error.to_string())?;
        Ok(CommandLineOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalExternalToolResolver {
    registry: HandlerRegistry,
}

impl LocalExternalToolResolver {
    /// Loads a versioned local handler document.
    ///
    /// # Errors
    ///
    /// Returns TOML diagnostics with source location or handler-schema validation errors.
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let document =
            toml::from_str::<HandlerDocument>(source).map_err(|error| error.to_string())?;
        HandlerRegistry::new(document)
            .map(|registry| Self { registry })
            .map_err(|error| error.to_string())
    }

    /// Creates the built-in structured local-file rule.
    ///
    /// # Panics
    ///
    /// Panics only if the statically constructed built-in rule violates the handler schema.
    pub fn new(
        program: impl Into<OsString>,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        let program = program.into().to_string_lossy().into_owned();
        let mut template_arguments = arguments
            .into_iter()
            .map(Into::into)
            .map(|argument: OsString| {
                HandlerValue::Literal(argument.to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>();
        template_arguments.push(HandlerValue::NativePath);
        let registry = HandlerRegistry::new(HandlerDocument {
            schema_version: HANDLER_SCHEMA_VERSION,
            handlers: vec![HandlerRule {
                id: "near.handler.local-text".to_owned(),
                actions: vec![
                    ExternalAction::Open,
                    ExternalAction::View,
                    ExternalAction::Edit,
                    ExternalAction::Inspect,
                ],
                predicate: ResourcePredicate {
                    hidden: HiddenPolicy::Include,
                    ..ResourcePredicate::default()
                },
                invocation: HandlerInvocationTemplate::Argv {
                    program,
                    arguments: template_arguments,
                    current_directory: Some(HandlerValue::NativeParent),
                },
            }],
        })
        .expect("built-in local handler rule must be valid");
        Self { registry }
    }

    pub fn macos_text_editor() -> Self {
        std::env::var_os("VISUAL")
            .or_else(|| std::env::var_os("EDITOR"))
            .map_or_else(
                || Self::new("/usr/bin/open", ["-W", "-t"]),
                |program| Self::new(program, std::iter::empty::<OsString>()),
            )
    }

    /// Explains which configured rule matches a local resource and why.
    ///
    /// # Errors
    ///
    /// Returns an error when the resource is not a local file URI.
    pub fn diagnose(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<HandlerDiagnostic, String> {
        let context = Self::context(resource)?;
        Ok(self.registry.diagnose(action, &context))
    }

    fn context(resource: &ResourceRef) -> Result<HandlerContext, String> {
        if resource.provider.as_str() != "near.local-fs" {
            return Err("the configured local handler only accepts local files".to_owned());
        }
        let path =
            LocalFileProvider::path(&resource.location).map_err(|error| error.to_string())?;
        let metadata = LocalFileProvider::rich_metadata(&path).unwrap_or_else(|_| {
            let file_name = path.file_name().unwrap_or(path.as_os_str());
            ResourceMetadata {
                name: escape_name(file_name),
                kind: ResourceKind::File,
                hidden: Some(name_is_dot_hidden(file_name)),
                ..ResourceMetadata::default()
            }
        });
        Ok(HandlerContext {
            resource: resource.clone(),
            metadata,
            native_path: Some(path),
        })
    }
}

impl ExternalToolResolver for LocalExternalToolResolver {
    fn resolve(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<ExternalInvocation, String> {
        self.resolve_explained(action, resource)
            .map(|resolution| resolution.invocation)
    }

    fn resolve_explained(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<ExternalResolution, String> {
        self.registry
            .resolve(action, &Self::context(resource)?)
            .map_err(|error| error.to_string())
    }

    fn alternatives(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
    ) -> Result<Vec<ExternalResolution>, String> {
        self.registry
            .resolve_all(action, &Self::context(resource)?)
            .map_err(|error| error.to_string())
    }

    fn resolve_named(
        &self,
        action: ExternalAction,
        resource: &ResourceRef,
        handler_id: &str,
    ) -> Result<ExternalResolution, String> {
        self.registry
            .resolve_named(action, &Self::context(resource)?, handler_id)
            .map_err(|error| error.to_string())
    }

    fn diagnose(&self, action: ExternalAction, resource: &ResourceRef) -> Result<String, String> {
        let diagnostic = self.diagnose(action, resource)?;
        let evaluations = diagnostic
            .evaluations
            .iter()
            .map(|evaluation| {
                format!(
                    "{}: {}{}",
                    evaluation.handler_id,
                    evaluation.reason,
                    if evaluation.selected {
                        " [selected]"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!("{}\n{evaluations}", diagnostic.explanation()))
    }
}

/// Encodes a native path as a local provider location.
pub fn local_location(path: &Path) -> Location {
    LocalFileProvider::location(path)
}

/// Decodes a local file location without depending on the concrete provider type.
///
/// # Errors
///
/// Returns an error for non-file schemes or malformed percent escapes.
pub fn local_path(location: &Location) -> Result<PathBuf, ProviderError> {
    LocalFileProvider::path(location)
}

/// Creates a local provider resource reference for a native path.
pub fn local_resource(path: &Path) -> ResourceRef {
    LocalFileProvider::resource_for_path(path)
}

impl LocalFileProvider {
    pub fn location(path: &Path) -> Location {
        Location::new(format!(
            "file://{}",
            percent_encode(&native_bytes(path.as_os_str()))
        ))
    }

    /// Decodes a provider location into its exact Unix path bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for non-file schemes or malformed percent escapes.
    pub fn path(location: &Location) -> Result<PathBuf, ProviderError> {
        let encoded = location
            .as_str()
            .strip_prefix("file://")
            .ok_or_else(|| ProviderError::Failed("location is not a file URI".to_owned()))?;
        Ok(PathBuf::from(native_string(&percent_decode(encoded)?)?))
    }

    fn resource(path: &Path) -> ResourceRef {
        ResourceRef {
            provider: ProviderId::from("near.local-fs"),
            location: Self::location(path),
        }
    }

    pub fn resource_for_path(path: &Path) -> ResourceRef {
        Self::resource(path)
    }

    fn list_entry(
        path: &Path,
        file_name: &OsStr,
        file_type: Result<fs::FileType, String>,
    ) -> ResourceEntry {
        let mut metadata = ResourceMetadata {
            name: escape_name(file_name),
            hidden: Some(name_is_dot_hidden(file_name)),
            ..ResourceMetadata::default()
        };
        match file_type {
            Ok(file_type) => metadata.kind = classify_file_type(path, file_type),
            Err(error) => {
                metadata.kind = ResourceKind::Other;
                metadata.field_errors.insert("file-type".to_owned(), error);
            }
        }
        ResourceEntry {
            resource: Self::resource(path),
            details: kind_label(metadata.kind).to_owned(),
            metadata,
        }
    }

    fn rich_metadata(path: &Path) -> Result<ResourceMetadata, ProviderError> {
        let stat = fs::symlink_metadata(path).map_err(provider_io(path))?;
        let file_name = path.file_name().unwrap_or(path.as_os_str());
        let mut metadata = platform_metadata(path, file_name, &stat);
        if stat.file_type().is_symlink() {
            match fs::read_link(path) {
                Ok(target) => {
                    let resolved = if target.is_absolute() {
                        target
                    } else {
                        path.parent().unwrap_or_else(|| Path::new("/")).join(target)
                    };
                    metadata.link_target = Some(Self::location(&resolved));
                }
                Err(error) => {
                    metadata
                        .field_errors
                        .insert("link-target".to_owned(), error.to_string());
                }
            }
        }
        collect_platform_metadata(path, &mut metadata);
        Ok(metadata)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DescriptionEncoding {
    #[default]
    Utf8,
    Utf8Bom,
    Latin1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DescriptionUpdatePolicy {
    Disabled,
    #[default]
    Always,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptionSettings {
    pub schema: u16,
    #[serde(default = "default_description_files")]
    pub description_files: Vec<String>,
    #[serde(default = "default_folder_description_files")]
    pub folder_description_files: Vec<String>,
    #[serde(default)]
    pub encoding: DescriptionEncoding,
    #[serde(default)]
    pub update_policy: DescriptionUpdatePolicy,
    #[serde(default)]
    pub show_description_files: bool,
}

impl Default for DescriptionSettings {
    fn default() -> Self {
        Self {
            schema: 1,
            description_files: default_description_files(),
            folder_description_files: default_folder_description_files(),
            encoding: DescriptionEncoding::Utf8,
            update_policy: DescriptionUpdatePolicy::Always,
            show_description_files: false,
        }
    }
}

impl DescriptionSettings {
    pub fn from_toml(source: &str) -> Result<Self, String> {
        let settings: Self = toml::from_str(source).map_err(|error| error.to_string())?;
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err(format!("unsupported description schema {}", self.schema));
        }
        if self.description_files.is_empty() || self.folder_description_files.is_empty() {
            return Err("description filename lists cannot be empty".to_owned());
        }
        for name in self
            .description_files
            .iter()
            .chain(&self.folder_description_files)
        {
            let mut components = Path::new(name).components();
            if name.trim().is_empty()
                || !matches!(components.next(), Some(std::path::Component::Normal(_)))
                || components.next().is_some()
            {
                return Err(format!("invalid description filename {name}"));
            }
        }
        Ok(())
    }
}

fn default_description_files() -> Vec<String> {
    vec!["descript.ion".to_owned(), "files.bbs".to_owned()]
}

fn default_folder_description_files() -> Vec<String> {
    vec![
        "README.md".to_owned(),
        "README.txt".to_owned(),
        "files.bbs".to_owned(),
    ]
}

#[derive(Clone, Debug)]
pub struct DescribedLocalFileProvider {
    settings: DescriptionSettings,
}

impl DescribedLocalFileProvider {
    pub fn new(settings: DescriptionSettings) -> Self {
        Self { settings }
    }

    pub fn from_toml(source: &str) -> Result<Self, String> {
        DescriptionSettings::from_toml(source).map(Self::new)
    }

    pub fn settings(&self) -> &DescriptionSettings {
        &self.settings
    }
}

fn description_file_path(directory: &Path, settings: &DescriptionSettings) -> PathBuf {
    settings
        .description_files
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| directory.join(&settings.description_files[0]))
}

fn decode_description_bytes(
    bytes: &[u8],
    encoding: DescriptionEncoding,
) -> Result<String, ProviderError> {
    if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8(bytes.to_vec())
            .map_err(|error| ProviderError::Failed(error.to_string()));
    }
    match encoding {
        DescriptionEncoding::Utf8 | DescriptionEncoding::Utf8Bom => {
            String::from_utf8(bytes.to_vec())
                .map_err(|error| ProviderError::Failed(error.to_string()))
        }
        DescriptionEncoding::Latin1 => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
    }
}

fn encode_description_text(
    text: &str,
    encoding: DescriptionEncoding,
) -> Result<Vec<u8>, ProviderError> {
    match encoding {
        DescriptionEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
        DescriptionEncoding::Utf8Bom => {
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend_from_slice(text.as_bytes());
            Ok(bytes)
        }
        DescriptionEncoding::Latin1 => text
            .chars()
            .map(|character| {
                u8::try_from(u32::from(character)).map_err(|_| {
                    ProviderError::Failed(format!(
                        "character {character:?} cannot be represented as Latin-1"
                    ))
                })
            })
            .collect(),
    }
}

fn parse_description_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
        return None;
    }
    if let Some(rest) = line.strip_prefix('"') {
        let mut escaped = false;
        let mut name = String::new();
        for (index, character) in rest.char_indices() {
            if escaped {
                name.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                return Some((name, rest[index + 1..].trim().to_owned()));
            } else {
                name.push(character);
            }
        }
        return None;
    }
    let split = line
        .char_indices()
        .find(|(_, character)| character.is_whitespace())?
        .0;
    Some((line[..split].to_owned(), line[split..].trim().to_owned()))
}

fn load_description_catalog(
    directory: &Path,
    settings: &DescriptionSettings,
) -> Result<BTreeMap<String, String>, ProviderError> {
    let path = description_file_path(directory, settings);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = fs::read(&path).map_err(provider_io(&path))?;
    let text = decode_description_bytes(&bytes, settings.encoding)?;
    Ok(text.lines().filter_map(parse_description_line).collect())
}

fn render_description_catalog(catalog: &BTreeMap<String, String>) -> String {
    let mut body = String::new();
    for (name, description) in catalog {
        let rendered_name = if name.chars().any(char::is_whitespace) || name.contains('"') {
            format!("\"{}\"", name.replace('"', "\\\""))
        } else {
            name.clone()
        };
        let _ = writeln!(body, "{rendered_name} {description}");
    }
    body
}

fn save_description_catalog(
    directory: &Path,
    settings: &DescriptionSettings,
    catalog: &BTreeMap<String, String>,
) -> Result<(), ProviderError> {
    let path = description_file_path(directory, settings);
    if catalog.is_empty() {
        if path.exists() {
            fs::remove_file(&path).map_err(provider_io(&path))?;
        }
        return Ok(());
    }
    let bytes = encode_description_text(&render_description_catalog(catalog), settings.encoding)?;
    let temporary = path.with_extension(format!("near-tmp-{}", std::process::id()));
    fs::write(&temporary, bytes).map_err(provider_io(&temporary))?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(&path).map_err(provider_io(&path))?;
    }
    fs::rename(&temporary, &path).map_err(provider_io(&path))
}

fn update_path_description(
    path: &Path,
    description: Option<String>,
    settings: &DescriptionSettings,
) -> Result<(), ProviderError> {
    if settings.update_policy == DescriptionUpdatePolicy::Disabled {
        return Err(ProviderError::Unsupported(
            "description updates are disabled".to_owned(),
        ));
    }
    let directory = path
        .parent()
        .ok_or_else(|| ProviderError::Failed("resource has no parent folder".to_owned()))?;
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| ProviderError::Failed("description filenames must be Unicode".to_owned()))?;
    let mut catalog = load_description_catalog(directory, settings)?;
    match description.filter(|description| !description.trim().is_empty()) {
        Some(description) => {
            catalog.insert(name.to_owned(), description);
        }
        None => {
            catalog.remove(name);
        }
    }
    save_description_catalog(directory, settings, &catalog)
}

impl ResourceProvider for LocalFileProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from("near.local-fs")
    }

    fn schemes(&self) -> &[&str] {
        SCHEMES
    }

    fn location_label(&self, location: &Location) -> String {
        Self::path(location).map_or_else(
            |_| location.as_str().to_owned(),
            |path| escape_name(path.file_name().unwrap_or(path.as_os_str())),
        )
    }

    fn parse_native_reference(
        &self,
        reference: &str,
    ) -> Result<Option<ResourceRef>, ProviderError> {
        let path = PathBuf::from(reference);
        Ok(path.is_absolute().then(|| Self::resource_for_path(&path)))
    }

    fn command_prefixes(&self) -> Vec<near_core::CommandPrefixDescriptor> {
        vec![near_core::CommandPrefixDescriptor {
            name: "file".to_owned(),
            description: "Navigate the focused panel to a native path or file URI".to_owned(),
        }]
    }

    fn resolve_command_prefix(
        &self,
        prefix: &str,
        arguments: &str,
        current: Option<&Location>,
    ) -> Result<Location, ProviderError> {
        if prefix != "file" {
            return Err(ProviderError::Unsupported(format!(
                "near.local-fs does not own command prefix {prefix}"
            )));
        }
        let target = arguments.trim();
        if target.is_empty() {
            return Err(ProviderError::Failed(
                "file: requires a native path or file URI".to_owned(),
            ));
        }
        if target.starts_with("file://") {
            let location = Location::new(target);
            Self::path(&location)?;
            return Ok(location);
        }
        let path = PathBuf::from(target);
        let path = if path.is_absolute() {
            path
        } else if let Some(current) = current {
            Self::path(current)?.join(path)
        } else {
            path
        };
        Ok(Self::location(&path))
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
            let directory = Self::path(location)?;
            let mut names = fs::read_dir(&directory)
                .map_err(provider_io(&directory))?
                .filter_map(Result::ok)
                .map(|entry| {
                    (
                        entry.file_name(),
                        entry.file_type().map_err(|error| error.to_string()),
                    )
                })
                .collect::<Vec<_>>();
            names.sort_by_key(|(name, _)| native_bytes(name));
            let offset = request
                .continuation
                .as_deref()
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| ProviderError::Failed("invalid continuation".to_owned()))?;
            let end = offset
                .saturating_add(request.page_size.max(1))
                .min(names.len());
            let entries = names[offset..end]
                .iter()
                .map(|(name, file_type)| {
                    Self::list_entry(&directory.join(name), name, file_type.clone())
                })
                .collect();
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            Ok(ListPage {
                generation: request.generation,
                entries,
                continuation: (end < names.len()).then(|| end.to_string()),
                complete: end == names.len(),
            })
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            ensure_provider(resource)?;
            Self::rich_metadata(&Self::path(&resource.location)?)
        })
    }

    fn open<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: OpenRequest,
    ) -> ProviderFuture<'a, ResourceStream> {
        Box::pin(async move {
            ensure_provider(resource)?;
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let path = Self::path(&resource.location)?;
            let mut file = File::open(&path).map_err(provider_io(&path))?;
            let total_size = file.metadata().ok().map(|metadata| metadata.len());
            file.seek(SeekFrom::Start(request.offset))
                .map_err(provider_io(&path))?;
            let mut bytes = Vec::new();
            file.by_ref()
                .take(u64::try_from(request.length.min(MAX_READ_LENGTH)).unwrap_or(u64::MAX))
                .read_to_end(&mut bytes)
                .map_err(provider_io(&path))?;
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let complete = total_size
                .is_some_and(|size| request.offset.saturating_add(bytes.len() as u64) >= size);
            Ok(ResourceStream {
                offset: request.offset,
                bytes,
                total_size,
                complete,
            })
        })
    }

    fn write<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: WriteRequest,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            ensure_provider(resource)?;
            if request.cancellation.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let path = Self::path(&resource.location)?;
            if let Some(expected) = request.expected {
                let current = Self::rich_metadata(&path)?;
                if expected.size.is_some_and(|size| current.size != Some(size))
                    || expected
                        .modified_unix_ms
                        .is_some_and(|modified| current.modified_unix_ms != Some(modified))
                {
                    return Err(ProviderError::Conflict(path.display().to_string()));
                }
            }
            let parent = path.parent().ok_or_else(|| {
                ProviderError::Failed(format!("{} has no parent", path.display()))
            })?;
            let temporary = parent.join(format!(
                ".{}.near-save-{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                std::process::id()
            ));
            fs::write(&temporary, &request.bytes).map_err(provider_io(&temporary))?;
            if request.cancellation.is_cancelled() {
                let _ = fs::remove_file(&temporary);
                return Err(ProviderError::Cancelled);
            }
            fs::rename(&temporary, &path).map_err(|error| {
                let _ = fs::remove_file(&temporary);
                provider_io(&path)(error)
            })?;
            Ok(())
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        let Ok(path) = Self::path(&resource.location) else {
            return CapabilitySet::default();
        };
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return CapabilitySet::default();
        };
        let mut capabilities = CapabilitySet::default();
        capabilities.insert("resource.inspect");
        let classification = Self::classify_resource(resource).ok();
        if matches!(
            classification,
            Some(ResourceClassification::Ordinary | ResourceClassification::Symlink)
        ) {
            capabilities.insert("resource.rename");
            capabilities.insert("resource.trash");
            capabilities.insert("resource.delete");
        }
        if metadata.is_dir() {
            capabilities.insert("resource.list");
            capabilities.insert("resource.create-directory");
        } else if metadata.is_file() {
            capabilities.insert("resource.read");
            if !metadata.permissions().readonly()
                && classification == Some(ResourceClassification::Ordinary)
            {
                capabilities.insert("resource.write");
                capabilities.insert("resource.wipe");
            }
        }
        capabilities
    }

    fn classify_resource(
        &self,
        resource: &ResourceRef,
    ) -> Result<ResourceClassification, ProviderError> {
        Self::classify_resource(resource)
    }

    fn mutation_eligibility(
        &self,
        resource: &ResourceRef,
        mutation: MutationKind,
    ) -> MutationEligibility {
        Self::mutation_eligibility(resource, mutation)
    }

    fn locations(&self) -> Vec<ProviderLocation> {
        local_provider_locations()
    }

    fn parent(&self, location: &Location) -> Option<Location> {
        let path = Self::path(location).ok()?;
        path.parent().map(Self::location)
    }
}

impl ResourceProvider for DescribedLocalFileProvider {
    fn id(&self) -> ProviderId {
        ProviderId::from("near.local-fs")
    }

    fn schemes(&self) -> &[&str] {
        SCHEMES
    }

    fn parse_native_reference(
        &self,
        reference: &str,
    ) -> Result<Option<ResourceRef>, ProviderError> {
        LocalFileProvider.parse_native_reference(reference)
    }

    fn list<'a>(
        &'a self,
        location: &'a Location,
        request: ListRequest,
    ) -> ProviderFuture<'a, ListPage> {
        Box::pin(async move {
            let mut page = LocalFileProvider.list(location, request).await?;
            let directory = LocalFileProvider::path(location)?;
            let catalog = load_description_catalog(&directory, &self.settings)?;
            for entry in &mut page.entries {
                if let Some(description) = catalog.get(&entry.metadata.name) {
                    entry.metadata.extensions.insert(
                        RESOURCE_DESCRIPTION_KEY.to_owned(),
                        MetadataValue::String(description.clone()),
                    );
                }
            }
            if !self.settings.show_description_files {
                page.entries.retain(|entry| {
                    !self
                        .settings
                        .description_files
                        .iter()
                        .any(|name| name == &entry.metadata.name)
                });
            }
            Ok(page)
        })
    }

    fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
        Box::pin(async move {
            let mut metadata = LocalFileProvider.stat(resource).await?;
            let path = LocalFileProvider::path(&resource.location)?;
            if let (Some(directory), Some(name)) =
                (path.parent(), path.file_name().and_then(OsStr::to_str))
                && let Some(description) =
                    load_description_catalog(directory, &self.settings)?.get(name)
            {
                metadata.extensions.insert(
                    RESOURCE_DESCRIPTION_KEY.to_owned(),
                    MetadataValue::String(description.clone()),
                );
            }
            Ok(metadata)
        })
    }

    fn open<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: OpenRequest,
    ) -> ProviderFuture<'a, ResourceStream> {
        LocalFileProvider.open(resource, request)
    }

    fn write<'a>(
        &'a self,
        resource: &'a ResourceRef,
        request: WriteRequest,
    ) -> ProviderFuture<'a, ()> {
        LocalFileProvider.write(resource, request)
    }

    fn set_description<'a>(
        &'a self,
        resource: &'a ResourceRef,
        description: Option<String>,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            let path = LocalFileProvider::path(&resource.location)?;
            update_path_description(&path, description, &self.settings)
        })
    }

    fn folder_description<'a>(
        &'a self,
        location: &'a Location,
        create: bool,
    ) -> ProviderFuture<'a, Option<ResourceRef>> {
        Box::pin(async move {
            let directory = LocalFileProvider::path(location)?;
            let existing = self
                .settings
                .folder_description_files
                .iter()
                .map(|name| directory.join(name))
                .find(|path| path.is_file());
            let path = match (existing, create) {
                (Some(path), _) => path,
                (None, false) => return Ok(None),
                (None, true) => {
                    let path = directory.join(&self.settings.folder_description_files[0]);
                    fs::write(&path, encode_description_text("", self.settings.encoding)?)
                        .map_err(provider_io(&path))?;
                    path
                }
            };
            Ok(Some(LocalFileProvider::resource_for_path(&path)))
        })
    }

    fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
        LocalFileProvider.capabilities(resource)
    }

    fn classify_resource(
        &self,
        resource: &ResourceRef,
    ) -> Result<ResourceClassification, ProviderError> {
        LocalFileProvider::classify_resource(resource)
    }

    fn mutation_eligibility(
        &self,
        resource: &ResourceRef,
        mutation: MutationKind,
    ) -> MutationEligibility {
        LocalFileProvider::mutation_eligibility(resource, mutation)
    }

    fn command_prefixes(&self) -> Vec<near_core::CommandPrefixDescriptor> {
        LocalFileProvider.command_prefixes()
    }

    fn resolve_command_prefix(
        &self,
        prefix: &str,
        arguments: &str,
        current: Option<&Location>,
    ) -> Result<Location, ProviderError> {
        LocalFileProvider.resolve_command_prefix(prefix, arguments, current)
    }

    fn locations(&self) -> Vec<ProviderLocation> {
        local_provider_locations()
    }

    fn parent(&self, location: &Location) -> Option<Location> {
        LocalFileProvider.parent(location)
    }
}

fn local_provider_locations() -> Vec<ProviderLocation> {
    let mut paths = BTreeMap::<PathBuf, String>::new();
    #[cfg(windows)]
    for drive in b'A'..=b'Z' {
        let path = PathBuf::from(format!("{}:\\", char::from(drive)));
        if path.exists() {
            paths.insert(path.clone(), format!("{}:", char::from(drive)));
        }
    }
    #[cfg(not(windows))]
    paths.insert(PathBuf::from("/"), "Filesystem root".to_owned());
    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        paths.insert(home, "Home".to_owned());
    }
    #[cfg(target_os = "macos")]
    add_child_locations(&mut paths, Path::new("/Volumes"));
    #[cfg(target_os = "linux")]
    {
        add_child_locations(&mut paths, Path::new("/mnt"));
        add_child_locations(&mut paths, Path::new("/media"));
        if let Some(user) = std::env::var_os("USER") {
            add_child_locations(&mut paths, &Path::new("/run/media").join(user));
        }
    }
    paths
        .into_iter()
        .map(|(path, label)| {
            let detail = fs::metadata(&path).map_or_else(
                |_| "native location".to_owned(),
                |metadata| {
                    format!(
                        "native directory • {}",
                        if metadata.permissions().readonly() {
                            "read-only"
                        } else {
                            "writable"
                        }
                    )
                },
            );
            ProviderLocation {
                location: LocalFileProvider::location(&path),
                label,
                detail,
            }
        })
        .collect()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn add_child_locations(paths: &mut BTreeMap<PathBuf, String>, root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            paths.insert(path, entry.file_name().to_string_lossy().into_owned());
        }
    }
}

pub fn escape_name(name: &OsStr) -> String {
    if let Some(text) = name.to_str() {
        return text
            .chars()
            .flat_map(|character| match character {
                '\\' => "\\\\".chars().collect::<Vec<_>>(),
                character if character.is_control() => {
                    format!("\\x{:02X}", character as u32).chars().collect()
                }
                character => vec![character],
            })
            .collect();
    }
    native_bytes(name)
        .iter()
        .map(|byte| match byte {
            b' '..=b'~' if *byte != b'\\' => char::from(*byte).to_string(),
            b'\\' => "\\\\".to_owned(),
            _ => format!("\\x{byte:02X}"),
        })
        .collect()
}

/// Reverses `escape_name` into exact Unix filename bytes.
///
/// # Errors
///
/// Returns an error for malformed hexadecimal escapes.
pub fn unescape_name(display: &str) -> Result<OsString, ProviderError> {
    let mut bytes = Vec::new();
    let source = display.as_bytes();
    let mut index = 0;
    while index < source.len() {
        if source[index] != b'\\' {
            let character = display[index..]
                .chars()
                .next()
                .ok_or_else(|| ProviderError::Failed("invalid display name".to_owned()))?;
            let mut encoded = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            index += character.len_utf8();
        } else if source.get(index + 1) == Some(&b'\\') {
            bytes.push(b'\\');
            index += 2;
        } else if source.get(index + 1) == Some(&b'x') {
            let hex = display
                .get(index + 2..index + 4)
                .ok_or_else(|| ProviderError::Failed("truncated filename escape".to_owned()))?;
            bytes.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| ProviderError::Failed("invalid filename escape".to_owned()))?,
            );
            index += 4;
        } else {
            return Err(ProviderError::Failed("invalid filename escape".to_owned()));
        }
    }
    native_string(&bytes)
}

fn classify(path: &Path, metadata: &Metadata) -> ResourceKind {
    classify_file_type(path, metadata.file_type())
}

fn classify_file_type(path: &Path, file_type: fs::FileType) -> ResourceKind {
    if file_type.is_symlink() {
        ResourceKind::Symlink
    } else if file_type.is_dir() && is_package(path) {
        ResourceKind::Package
    } else if file_type.is_dir() {
        ResourceKind::Directory
    } else if file_type.is_file() {
        ResourceKind::File
    } else {
        ResourceKind::Other
    }
}

fn is_package(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(OsStr::to_str) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "app" | "bundle" | "framework" | "kext" | "pkg" | "plugin"
    )
}

fn kind_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Directory => "directory",
        ResourceKind::Package => "package",
        ResourceKind::Symlink => "symlink",
        ResourceKind::File => "file",
        ResourceKind::Virtual => "virtual",
        ResourceKind::Other => "other",
        _ => "resource",
    }
}

#[cfg(unix)]
fn unix_millis(seconds: i64, nanoseconds: i64) -> Option<i64> {
    seconds
        .checked_mul(1_000)?
        .checked_add(nanoseconds.checked_div(1_000_000)?)
}

#[cfg(unix)]
fn native_bytes(value: &OsStr) -> Vec<u8> {
    value.as_bytes().to_vec()
}

#[cfg(windows)]
fn native_bytes(value: &OsStr) -> Vec<u8> {
    value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
fn native_string(bytes: &[u8]) -> Result<OsString, ProviderError> {
    Ok(OsString::from_vec(bytes.to_vec()))
}

#[cfg(windows)]
fn native_string(bytes: &[u8]) -> Result<OsString, ProviderError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(ProviderError::Failed(
            "Windows path encoding has an odd byte length".to_owned(),
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Ok(OsString::from_wide(&units))
}

fn name_is_dot_hidden(value: &OsStr) -> bool {
    value.to_string_lossy().starts_with('.')
}

#[cfg(unix)]
fn platform_metadata(path: &Path, file_name: &OsStr, stat: &Metadata) -> ResourceMetadata {
    let mode = stat.mode();
    ResourceMetadata {
        name: escape_name(file_name),
        kind: classify(path, stat),
        size: Some(stat.len()),
        modified_unix_ms: unix_millis(stat.mtime(), stat.mtime_nsec()),
        created_unix_ms: stat
            .created()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_millis()).ok()),
        accessed_unix_ms: unix_millis(stat.atime(), stat.atime_nsec()),
        stable_id: Some(format!("unix:{}:{}", stat.dev(), stat.ino())),
        permissions: Some(PermissionSummary {
            unix_mode: Some(mode),
            readonly: stat.permissions().readonly(),
            executable: mode & 0o111 != 0,
        }),
        owner: Some(OwnerSummary {
            user_id: Some(stat.uid()),
            group_id: Some(stat.gid()),
            user_name: None,
            group_name: None,
        }),
        hidden: Some(name_is_dot_hidden(file_name)),
        ..ResourceMetadata::default()
    }
}

#[cfg(windows)]
fn platform_metadata(path: &Path, file_name: &OsStr, stat: &Metadata) -> ResourceMetadata {
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let attributes = stat.file_attributes();
    let mut extensions = BTreeMap::new();
    extensions.insert(
        "windows.file-attributes".to_owned(),
        MetadataValue::Integer(i64::from(attributes)),
    );
    extensions.insert(
        "windows.reparse-point".to_owned(),
        MetadataValue::Boolean(attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0),
    );
    ResourceMetadata {
        name: escape_name(file_name),
        kind: classify(path, stat),
        size: Some(stat.file_size()),
        modified_unix_ms: windows_filetime_millis(stat.last_write_time()),
        created_unix_ms: windows_filetime_millis(stat.creation_time()),
        accessed_unix_ms: windows_filetime_millis(stat.last_access_time()),
        permissions: Some(PermissionSummary {
            unix_mode: None,
            readonly: stat.permissions().readonly(),
            executable: false,
        }),
        hidden: Some(attributes & FILE_ATTRIBUTE_HIDDEN != 0 || name_is_dot_hidden(file_name)),
        extensions,
        ..ResourceMetadata::default()
    }
}

#[cfg(windows)]
fn windows_filetime_millis(value: u64) -> Option<i64> {
    const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
    let unix_100ns = value.checked_sub(WINDOWS_TO_UNIX_EPOCH_100NS)?;
    i64::try_from(unix_100ns / 10_000).ok()
}

fn ensure_provider(resource: &ResourceRef) -> Result<(), ProviderError> {
    if resource.provider == ProviderId::from("near.local-fs") {
        Ok(())
    } else {
        Err(ProviderError::Unsupported(format!(
            "resource belongs to {}",
            resource.provider
        )))
    }
}

impl LocalFileProvider {
    pub fn classify_resource(
        resource: &ResourceRef,
    ) -> Result<ResourceClassification, ProviderError> {
        ensure_provider(resource)?;
        let path = Self::path(&resource.location)?;
        classify_local_path(&path)
    }

    pub fn mutation_eligibility(
        resource: &ResourceRef,
        mutation: MutationKind,
    ) -> MutationEligibility {
        let classification = match Self::classify_resource(resource) {
            Ok(classification) => classification,
            Err(error) => {
                return MutationEligibility::Denied(MutationDenial {
                    reason: format!("cannot classify resource before mutation: {error}"),
                    alternative: None,
                });
            }
        };
        if mutation == MutationKind::Wipe
            && classification == ResourceClassification::Ordinary
            && Self::path(&resource.location)
                .ok()
                .is_none_or(|path| !path.is_file())
        {
            return MutationEligibility::Denied(MutationDenial {
                reason: "wipe supports regular files only".to_owned(),
                alternative: None,
            });
        }
        match (classification, mutation) {
            (ResourceClassification::Ordinary | ResourceClassification::Symlink, _) => {
                MutationEligibility::Allowed
            }
            (ResourceClassification::MountRoot, _) => MutationEligibility::Denied(MutationDenial {
                reason: format!(
                    "cannot mutate mounted volume {}; eject or unmount it instead",
                    resource.location.as_str()
                ),
                alternative: Some(MutationAlternative::Unmount),
            }),
            (ResourceClassification::RemovableDevice, _) => {
                MutationEligibility::Denied(MutationDenial {
                    reason: "cannot mutate a removable device root; eject it instead".to_owned(),
                    alternative: Some(MutationAlternative::Eject),
                })
            }
            (ResourceClassification::ProviderRoot, _) => {
                MutationEligibility::Denied(MutationDenial {
                    reason: "cannot mutate a provider root; disconnect it instead".to_owned(),
                    alternative: Some(MutationAlternative::Disconnect),
                })
            }
            (ResourceClassification::FilesystemRoot, _) => {
                MutationEligibility::Denied(MutationDenial {
                    reason: "cannot mutate the filesystem root; eject or unmount it instead"
                        .to_owned(),
                    alternative: None,
                })
            }
            (ResourceClassification::VirtualRoot, _) => {
                MutationEligibility::Denied(MutationDenial {
                    reason: "cannot mutate a virtual collection root".to_owned(),
                    alternative: None,
                })
            }
            (ResourceClassification::UnsupportedSpecial, _) => {
                MutationEligibility::Denied(MutationDenial {
                    reason: "cannot mutate this unsupported special filesystem resource".to_owned(),
                    alternative: None,
                })
            }
            _ => MutationEligibility::Denied(MutationDenial {
                reason: "resource classification is not supported for this mutation".to_owned(),
                alternative: None,
            }),
        }
    }
}

fn classify_local_path(path: &Path) -> Result<ResourceClassification, ProviderError> {
    if is_platform_mount_namespace_root(path) {
        return Ok(ResourceClassification::MountRoot);
    }
    let metadata = fs::symlink_metadata(path).map_err(provider_io(path))?;
    if metadata.file_type().is_symlink() {
        return Ok(ResourceClassification::Symlink);
    }
    if path.parent().is_none() || path.parent() == Some(path) {
        return Ok(ResourceClassification::FilesystemRoot);
    }
    if is_protected_trash_root(path) {
        return Ok(ResourceClassification::MountRoot);
    }
    #[cfg(unix)]
    if {
        let file_type = metadata.file_type();
        file_type.is_block_device()
            || file_type.is_char_device()
            || file_type.is_fifo()
            || file_type.is_socket()
    } {
        return Ok(ResourceClassification::UnsupportedSpecial);
    }
    if metadata.is_file() || metadata.is_dir() {
        Ok(ResourceClassification::Ordinary)
    } else {
        Ok(ResourceClassification::UnsupportedSpecial)
    }
}

#[cfg(target_os = "macos")]
fn is_platform_mount_namespace_root(path: &Path) -> bool {
    path.parent() == Some(Path::new("/Volumes"))
}

#[cfg(not(target_os = "macos"))]
fn is_platform_mount_namespace_root(_path: &Path) -> bool {
    false
}

fn provider_io(path: &Path) -> impl FnOnce(std::io::Error) -> ProviderError + '_ {
    move |error| ProviderError::Failed(format!("{}: {error}", escape_name(path.as_os_str())))
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut encoded = String::new();
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(*byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    encoded
}

fn percent_decode(encoded: &str) -> Result<Vec<u8>, ProviderError> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = encoded
                .get(index + 1..index + 3)
                .ok_or_else(|| ProviderError::Failed("truncated file URI escape".to_owned()))?;
            decoded.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| ProviderError::Failed("invalid file URI escape".to_owned()))?,
            );
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Ok(decoded)
}

#[cfg(target_os = "macos")]
fn collect_platform_metadata(path: &Path, metadata: &mut ResourceMetadata) {
    collect_command_field(path, metadata, "macos.xattrs", "/usr/bin/xattr", &["-l"]);
    collect_command_field(
        path,
        metadata,
        "macos.finder-tags",
        "/usr/bin/mdls",
        &["-raw", "-name", "kMDItemUserTags"],
    );
    collect_command_field(path, metadata, "macos.acl", "/bin/ls", &["-lde"]);
    let quarantine = Command::new("/usr/bin/xattr")
        .args(["-p", "com.apple.quarantine"])
        .arg(path)
        .output();
    match quarantine {
        Ok(output) if output.status.success() => {
            metadata.extensions.insert(
                "macos.quarantine".to_owned(),
                MetadataValue::Bytes(output.stdout),
            );
        }
        Ok(_) => {
            metadata.extensions.insert(
                "macos.quarantine-present".to_owned(),
                MetadataValue::Boolean(false),
            );
        }
        Err(error) => {
            metadata
                .field_errors
                .insert("macos.quarantine".to_owned(), error.to_string());
        }
    }
}

#[cfg(target_os = "windows")]
fn collect_platform_metadata(path: &Path, metadata: &mut ResourceMetadata) {
    collect_windows_field(
        path,
        metadata,
        "windows.acl",
        "Get-Acl -LiteralPath $args[0] | Select-Object Owner,Group,Access | ConvertTo-Json -Compress -Depth 4",
    );
    collect_windows_field(
        path,
        metadata,
        "windows.alternate-streams",
        "Get-Item -LiteralPath $args[0] -Stream * | Select-Object Stream,Length | ConvertTo-Json -Compress",
    );
}

#[cfg(target_os = "windows")]
fn collect_windows_field(path: &Path, metadata: &mut ResourceMetadata, field: &str, script: &str) {
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .arg(path)
        .output();
    match output {
        Ok(output) if output.status.success() => {
            metadata.extensions.insert(
                field.to_owned(),
                MetadataValue::String(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            );
        }
        Ok(output) => {
            metadata.field_errors.insert(
                field.to_owned(),
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            );
        }
        Err(error) => {
            metadata
                .field_errors
                .insert(field.to_owned(), error.to_string());
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_platform_metadata(_path: &Path, _metadata: &mut ResourceMetadata) {}

#[cfg(target_os = "macos")]
fn collect_command_field(
    path: &Path,
    metadata: &mut ResourceMetadata,
    field: &str,
    program: &str,
    arguments: &[&str],
) {
    match Command::new(program).args(arguments).arg(path).output() {
        Ok(output) if output.status.success() => {
            metadata.extensions.insert(
                field.to_owned(),
                MetadataValue::String(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            );
        }
        Ok(output) => {
            metadata.field_errors.insert(
                field.to_owned(),
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            );
        }
        Err(error) => {
            metadata
                .field_errors
                .insert(field.to_owned(), error.to_string());
        }
    }
}

pub struct LocalOperationPlanner {
    planner: OperationPlanner,
    trash_directory: PathBuf,
    native_platform_trash: bool,
}

impl LocalOperationPlanner {
    pub fn new(trash_directory: PathBuf) -> Self {
        Self {
            planner: OperationPlanner::default(),
            trash_directory,
            native_platform_trash: false,
        }
    }

    pub fn macos_default() -> Self {
        Self::platform_default()
    }

    pub fn platform_default() -> Self {
        Self {
            planner: OperationPlanner::default(),
            trash_directory: platform_trash_directory(),
            native_platform_trash: cfg!(any(target_os = "macos", target_os = "windows")),
        }
    }

    pub fn classify(
        &self,
        resource: &ResourceRef,
    ) -> Result<ResourceClassification, ProviderError> {
        LocalFileProvider::classify_resource(resource)
    }

    pub fn eligibility(
        &self,
        resource: &ResourceRef,
        mutation: MutationKind,
    ) -> MutationEligibility {
        LocalFileProvider::mutation_eligibility(resource, mutation)
    }

    fn require_eligible(
        &self,
        sources: &[ResourceRef],
        mutation: MutationKind,
    ) -> Result<(), PlanError> {
        for source in sources {
            if let MutationEligibility::Denied(denial) = self.eligibility(source, mutation) {
                return Err(PlanError::Invalid(denial.reason));
            }
        }
        Ok(())
    }

    /// Plans copies into a resolved destination directory.
    ///
    /// # Errors
    ///
    /// Returns an error when sources or destination cannot form a valid immutable plan.
    pub fn copy(
        &self,
        sources: &[ResourceRef],
        destination: &Location,
        generation: near_core::ListingGeneration,
        conflict: near_ops::ConflictPolicy,
    ) -> Result<OperationPlan, PlanError> {
        self.transfer(
            OperationKind::Copy,
            sources,
            destination,
            generation,
            conflict,
        )
    }

    /// Plans moves and records whether they are atomic or copy-plus-delete.
    ///
    /// # Errors
    ///
    /// Returns an error when sources or destination cannot form a valid immutable plan.
    pub fn move_resources(
        &self,
        sources: &[ResourceRef],
        destination: &Location,
        generation: near_core::ListingGeneration,
        conflict: near_ops::ConflictPolicy,
    ) -> Result<OperationPlan, PlanError> {
        self.transfer(
            OperationKind::Move,
            sources,
            destination,
            generation,
            conflict,
        )
    }

    fn transfer(
        &self,
        kind: OperationKind,
        sources: &[ResourceRef],
        destination: &Location,
        generation: near_core::ListingGeneration,
        conflict: near_ops::ConflictPolicy,
    ) -> Result<OperationPlan, PlanError> {
        let destination_path =
            LocalFileProvider::path(destination).map_err(|_| PlanError::Empty)?;
        let items = sources
            .iter()
            .filter_map(|source| {
                let source_path = LocalFileProvider::path(&source.location).ok()?;
                let name = source_path.file_name()?;
                let target = destination_path.join(name);
                Some(PlannedItem {
                    source: Some(source.clone()),
                    target: LocalFileProvider::location(&target),
                    conflict_expected: target.exists(),
                    recursive: source_path.is_dir(),
                    parameters: BTreeMap::default(),
                })
            })
            .collect::<Vec<_>>();
        let cross_device = if kind == OperationKind::Move
            && items
                .iter()
                .any(|item| is_cross_device(item, &destination_path))
        {
            CrossDeviceBehavior::CopyThenDelete
        } else if kind == OperationKind::Move {
            CrossDeviceBehavior::AtomicRename
        } else {
            CrossDeviceBehavior::NotApplicable
        };
        self.planner.plan(PlanRequest {
            kind,
            items,
            destination: Some(destination.clone()),
            policies: PlanPolicies {
                conflict,
                metadata: MetadataPolicy::Preserve,
                verification: VerificationPolicy::SizeAndTime,
                recovery: RecoveryPolicy::Backup,
                cross_device,
                symlink: SymlinkPolicy::Preserve,
            },
            safety: near_core::SafetyClass::Confirmable,
            context_generation: generation,
            high_impact: false,
        })
    }

    /// Plans creation of one directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the target cannot form a valid immutable plan.
    pub fn create_directory(
        &self,
        target: Location,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        let conflict_expected = LocalFileProvider::path(&target).is_ok_and(|path| path.exists());
        self.planner.plan(PlanRequest {
            kind: OperationKind::CreateDirectory,
            items: vec![PlannedItem {
                source: None,
                target: target.clone(),
                conflict_expected,
                recursive: false,
                parameters: BTreeMap::default(),
            }],
            destination: Some(target),
            policies: PlanPolicies::default(),
            safety: near_core::SafetyClass::Confirmable,
            context_generation: generation,
            high_impact: false,
        })
    }

    /// Plans reversible movement into the platform Trash or Recycle Bin.
    ///
    /// # Errors
    ///
    /// Returns an error when no valid source items can be resolved.
    pub fn trash(
        &self,
        sources: &[ResourceRef],
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        self.require_eligible(sources, MutationKind::Trash)?;
        let items = sources
            .iter()
            .filter_map(|source| {
                let source_path = LocalFileProvider::path(&source.location).ok()?;
                let target = self.trash_directory.join(source_path.file_name()?);
                Some(PlannedItem {
                    source: Some(source.clone()),
                    target: LocalFileProvider::location(&target),
                    conflict_expected: !self.native_platform_trash && target.exists(),
                    recursive: source_path.is_dir(),
                    parameters: BTreeMap::default(),
                })
            })
            .collect();
        self.planner.plan(PlanRequest {
            kind: OperationKind::Trash,
            items,
            destination: Some(LocalFileProvider::location(&self.trash_directory)),
            policies: PlanPolicies {
                conflict: near_ops::ConflictPolicy::Rename,
                recovery: RecoveryPolicy::Trash,
                cross_device: CrossDeviceBehavior::CopyThenDelete,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Reversible,
            context_generation: generation,
            high_impact: false,
        })
    }

    /// Plans restoration from recorded Trash locations to original destinations.
    ///
    /// # Errors
    ///
    /// Returns an error when no valid local source and destination pairs can be resolved.
    pub fn restore(
        &self,
        restorations: &[(ResourceRef, Location)],
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        let items = restorations
            .iter()
            .filter_map(|(source, target)| {
                if source.provider != ProviderId::from("near.local-fs") {
                    return None;
                }
                let source_path = LocalFileProvider::path(&source.location).ok()?;
                let target_path = LocalFileProvider::path(target).ok()?;
                Some(PlannedItem {
                    source: Some(source.clone()),
                    target: target.clone(),
                    conflict_expected: target_path.exists(),
                    recursive: source_path.is_dir(),
                    parameters: BTreeMap::default(),
                })
            })
            .collect::<Vec<_>>();
        let cross_device = if items.iter().any(|item| {
            LocalFileProvider::path(&item.target)
                .ok()
                .and_then(|target| target.parent().map(Path::to_path_buf))
                .is_some_and(|parent| is_cross_device(item, &parent))
        }) {
            CrossDeviceBehavior::CopyThenDelete
        } else {
            CrossDeviceBehavior::AtomicRename
        };
        self.planner.plan(PlanRequest {
            kind: OperationKind::Restore,
            items,
            destination: None,
            policies: PlanPolicies {
                conflict: near_ops::ConflictPolicy::Ask,
                recovery: RecoveryPolicy::JournalOnly,
                cross_device,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Confirmable,
            context_generation: generation,
            high_impact: false,
        })
    }

    /// Plans permanent deletion with optional recursive high-impact classification.
    ///
    /// # Errors
    ///
    /// Returns an error when no valid source items can be resolved.
    pub fn delete(
        &self,
        sources: &[ResourceRef],
        generation: near_core::ListingGeneration,
        recursive: bool,
    ) -> Result<OperationPlan, PlanError> {
        self.require_eligible(sources, MutationKind::Delete)?;
        let items = sources
            .iter()
            .map(|source| PlannedItem {
                source: Some(source.clone()),
                target: source.location.clone(),
                conflict_expected: false,
                recursive,
                parameters: BTreeMap::default(),
            })
            .collect();
        self.planner.plan(PlanRequest {
            kind: OperationKind::Delete,
            items,
            destination: None,
            policies: PlanPolicies {
                recovery: RecoveryPolicy::None,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Destructive,
            context_generation: generation,
            high_impact: true,
        })
    }

    /// Plans overwrite-before-delete for regular files.
    ///
    /// # Errors
    ///
    /// Returns an error for empty sources, non-files, symbolic links, or pass counts outside 1..=7.
    pub fn wipe(
        &self,
        sources: &[ResourceRef],
        generation: near_core::ListingGeneration,
        passes: u8,
    ) -> Result<OperationPlan, PlanError> {
        self.require_eligible(sources, MutationKind::Wipe)?;
        if !(1..=7).contains(&passes) {
            return Err(PlanError::Invalid(
                "wipe passes must be between 1 and 7".to_owned(),
            ));
        }
        let mut items = Vec::with_capacity(sources.len());
        for source in sources {
            let path = LocalFileProvider::path(&source.location).map_err(|error| {
                PlanError::Invalid(format!("cannot resolve wipe source: {error}"))
            })?;
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                PlanError::Invalid(format!("cannot inspect {}: {error}", path.display()))
            })?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(PlanError::Invalid(format!(
                    "wipe supports regular files only: {}",
                    path.display()
                )));
            }
            if metadata.permissions().readonly() {
                return Err(PlanError::Invalid(format!(
                    "wipe requires a writable file: {}",
                    path.display()
                )));
            }
            items.push(PlannedItem {
                source: Some(source.clone()),
                target: source.location.clone(),
                conflict_expected: false,
                recursive: false,
                parameters: BTreeMap::from([("passes".to_owned(), passes.to_string())]),
            });
        }
        self.planner.plan(PlanRequest {
            kind: OperationKind::Wipe,
            items,
            destination: None,
            policies: PlanPolicies {
                recovery: RecoveryPolicy::None,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Destructive,
            context_generation: generation,
            high_impact: true,
        })
    }

    /// Plans a rename to an exact target location.
    ///
    /// # Errors
    ///
    /// Returns an error when the source or target cannot form a valid plan.
    pub fn rename(
        &self,
        source: ResourceRef,
        target: Location,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        self.single_source_plan(
            OperationKind::Rename,
            source,
            target,
            generation,
            BTreeMap::default(),
        )
    }

    /// Plans multiple exact-name renames as one immutable operation.
    ///
    /// # Errors
    ///
    /// Returns an error when a source has no parent or a target name is invalid.
    pub fn rename_many(
        &self,
        items: &[(ResourceRef, String)],
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        let planned = items
            .iter()
            .map(|(source, name)| {
                let source_path = LocalFileProvider::path(&source.location)
                    .map_err(|error| PlanError::Invalid(error.to_string()))?;
                let parent = source_path.parent().ok_or_else(|| {
                    PlanError::Invalid(format!("{} has no parent", source.location.as_str()))
                })?;
                if name.is_empty() || name.contains(['/', '\\']) {
                    return Err(PlanError::Invalid(format!("invalid rename target: {name}")));
                }
                let target = LocalFileProvider::location(&parent.join(name));
                let conflict_expected = source.location != target
                    && LocalFileProvider::path(&target).is_ok_and(|path| path.exists());
                Ok(PlannedItem {
                    source: Some(source.clone()),
                    target,
                    conflict_expected,
                    recursive: false,
                    parameters: BTreeMap::default(),
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        self.planner.plan(PlanRequest {
            kind: OperationKind::Rename,
            destination: None,
            items: planned,
            policies: PlanPolicies {
                conflict: near_ops::ConflictPolicy::Ask,
                recovery: RecoveryPolicy::Backup,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Confirmable,
            context_generation: generation,
            high_impact: false,
        })
    }

    /// Plans a hard link to an exact target location.
    ///
    /// # Errors
    ///
    /// Returns an error when the source or target cannot form a valid plan.
    pub fn hard_link(
        &self,
        source: ResourceRef,
        target: Location,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        self.single_source_plan(
            OperationKind::HardLink,
            source,
            target,
            generation,
            BTreeMap::default(),
        )
    }

    /// Plans a symbolic link to an exact target location.
    ///
    /// # Errors
    ///
    /// Returns an error when the source or target cannot form a valid plan.
    pub fn symbolic_link(
        &self,
        source: ResourceRef,
        target: Location,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        self.single_source_plan(
            OperationKind::SymbolicLink,
            source,
            target,
            generation,
            BTreeMap::default(),
        )
    }

    /// Plans creation or timestamp update of a file.
    ///
    /// # Errors
    ///
    /// Returns an error when the target cannot form a valid plan.
    pub fn touch(
        &self,
        target: Location,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        let conflict_expected = LocalFileProvider::path(&target).is_ok_and(|path| path.exists());
        self.planner.plan(PlanRequest {
            kind: OperationKind::Touch,
            items: vec![PlannedItem {
                source: None,
                target: target.clone(),
                conflict_expected,
                recursive: false,
                parameters: BTreeMap::default(),
            }],
            destination: Some(target),
            policies: PlanPolicies {
                conflict: near_ops::ConflictPolicy::Replace,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Reversible,
            context_generation: generation,
            high_impact: false,
        })
    }

    /// Plans a Unix mode change as an attribute operation.
    ///
    /// # Errors
    ///
    /// Returns an error when the resource cannot form a valid plan.
    pub fn set_unix_mode(
        &self,
        source: &ResourceRef,
        mode: u32,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        self.single_source_plan(
            OperationKind::SetAttributes,
            source.clone(),
            source.location.clone(),
            generation,
            BTreeMap::from([("unix-mode".to_owned(), format!("{mode:o}"))]),
        )
    }

    /// Plans portable and platform-specific metadata updates over explicit resources.
    ///
    /// Recursive requests expand descendants into individual preview and outcome items.
    ///
    /// # Errors
    ///
    /// Returns an error for empty updates, unsupported fields, or unreadable recursion roots.
    pub fn set_attributes(
        &self,
        sources: &[ResourceRef],
        update: &AttributeUpdate,
        recursive: bool,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, PlanError> {
        if update.is_empty() {
            return Err(PlanError::Invalid("attribute update is empty".to_owned()));
        }
        #[cfg(not(unix))]
        if update.unix_mode.is_some() || update.owner.is_some() || update.group.is_some() {
            return Err(PlanError::Invalid(
                "Unix mode and ownership fields are unsupported on this platform".to_owned(),
            ));
        }
        let mut resources = Vec::new();
        for source in sources {
            if !resources.contains(source) {
                resources.push(source.clone());
            }
            if recursive {
                let root = LocalFileProvider::path(&source.location)
                    .map_err(|error| PlanError::Invalid(error.to_string()))?;
                collect_attribute_descendants(&root, &mut resources)?;
            }
        }
        let parameters = attribute_parameters(update);
        let items = resources
            .into_iter()
            .map(|source| PlannedItem {
                target: source.location.clone(),
                source: Some(source),
                conflict_expected: false,
                recursive: false,
                parameters: parameters.clone(),
            })
            .collect();
        self.planner.plan(PlanRequest {
            kind: OperationKind::SetAttributes,
            items,
            destination: None,
            policies: PlanPolicies {
                recovery: RecoveryPolicy::JournalOnly,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Confirmable,
            context_generation: generation,
            high_impact: false,
        })
    }

    fn single_source_plan(
        &self,
        kind: OperationKind,
        source: ResourceRef,
        target: Location,
        generation: near_core::ListingGeneration,
        parameters: BTreeMap<String, String>,
    ) -> Result<OperationPlan, PlanError> {
        let conflict_expected = source.location != target
            && LocalFileProvider::path(&target).is_ok_and(|path| path.exists());
        self.planner.plan(PlanRequest {
            kind,
            items: vec![PlannedItem {
                source: Some(source),
                target: target.clone(),
                conflict_expected,
                recursive: false,
                parameters,
            }],
            destination: Some(target),
            policies: PlanPolicies {
                conflict: near_ops::ConflictPolicy::Ask,
                recovery: RecoveryPolicy::Backup,
                ..PlanPolicies::default()
            },
            safety: near_core::SafetyClass::Confirmable,
            context_generation: generation,
            high_impact: false,
        })
    }
}

pub struct LocalOperationBackend {
    trash_directory: PathBuf,
    use_platform_trash: bool,
    description_settings: Option<DescriptionSettings>,
}

fn collect_attribute_descendants(
    root: &Path,
    resources: &mut Vec<ResourceRef>,
) -> Result<(), PlanError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| PlanError::Invalid(format!("{}: {error}", root.display())))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(());
    }
    let entries = fs::read_dir(root)
        .map_err(|error| PlanError::Invalid(format!("{}: {error}", root.display())))?;
    for entry in entries {
        let path = entry
            .map_err(|error| PlanError::Invalid(format!("{}: {error}", root.display())))?
            .path();
        let resource = LocalFileProvider::resource_for_path(&path);
        if !resources.contains(&resource) {
            resources.push(resource);
        }
        collect_attribute_descendants(&path, resources)?;
    }
    Ok(())
}

fn attribute_parameters(update: &AttributeUpdate) -> BTreeMap<String, String> {
    let mut parameters = BTreeMap::new();
    if let Some(readonly) = update.readonly {
        parameters.insert("readonly".to_owned(), readonly.to_string());
    }
    if let Some(mode) = update.unix_mode {
        parameters.insert("unix-mode".to_owned(), format!("{mode:o}"));
    }
    if let Some(owner) = update.owner {
        parameters.insert("owner".to_owned(), owner.to_string());
    }
    if let Some(group) = update.group {
        parameters.insert("group".to_owned(), group.to_string());
    }
    if let Some(modified) = update.modified_unix_ms {
        parameters.insert("modified-unix-ms".to_owned(), modified.to_string());
    }
    if let Some(accessed) = update.accessed_unix_ms {
        parameters.insert("accessed-unix-ms".to_owned(), accessed.to_string());
    }
    parameters
}

impl LocalOperationBackend {
    pub fn new(trash_directory: PathBuf) -> Self {
        Self {
            trash_directory,
            use_platform_trash: false,
            description_settings: None,
        }
    }

    fn platform_default(trash_directory: PathBuf) -> Self {
        Self {
            trash_directory,
            use_platform_trash: true,
            description_settings: None,
        }
    }

    pub fn macos_default() -> Self {
        Self::platform_default(platform_trash_directory())
    }

    fn execute_move_or_restore(
        &self,
        plan: &OperationPlan,
        item: &PlannedItem,
        target: &Path,
        cancellation: &near_core::CancellationToken,
    ) -> Result<(), String> {
        let source = source_path(item)?;
        let restored_natively = plan.kind() == OperationKind::Restore
            && self.use_platform_trash
            && restore_from_platform_trash(&source, target)?;
        if !restored_natively {
            if plan.policies().cross_device == CrossDeviceBehavior::AtomicRename {
                fs::rename(&source, target).map_err(|error| error.to_string())?;
            } else {
                copy_item(&source, target, cancellation)?;
                remove_item(&source, item.recursive)?;
            }
        }
        if plan.kind() == OperationKind::Restore {
            remove_trash_metadata(&source)?;
        }
        Ok(())
    }
}

impl LocalOperationPlanner {
    pub fn into_backend(self) -> LocalOperationBackend {
        LocalOperationBackend {
            trash_directory: self.trash_directory,
            use_platform_trash: self.native_platform_trash,
            description_settings: None,
        }
    }
}

impl OperationBackend for LocalOperationBackend {
    fn target_exists(&self, target: &Location) -> bool {
        LocalFileProvider::path(target).is_ok_and(|path| path.exists())
    }

    fn execute(
        &mut self,
        plan: &OperationPlan,
        item: &PlannedItem,
        action: Option<ConflictAction>,
        cancellation: &near_core::CancellationToken,
    ) -> Result<ExecutionEffect, String> {
        if cancellation.is_cancelled() {
            return Err("cancelled".to_owned());
        }
        let raw_target =
            LocalFileProvider::path(&item.target).map_err(|error| error.to_string())?;
        let target = if matches!(
            plan.kind(),
            OperationKind::Delete
                | OperationKind::Wipe
                | OperationKind::SetAttributes
                | OperationKind::Touch
        ) || (self.use_platform_trash
            && cfg!(any(target_os = "macos", target_os = "windows"))
            && plan.kind() == OperationKind::Trash)
        {
            raw_target
        } else {
            prepare_target(&raw_target, action, plan.policies().recovery)?
        };
        let source = item
            .source
            .as_ref()
            .and_then(|source| LocalFileProvider::path(&source.location).ok());
        let actual_target = match plan.kind() {
            OperationKind::Copy => {
                copy_item(source_path(item)?, &target, cancellation)?;
                None
            }
            OperationKind::Move | OperationKind::Restore => {
                self.execute_move_or_restore(plan, item, &target, cancellation)?;
                None
            }
            OperationKind::Rename => {
                fs::rename(source_path(item)?, &target).map_err(|error| error.to_string())?;
                None
            }
            OperationKind::HardLink => {
                fs::hard_link(source_path(item)?, &target).map_err(|error| error.to_string())?;
                None
            }
            OperationKind::SymbolicLink => {
                create_symlink(&source_path(item)?, &target)?;
                None
            }
            OperationKind::Trash => {
                let source = source_path(item)?;
                move_to_platform_trash(
                    &source,
                    &target,
                    item.recursive,
                    &self.trash_directory,
                    self.use_platform_trash,
                    cancellation,
                )?
            }
            OperationKind::Delete => {
                remove_item(&source_path(item)?, item.recursive)?;
                None
            }
            OperationKind::Wipe => {
                wipe_file(&source_path(item)?, item, cancellation)?;
                None
            }
            OperationKind::CreateDirectory => {
                fs::create_dir(&target).map_err(|error| error.to_string())?;
                None
            }
            OperationKind::Touch => OpenOptions::new()
                .create(true)
                .append(true)
                .open(&target)
                .map(|_| None)
                .map_err(|error| error.to_string())?,
            OperationKind::SetAttributes => {
                let source = source_path(item)?;
                apply_attribute_parameters(&source, &item.parameters)?;
                None
            }
            _ => return Err("operation is not supported by local filesystem backend".to_owned()),
        };
        let effective_target = actual_target.as_ref().unwrap_or(&target);
        if let (Some(settings), Some(source)) = (&self.description_settings, source) {
            sync_operation_description(plan.kind(), &source, effective_target, settings)
                .map_err(|error| error.to_string())?;
        }
        Ok(ExecutionEffect {
            target: actual_target.map(|path| LocalFileProvider::location(&path)),
        })
    }
}

fn sync_operation_description(
    kind: OperationKind,
    source: &Path,
    target: &Path,
    settings: &DescriptionSettings,
) -> Result<(), ProviderError> {
    if settings.update_policy == DescriptionUpdatePolicy::Disabled {
        return Ok(());
    }
    let source_directory = source
        .parent()
        .ok_or_else(|| ProviderError::Failed("source has no parent folder".to_owned()))?;
    let source_name = source
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| ProviderError::Failed("description filenames must be Unicode".to_owned()))?;
    let description = load_description_catalog(source_directory, settings)?
        .get(source_name)
        .cloned();
    match kind {
        OperationKind::Copy => {
            if let Some(description) = description {
                update_path_description(target, Some(description), settings)?;
            }
        }
        OperationKind::Move | OperationKind::Rename => {
            if let Some(description) = description {
                update_path_description(target, Some(description), settings)?;
            }
            update_path_description(source, None, settings)?;
        }
        OperationKind::Trash | OperationKind::Delete | OperationKind::Wipe => {
            update_path_description(source, None, settings)?;
        }
        _ => {}
    }
    Ok(())
}

fn source_path(item: &PlannedItem) -> Result<PathBuf, String> {
    let source = item
        .source
        .as_ref()
        .ok_or_else(|| "operation item has no source".to_owned())?;
    LocalFileProvider::path(&source.location).map_err(|error| error.to_string())
}

fn wipe_file(
    path: &Path,
    item: &PlannedItem,
    cancellation: &near_core::CancellationToken,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("wipe supports regular files only".to_owned());
    }
    let passes = item
        .parameters
        .get("passes")
        .and_then(|passes| passes.parse::<u8>().ok())
        .filter(|passes| (1..=7).contains(passes))
        .ok_or_else(|| "wipe plan has an invalid pass count".to_owned())?;
    let length = metadata.len();
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    for pass in 0..passes {
        if cancellation.is_cancelled() {
            return Err("cancelled during wipe; file may be partially overwritten".to_owned());
        }
        buffer.fill(if pass % 2 == 0 { 0x00 } else { 0xff });
        file.seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        let mut remaining = length;
        while remaining > 0 {
            if cancellation.is_cancelled() {
                return Err("cancelled during wipe; file may be partially overwritten".to_owned());
            }
            let count = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|error| error.to_string())?;
            std::io::Write::write_all(&mut file, &buffer[..count])
                .map_err(|error| error.to_string())?;
            remaining = remaining.saturating_sub(count as u64);
        }
        file.sync_all().map_err(|error| error.to_string())?;
    }
    drop(file);
    fs::remove_file(path).map_err(|error| error.to_string())
}

fn is_cross_device(item: &PlannedItem, destination: &Path) -> bool {
    let Ok(source) = source_path(item) else {
        return false;
    };
    platform_devices_differ(&source, destination)
}

#[cfg(unix)]
fn is_protected_trash_root(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return true;
    };
    if parent == path {
        return true;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(parent_metadata) = fs::metadata(parent) else {
        return false;
    };
    metadata.dev() != parent_metadata.dev()
}

#[cfg(windows)]
fn is_protected_trash_root(path: &Path) -> bool {
    path.parent().is_none()
}

fn prepare_target(
    target: &Path,
    action: Option<ConflictAction>,
    recovery: RecoveryPolicy,
) -> Result<PathBuf, String> {
    if !target.exists() {
        return Ok(target.to_owned());
    }
    match action {
        Some(ConflictAction::Replace) => {
            if recovery == RecoveryPolicy::Backup {
                let backup = unique_path(target, ".near-backup");
                fs::rename(target, backup).map_err(|error| error.to_string())?;
            } else {
                remove_item(target, target.is_dir())?;
            }
            Ok(target.to_owned())
        }
        Some(ConflictAction::Rename) => Ok(unique_path(target, " copy")),
        Some(ConflictAction::Skip | ConflictAction::Cancel) => Ok(target.to_owned()),
        None => Err(format!(
            "target already exists: {}",
            escape_name(target.as_os_str())
        )),
    }
}

fn unique_path(path: &Path, suffix: &str) -> PathBuf {
    for index in 1..=10_000 {
        let candidate = path.with_file_name(format!(
            "{}{} {index}",
            escape_name(path.file_name().unwrap_or(path.as_os_str())),
            suffix
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.with_file_name(format!("near-conflict-{}", std::process::id()))
}

fn copy_item(
    source: impl AsRef<Path>,
    target: &Path,
    cancellation: &near_core::CancellationToken,
) -> Result<(), String> {
    let source = source.as_ref();
    if cancellation.is_cancelled() {
        return Err("cancelled".to_owned());
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        let link = fs::read_link(source).map_err(|error| error.to_string())?;
        create_symlink(&link, target)
    } else if metadata.is_dir() {
        fs::create_dir(target).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            copy_item(entry.path(), &target.join(entry.file_name()), cancellation)?;
        }
        fs::set_permissions(target, metadata.permissions()).map_err(|error| error.to_string())
    } else {
        fs::copy(source, target)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn remove_item(path: &Path, recursive: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        if recursive {
            fs::remove_dir_all(path).map_err(|error| error.to_string())
        } else {
            fs::remove_dir(path).map_err(|error| error.to_string())
        }
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> Result<(), String> {
    let directory = fs::metadata(source).is_ok_and(|metadata| metadata.is_dir());
    if directory {
        std::os::windows::fs::symlink_dir(source, target).map_err(|error| error.to_string())
    } else {
        std::os::windows::fs::symlink_file(source, target).map_err(|error| error.to_string())
    }
}

#[cfg(unix)]
fn set_unix_permissions(path: &Path, mode: u32) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn set_unix_permissions(_path: &Path, _mode: u32) -> Result<(), String> {
    Err("Unix mode changes are unavailable on Windows".to_owned())
}

fn apply_attribute_parameters(
    path: &Path,
    parameters: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Some(mode) = parameters.get("unix-mode") {
        match u32::from_str_radix(mode, 8)
            .map_err(|error| error.to_string())
            .and_then(|mode| set_unix_permissions(path, mode))
        {
            Ok(()) => {}
            Err(error) => errors.push(format!("unix mode: {error}")),
        }
    }
    if parameters.contains_key("owner") || parameters.contains_key("group") {
        let owner = parameters
            .get("owner")
            .map(|value| value.parse::<u32>().map_err(|error| error.to_string()))
            .transpose();
        let group = parameters
            .get("group")
            .map(|value| value.parse::<u32>().map_err(|error| error.to_string()))
            .transpose();
        match (owner, group) {
            (Ok(owner), Ok(group)) => {
                if let Err(error) = set_ownership(path, owner, group) {
                    errors.push(format!("ownership: {error}"));
                }
            }
            (Err(error), _) | (_, Err(error)) => {
                errors.push(format!("ownership: {error}"));
            }
        }
    }
    if parameters.contains_key("modified-unix-ms") || parameters.contains_key("accessed-unix-ms") {
        let modified = parameters
            .get("modified-unix-ms")
            .map(|value| parse_system_time(value))
            .transpose();
        let accessed = parameters
            .get("accessed-unix-ms")
            .map(|value| parse_system_time(value))
            .transpose();
        match (modified, accessed) {
            (Ok(modified), Ok(accessed)) => {
                if let Err(error) = set_file_times(path, modified, accessed) {
                    errors.push(format!("timestamps: {error}"));
                }
            }
            (Err(error), _) | (_, Err(error)) => {
                errors.push(format!("timestamps: {error}"));
            }
        }
    }
    if let Some(readonly) = parameters.get("readonly") {
        match readonly
            .parse::<bool>()
            .map_err(|error| error.to_string())
            .and_then(|readonly| set_portable_readonly(path, readonly))
        {
            Ok(()) => {}
            Err(error) => errors.push(format!("read-only: {error}")),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn parse_system_time(value: &str) -> Result<SystemTime, String> {
    let milliseconds = value.parse::<i64>().map_err(|error| error.to_string())?;
    if milliseconds >= 0 {
        Ok(UNIX_EPOCH + Duration::from_millis(milliseconds.unsigned_abs()))
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::from_millis(milliseconds.unsigned_abs()))
            .ok_or_else(|| "timestamp is outside the supported range".to_owned())
    }
}

fn set_file_times(
    path: &Path,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
) -> Result<(), String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut times = fs::FileTimes::new();
    if let Some(modified) = modified {
        times = times.set_modified(modified);
    }
    if let Some(accessed) = accessed {
        times = times.set_accessed(accessed);
    }
    file.set_times(times).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn set_ownership(path: &Path, owner: Option<u32>, group: Option<u32>) -> Result<(), String> {
    std::os::unix::fs::chown(path, owner, group).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_ownership(_path: &Path, _owner: Option<u32>, _group: Option<u32>) -> Result<(), String> {
    Err("ownership changes are not supported on this platform".to_owned())
}

#[cfg(unix)]
fn set_portable_readonly(path: &Path, readonly: bool) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    let mode = permissions.mode();
    permissions.set_mode(if readonly {
        mode & !0o222
    } else {
        mode | 0o200
    });
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_portable_readonly(path: &Path, readonly: bool) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn platform_devices_differ(source: &Path, destination: &Path) -> bool {
    let Ok(source_metadata) = fs::symlink_metadata(source) else {
        return false;
    };
    let Ok(destination_metadata) = fs::metadata(destination) else {
        return false;
    };
    source_metadata.dev() != destination_metadata.dev()
}

#[cfg(windows)]
fn platform_devices_differ(source: &Path, destination: &Path) -> bool {
    source.components().next() != destination.components().next()
}

#[cfg(target_os = "macos")]
fn platform_trash_directory() -> PathBuf {
    std::env::var_os("HOME")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(".Trash")
}

#[cfg(target_os = "macos")]
fn move_to_platform_trash(
    source: &Path,
    target: &Path,
    recursive: bool,
    trash_directory: &Path,
    use_platform_trash: bool,
    cancellation: &near_core::CancellationToken,
) -> Result<Option<PathBuf>, String> {
    if use_platform_trash {
        if cancellation.is_cancelled() {
            return Err("cancelled".to_owned());
        }
        return run_macos_trash_helper(source).map(Some);
    }
    fs::create_dir_all(trash_directory).map_err(|error| error.to_string())?;
    fs::rename(source, target).or_else(|_| {
        copy_item(source, target, cancellation)?;
        remove_item(source, recursive)
    })?;
    Ok(Some(target.to_path_buf()))
}

#[cfg(target_os = "macos")]
fn run_macos_trash_helper(source: &Path) -> Result<PathBuf, String> {
    let executable = std::env::var_os("NEAR_NATIVE_TRASH_HELPER")
        .map(PathBuf::from)
        .map_or_else(std::env::current_exe, Ok)
        .map_err(|error| error.to_string())?;
    run_macos_trash_helper_with(&executable, source, Duration::from_secs(30))
}

#[cfg(target_os = "macos")]
fn run_macos_trash_helper_with(
    executable: &Path,
    source: &Path,
    timeout: Duration,
) -> Result<PathBuf, String> {
    run_macos_file_helper_with(
        executable,
        "--near-native-trash-helper",
        source,
        None,
        timeout,
    )
}

#[cfg(target_os = "macos")]
fn run_macos_restore_helper(source: &Path, target: &Path) -> Result<PathBuf, String> {
    let executable = std::env::var_os("NEAR_NATIVE_TRASH_HELPER")
        .map(PathBuf::from)
        .map_or_else(std::env::current_exe, Ok)
        .map_err(|error| error.to_string())?;
    run_macos_restore_helper_with(&executable, source, target, Duration::from_secs(30))
}

#[cfg(target_os = "macos")]
fn run_macos_restore_helper_with(
    executable: &Path,
    source: &Path,
    target: &Path,
    timeout: Duration,
) -> Result<PathBuf, String> {
    run_macos_file_helper_with(
        executable,
        "--near-native-restore-helper",
        source,
        Some(target),
        timeout,
    )
}

#[cfg(target_os = "macos")]
fn run_macos_file_helper_with(
    executable: &Path,
    mode: &str,
    source: &Path,
    target: Option<&Path>,
    timeout: Duration,
) -> Result<PathBuf, String> {
    let mut command = Command::new(executable);
    command.arg(mode).arg(source);
    if let Some(target) = target {
        command.arg(target);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            let output = child
                .wait_with_output()
                .map_err(|error| error.to_string())?;
            return if output.status.success() {
                if output.stdout.is_empty() {
                    Err("native Trash helper returned no resulting path".to_owned())
                } else {
                    Ok(PathBuf::from(OsString::from_vec(output.stdout)))
                }
            } else {
                let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                Err(if detail.is_empty() {
                    format!("native Trash helper exited with {}", output.status)
                } else {
                    detail
                })
            };
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "native macOS Trash helper timed out after {timeout:?}"
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(target_os = "macos")]
fn restore_from_platform_trash(source: &Path, target: &Path) -> Result<bool, String> {
    let restored = run_macos_restore_helper(source, target)?;
    if restored != target {
        return Err(format!(
            "native Restore helper returned unexpected path {}",
            restored.display()
        ));
    }
    Ok(true)
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps)]
fn restore_from_platform_trash(_source: &Path, _target: &Path) -> Result<bool, String> {
    Ok(false)
}

#[cfg(target_os = "macos")]
pub fn execute_native_trash_helper(source: &Path) -> Result<PathBuf, String> {
    use objc2_foundation::{NSFileManager, NSURL};

    let source_url = NSURL::from_file_path(source)
        .ok_or_else(|| "native Trash source path cannot be represented as a file URL".to_owned())?;
    let mut resulting_url = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&source_url, Some(&mut resulting_url))
        .map_err(|error| error.to_string())?;
    resulting_url
        .and_then(|url| url.to_file_path())
        .ok_or_else(|| "native Trash API returned no filesystem result path".to_owned())
}

#[cfg(target_os = "macos")]
pub fn execute_native_restore_helper(source: &Path, target: &Path) -> Result<PathBuf, String> {
    use objc2_foundation::{NSFileManager, NSURL};

    let source_url = NSURL::from_file_path(source).ok_or_else(|| {
        "native Restore source path cannot be represented as a file URL".to_owned()
    })?;
    let target_url = NSURL::from_file_path(target).ok_or_else(|| {
        "native Restore target path cannot be represented as a file URL".to_owned()
    })?;
    NSFileManager::defaultManager()
        .moveItemAtURL_toURL_error(&source_url, &target_url)
        .map_err(|error| error.to_string())?;
    Ok(target.to_path_buf())
}

#[cfg(target_os = "linux")]
fn move_to_platform_trash(
    source: &Path,
    target: &Path,
    recursive: bool,
    trash_directory: &Path,
    _use_platform_trash: bool,
    cancellation: &near_core::CancellationToken,
) -> Result<Option<PathBuf>, String> {
    fs::create_dir_all(trash_directory).map_err(|error| error.to_string())?;
    fs::rename(source, target).or_else(|_| {
        copy_item(source, target, cancellation)?;
        remove_item(source, recursive)
    })?;
    write_trash_metadata(source, target)?;
    Ok(Some(target.to_path_buf()))
}

#[cfg(target_os = "linux")]
fn write_trash_metadata(source: &Path, target: &Path) -> Result<(), String> {
    let Some(files_directory) = target.parent() else {
        return Err("Trash target has no parent directory".to_owned());
    };
    let Some(trash_root) = files_directory.parent() else {
        return Err("Trash files directory has no root".to_owned());
    };
    let info_directory = trash_root.join("info");
    fs::create_dir_all(&info_directory).map_err(|error| error.to_string())?;
    let name = target
        .file_name()
        .ok_or_else(|| "Trash target has no file name".to_owned())?;
    let mut info_name = name.to_os_string();
    info_name.push(".trashinfo");
    let deletion_date = linux_deletion_date();
    let path = percent_encode(&native_bytes(source.as_os_str()));
    fs::write(
        info_directory.join(info_name),
        format!("[Trash Info]\nPath={path}\nDeletionDate={deletion_date}\n"),
    )
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn remove_trash_metadata(source: &Path) -> Result<(), String> {
    let Some(files_directory) = source.parent() else {
        return Ok(());
    };
    let Some(trash_root) = files_directory.parent() else {
        return Ok(());
    };
    let Some(name) = source.file_name() else {
        return Ok(());
    };
    let mut info_name = name.to_os_string();
    info_name.push(".trashinfo");
    let metadata = trash_root.join("info").join(info_name);
    match fs::remove_file(metadata) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::unnecessary_wraps)]
fn remove_trash_metadata(_source: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_deletion_date() -> String {
    let output = Command::new("date").args(["+%Y-%m-%dT%H:%M:%S"]).output();
    output.map_or_else(
        |_| "1970-01-01T00:00:00".to_owned(),
        |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    )
}

#[cfg(windows)]
fn move_to_platform_trash(
    source: &Path,
    target: &Path,
    recursive: bool,
    trash_directory: &Path,
    use_platform_trash: bool,
    cancellation: &near_core::CancellationToken,
) -> Result<Option<PathBuf>, String> {
    if !use_platform_trash {
        fs::create_dir_all(trash_directory).map_err(|error| error.to_string())?;
        fs::rename(source, target).or_else(|_| {
            copy_item(source, target, cancellation)?;
            remove_item(source, recursive)
        })?;
        return Ok(Some(target.to_path_buf()));
    }
    let script = windows_recycle_script(recursive);
    let output = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(script)
        .arg(source)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(None)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

#[cfg(windows)]
fn windows_recycle_script(recursive: bool) -> String {
    let method = if recursive {
        "DeleteDirectory"
    } else {
        "DeleteFile"
    };
    format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::{method}($args[0], [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs, [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin)"
    )
}

#[cfg(target_os = "linux")]
fn platform_trash_directory() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            std::env::var_os("HOME")
                .map_or_else(std::env::temp_dir, PathBuf::from)
                .join(".local/share/Trash/files")
        },
        |directory| PathBuf::from(directory).join("Trash/files"),
    )
}

#[cfg(windows)]
fn platform_trash_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Near/Trash")
}

pub struct LocalOperationService {
    planner: LocalOperationPlanner,
    engine: OperationEngine<LocalOperationBackend>,
    elevation: Option<Arc<dyn ElevationBroker>>,
}

pub trait ElevationBroker: Send + Sync {
    fn execute(
        &self,
        plan: &OperationPlan,
        authorization: ExecutionAuthorization,
        conflict: near_ops::ConflictDecision,
    ) -> Result<ExecutionSummary, String>;
}

pub struct PlatformElevationBroker {
    journal: String,
}

impl PlatformElevationBroker {
    pub fn new(journal: impl Into<String>) -> Self {
        Self {
            journal: journal.into(),
        }
    }
}

impl LocalOperationService {
    pub fn new(trash_directory: PathBuf, journal: OperationJournal) -> Self {
        Self {
            planner: LocalOperationPlanner::new(trash_directory.clone()),
            engine: OperationEngine::new(LocalOperationBackend::new(trash_directory), journal),
            elevation: None,
        }
    }

    pub fn macos_default(journal: OperationJournal) -> Self {
        Self::platform_default(journal)
    }

    pub fn platform_default(journal: OperationJournal) -> Self {
        let planner = LocalOperationPlanner::platform_default();
        let trash_directory = planner.trash_directory.clone();
        Self {
            planner,
            engine: OperationEngine::new(
                LocalOperationBackend::platform_default(trash_directory),
                journal,
            ),
            elevation: None,
        }
    }

    #[must_use]
    pub fn with_elevation_broker(mut self, broker: impl ElevationBroker + 'static) -> Self {
        self.elevation = Some(Arc::new(broker));
        self
    }

    #[must_use]
    pub fn with_description_settings(mut self, settings: DescriptionSettings) -> Self {
        self.engine.backend_mut().description_settings = Some(settings);
        self
    }
}

impl OperationService for LocalOperationService {
    fn plan(
        &mut self,
        intent: OperationIntent,
        generation: near_core::ListingGeneration,
    ) -> Result<OperationPlan, String> {
        let plan = match intent {
            OperationIntent::CopyTo {
                sources,
                destination,
            } => self.planner.copy(
                &sources,
                &destination,
                generation,
                near_ops::ConflictPolicy::Ask,
            ),
            OperationIntent::MoveTo {
                sources,
                destination,
            } => self.planner.move_resources(
                &sources,
                &destination,
                generation,
                near_ops::ConflictPolicy::Ask,
            ),
            OperationIntent::Trash { sources } => self.planner.trash(&sources, generation),
            OperationIntent::Restore { items } => self.planner.restore(&items, generation),
            OperationIntent::Delete { sources, recursive } => {
                self.planner.delete(&sources, generation, recursive)
            }
            OperationIntent::Wipe { sources, passes } => {
                self.planner.wipe(&sources, generation, passes)
            }
            OperationIntent::CreateDirectory { parent, name } => {
                let parent = LocalFileProvider::path(&parent).map_err(|error| error.to_string())?;
                self.planner
                    .create_directory(LocalFileProvider::location(&parent.join(name)), generation)
            }
            OperationIntent::Rename { items } => self.planner.rename_many(&items, generation),
            OperationIntent::CreateLink { source, name, kind } => {
                if name.is_empty() || name.contains(['/', '\\']) {
                    return Err(format!("invalid link target name: {name}"));
                }
                let source_path =
                    LocalFileProvider::path(&source.location).map_err(|error| error.to_string())?;
                let metadata = fs::symlink_metadata(&source_path).map_err(|error| {
                    format!(
                        "cannot inspect link source {}: {error}",
                        source_path.display()
                    )
                })?;
                let parent = source_path.parent().ok_or_else(|| {
                    format!("link source has no parent: {}", source_path.display())
                })?;
                let target = LocalFileProvider::location(&parent.join(name));
                match kind {
                    LinkKind::Hard if !metadata.is_file() => {
                        return Err("hard links require a regular file source".to_owned());
                    }
                    LinkKind::Junction if !metadata.is_dir() => {
                        return Err("junction links require a directory source".to_owned());
                    }
                    LinkKind::Hard => self.planner.hard_link(source, target, generation),
                    LinkKind::Symbolic | LinkKind::Junction => {
                        self.planner.symbolic_link(source, target, generation)
                    }
                }
            }
            OperationIntent::SetAttributes {
                sources,
                update,
                recursive,
            } => self
                .planner
                .set_attributes(&sources, &update, recursive, generation),
        }
        .map_err(|error| error.to_string())?;
        self.engine
            .record(plan.clone())
            .map_err(|error| error.to_string())?;
        Ok(plan)
    }

    fn execute(
        &mut self,
        plan: &near_core::OperationId,
        authorization: near_ops::ExecutionAuthorization,
        cancellation: &near_core::CancellationToken,
        conflict: near_ops::ConflictDecision,
    ) -> Result<near_ops::ExecutionSummary, String> {
        struct FixedResolver(near_ops::ConflictDecision);
        impl near_ops::ConflictResolver for FixedResolver {
            fn decide(
                &mut self,
                _plan: &OperationPlan,
                _item: &PlannedItem,
            ) -> near_ops::ConflictDecision {
                self.0
            }
        }
        self.engine
            .execute(
                plan,
                authorization,
                cancellation,
                &mut FixedResolver(conflict),
            )
            .map_err(|error| error.to_string())
    }

    fn execute_elevated(
        &mut self,
        plan: &near_core::OperationId,
        authorization: ExecutionAuthorization,
        conflict: near_ops::ConflictDecision,
    ) -> Result<ExecutionSummary, String> {
        let plan = self
            .engine
            .plan(plan)
            .cloned()
            .ok_or_else(|| format!("unknown operation plan: {plan}"))?;
        self.elevation
            .as_ref()
            .ok_or("platform elevation broker is not configured")?
            .execute(&plan, authorization, conflict)
    }
}

impl ElevationBroker for PlatformElevationBroker {
    fn execute(
        &self,
        plan: &OperationPlan,
        authorization: ExecutionAuthorization,
        conflict: near_ops::ConflictDecision,
    ) -> Result<ExecutionSummary, String> {
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        );
        let request_path = std::env::temp_dir().join(format!("near-elevated-{nonce}.toml"));
        let response_path =
            std::env::temp_dir().join(format!("near-elevated-{nonce}.response.toml"));
        let request = ElevatedOperationRequest {
            plan: plan.clone(),
            authorization,
            conflict,
        };
        let bytes = request.to_toml()?.into_bytes();
        write_private_file(&request_path, &bytes)?;
        let digest = hex_digest(&bytes);
        let audit_path = std::env::temp_dir().join(format!("near-elevated-{nonce}.audit.log"));
        let launch = launch_elevated_helper(&request_path, &digest);
        let audit = fs::read(&audit_path).unwrap_or_default();
        let audit_result = append_audit_file(Path::new(&self.journal), &audit);
        let result = launch.and_then(|()| {
            audit_result?;
            let response = fs::read_to_string(&response_path)
                .map_err(|error| format!("cannot read elevated operation response: {error}"))?;
            ExecutionSummary::from_toml(&response)
        });
        let _ = fs::remove_file(request_path);
        let _ = fs::remove_file(response_path);
        let _ = fs::remove_file(audit_path);
        result
    }
}

pub fn execute_elevated_request(request_path: &str, expected_digest: &str) -> Result<(), String> {
    validate_elevated_request_path(Path::new(request_path))?;
    let bytes = fs::read(request_path)
        .map_err(|error| format!("cannot read elevated operation request: {error}"))?;
    if hex_digest(&bytes) != expected_digest {
        return Err("elevated operation request digest mismatch".to_owned());
    }
    let request = ElevatedOperationRequest::from_toml(
        std::str::from_utf8(&bytes).map_err(|error| error.to_string())?,
    )?;
    let response_path = sibling_elevation_path(Path::new(request_path), "response.toml")?;
    let audit_path = sibling_elevation_path(Path::new(request_path), "audit.log")?;
    let planner = LocalOperationPlanner::platform_default();
    let trash_directory = planner.trash_directory.clone();
    let mut engine = OperationEngine::new(
        LocalOperationBackend::platform_default(trash_directory),
        OperationJournal::append_file(audit_path),
    );
    let id = engine
        .record_elevated(request.plan)
        .map_err(|error| error.to_string())?;
    let cancellation = near_core::CancellationToken::default();
    let mut resolver = FixedConflictResolver(request.conflict);
    let summary = engine
        .execute(&id, request.authorization, &cancellation, &mut resolver)
        .map_err(|error| error.to_string())?;
    write_private_file(&response_path, summary.to_toml()?.as_bytes())
}

struct FixedConflictResolver(near_ops::ConflictDecision);

impl near_ops::ConflictResolver for FixedConflictResolver {
    fn decide(&mut self, _plan: &OperationPlan, _item: &PlannedItem) -> near_ops::ConflictDecision {
        self.0
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| format!("cannot create private operation file: {error}"))?;
    std::io::Write::write_all(&mut file, bytes)
        .map_err(|error| format!("cannot write private operation file: {error}"))
}

fn append_audit_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot append elevated operation audit: {error}"))?;
    std::io::Write::write_all(&mut file, bytes)
        .map_err(|error| format!("cannot append elevated operation audit: {error}"))
}

fn validate_elevated_request_path(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("elevated request has no parent directory")?;
    let expected_parent = std::env::temp_dir()
        .canonicalize()
        .map_err(|error| format!("cannot resolve temporary directory: {error}"))?;
    let actual_parent = parent
        .canonicalize()
        .map_err(|error| format!("cannot resolve elevated request directory: {error}"))?;
    if actual_parent != expected_parent {
        return Err("elevated request is outside the private temporary directory".to_owned());
    }
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("elevated request has an invalid file name")?;
    if !name.starts_with("near-elevated-") || !name.ends_with(".toml") {
        return Err("elevated request has an invalid file name".to_owned());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect elevated request: {error}"))?;
    if !metadata.file_type().is_file() {
        return Err("elevated request is not a regular file".to_owned());
    }
    #[cfg(unix)]
    if metadata.mode() & 0o777 != 0o600 {
        return Err("elevated request permissions must be 0600".to_owned());
    }
    Ok(())
}

fn sibling_elevation_path(request: &Path, suffix: &str) -> Result<PathBuf, String> {
    let name = request
        .file_name()
        .and_then(OsStr::to_str)
        .and_then(|name| name.strip_suffix(".toml"))
        .ok_or("elevated request has an invalid file name")?;
    Ok(request.with_file_name(format!("{name}.{suffix}")))
}

#[cfg(target_os = "macos")]
fn launch_elevated_helper(request: &Path, digest: &str) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let command = format!(
        "{} --elevated-operation {} {}",
        shell_quote(&executable.to_string_lossy()),
        shell_quote(&request.to_string_lossy()),
        shell_quote(digest)
    );
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        command.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let status = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .status()
        .map_err(|error| format!("cannot launch macOS authorization prompt: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("macOS authorization helper exited with status {status}"))
}

#[cfg(target_os = "linux")]
fn launch_elevated_helper(request: &Path, digest: &str) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let status = Command::new("pkexec")
        .arg(executable)
        .arg("--elevated-operation")
        .arg(request)
        .arg(digest)
        .status()
        .map_err(|error| format!("cannot launch polkit authorization prompt: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("polkit helper exited with status {status}"))
}

#[cfg(windows)]
fn launch_elevated_helper(request: &Path, digest: &str) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let arguments = format!(
        "--elevated-operation {} {}",
        powershell_quote(&request.to_string_lossy()),
        powershell_quote(digest)
    );
    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -Wait",
                powershell_quote(&executable.to_string_lossy()),
                powershell_quote(&arguments)
            ),
        ])
        .status()
        .map_err(|error| format!("cannot launch Windows elevation prompt: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("Windows elevation helper exited with status {status}"))
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        future::Future,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::atomic::{AtomicU64, Ordering},
        task::{Context, Poll, Waker},
    };

    use near_core::{CancellationToken, ListRequest, ListingGeneration, OpenRequest};
    use near_ops::{
        ConflictDecision, ConflictPolicy, ConflictResolver, DecisionScope, ExecutionAuthorization,
        OperationEngine, OperationJournal,
    };

    use super::*;

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct RecordingElevationBroker(Arc<std::sync::Mutex<Option<OperationPlan>>>);

    impl ElevationBroker for RecordingElevationBroker {
        fn execute(
            &self,
            plan: &OperationPlan,
            _authorization: ExecutionAuthorization,
            _conflict: near_ops::ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            *self.0.lock().unwrap() = Some(plan.clone());
            Ok(ExecutionSummary {
                plan: plan.id().clone(),
                kind: plan.kind(),
                items: plan
                    .items()
                    .iter()
                    .cloned()
                    .map(|item| near_ops::ItemOutcome {
                        item,
                        status: near_ops::ItemStatus::Completed,
                    })
                    .collect(),
                cancelled: false,
            })
        }
    }

    #[test]
    fn elevation_broker_receives_the_exact_recorded_plan() {
        let fixture = Fixture::new();
        let recorded = Arc::new(std::sync::Mutex::new(None));
        let mut service =
            LocalOperationService::new(fixture.0.join("trash"), OperationJournal::memory())
                .with_elevation_broker(RecordingElevationBroker(Arc::clone(&recorded)));
        let plan = service
            .plan(
                OperationIntent::CreateDirectory {
                    parent: LocalFileProvider::location(&fixture.0),
                    name: "protected".to_owned(),
                },
                ListingGeneration(9),
            )
            .unwrap();
        let summary = service
            .execute_elevated(
                plan.id(),
                ExecutionAuthorization {
                    context_generation: ListingGeneration(9),
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                ConflictDecision {
                    action: ConflictAction::Skip,
                    scope: DecisionScope::Remaining,
                },
            )
            .unwrap();
        assert_eq!(summary.plan, *plan.id());
        assert_eq!(recorded.lock().unwrap().as_ref(), Some(&plan));
    }

    #[test]
    fn elevated_helper_rejects_requests_outside_the_private_temp_contract() {
        let fixture = Fixture::new();
        let request = fixture.0.join("near-elevated-forged.toml");
        fs::write(&request, "forged").unwrap();
        fs::set_permissions(&request, fs::Permissions::from_mode(0o600)).unwrap();
        let error = validate_elevated_request_path(&request).unwrap_err();
        assert!(error.contains("outside"));
    }

    #[test]
    fn local_provider_exposes_native_roots_with_metadata() {
        let locations = LocalFileProvider.locations();
        assert!(!locations.is_empty());
        assert!(locations.iter().all(|location| {
            location.location.as_str().starts_with("file://")
                && !location.label.is_empty()
                && location.detail.contains("native")
        }));
        #[cfg(not(windows))]
        assert!(locations.iter().any(|location| {
            LocalFileProvider::path(&location.location).is_ok_and(|path| path == Path::new("/"))
        }));
    }

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("near-local-fs-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn block_on<T>(mut future: ProviderFuture<'_, T>) -> Result<T, ProviderError> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(result) => return result,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn command_line_executor_runs_in_the_panel_directory() {
        let fixture = Fixture::new();
        let output = LocalCommandLineExecutor
            .execute(
                &LocalFileProvider::location(&fixture.0),
                "printf 'near-command:'; pwd",
            )
            .unwrap();

        assert_eq!(output.exit_code, Some(0));
        assert!(output.stderr.is_empty());
        assert!(output.stdout.starts_with("near-command:"));
        assert!(
            output
                .stdout
                .contains(&fixture.0.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn command_history_store_round_trips_locked_entries_atomically() {
        let fixture = Fixture::new();
        let store = LocalCommandHistoryStore::new(fixture.0.join("command-history.toml"));
        let mut locked = CommandHistoryEntry::new("git status");
        locked.locked = true;
        locked.use_count = 7;
        store
            .save(&[CommandHistoryEntry::new("pwd"), locked.clone()])
            .unwrap();

        assert_eq!(
            store.load().unwrap(),
            [CommandHistoryEntry::new("pwd"), locked]
        );
        assert!(store.path().exists());
        assert!(
            !fixture
                .0
                .join(format!("command-history.tmp-{}", std::process::id()))
                .exists()
        );
    }

    #[test]
    fn resource_history_store_round_trips_retention_locks_and_errors() {
        let fixture = Fixture::new();
        let store = LocalResourceHistoryStore::new(fixture.0.join("resource-history.toml"));
        let mut viewed = near_core::ResourceHistoryEntry::new(
            ResourceRef {
                provider: ProviderId::from("near.local-fs"),
                location: LocalFileProvider::location(&fixture.0.join("viewed.txt")),
            },
            "viewed.txt",
        );
        viewed.locked = true;
        viewed.use_count = 4;
        viewed.last_error = Some("temporarily unavailable".to_owned());
        let state = ResourceHistoryState {
            viewed: vec![viewed],
            edited: vec![near_core::ResourceHistoryEntry::new(
                ResourceRef {
                    provider: ProviderId::from("near.local-fs"),
                    location: LocalFileProvider::location(&fixture.0.join("edited.txt")),
                },
                "edited.txt",
            )],
            max_unlocked: 37,
        };
        store.save(&state).unwrap();

        assert_eq!(store.load().unwrap(), state);
        assert!(store.path().exists());
        assert!(
            !fixture
                .0
                .join(format!("resource-history.tmp-{}", std::process::id()))
                .exists()
        );
    }

    #[test]
    fn state_document_store_replaces_documents_and_keeps_a_recovery_copy() {
        let fixture = Fixture::new();
        let store = LocalStateDocumentStore::new(&fixture.0);

        assert_eq!(store.load("temporary-panels.toml").unwrap(), None);
        store
            .persist("temporary-panels.toml", "schema_version = 1\nvalue = 1\n")
            .unwrap();
        store
            .persist("temporary-panels.toml", "schema_version = 1\nvalue = 2\n")
            .unwrap();

        assert_eq!(
            store.load("temporary-panels.toml").unwrap().as_deref(),
            Some("schema_version = 1\nvalue = 2\n")
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("temporary-panels.toml.bak")).unwrap(),
            "schema_version = 1\nvalue = 1\n"
        );
        assert!(store.persist("../escape.toml", "bad").is_err());
    }

    #[test]
    fn command_line_arguments_quote_native_paths_and_reject_virtual_providers() {
        let fixture = Fixture::new();
        let file = fixture.0.join("two words'file.txt");
        fs::write(&file, "content").unwrap();
        let resolver = LocalCommandLineArgumentResolver;
        let location = LocalFileProvider::location(&fixture.0);
        let resource = ResourceRef {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&file),
        };

        assert_eq!(
            resolver.location_argument(&location).unwrap(),
            resolver.quote_text(fixture.0.to_str().unwrap())
        );
        assert_eq!(
            resolver.resource_argument(&resource).unwrap(),
            resolver.quote_text(file.to_str().unwrap())
        );
        let virtual_resource = ResourceRef {
            provider: ProviderId::from("near.virtual"),
            location: Location::new("virtual:///item"),
        };
        assert!(
            resolver
                .resource_argument(&virtual_resource)
                .unwrap_err()
                .contains("no native command-line path")
        );
    }

    #[test]
    fn shipped_platform_handlers_keep_open_separate_from_view_and_edit() {
        let fixture = Fixture::new();
        let path = fixture.0.join("document.txt");
        fs::write(&path, "content").unwrap();
        let resource = LocalFileProvider::resource_for_path(&path);

        let macos =
            LocalExternalToolResolver::from_toml(include_str!("../../../specs/handlers.toml"))
                .unwrap();
        let macos_open = macos
            .resolve_explained(ExternalAction::Open, &resource)
            .unwrap();
        assert_eq!(macos_open.handler_id, "near.handler.macos-default");
        assert_eq!(
            macos_open.invocation.program.to_string_lossy(),
            "/usr/bin/open"
        );
        assert_eq!(
            macos_open.invocation.arguments,
            [path.clone().into_os_string()]
        );
        let macos_view = macos
            .resolve_explained(ExternalAction::View, &resource)
            .unwrap();
        assert_eq!(macos_view.handler_id, "near.handler.macos-text");
        assert_eq!(
            macos_view.invocation.arguments,
            ["-W".into(), "-t".into(), path.clone().into_os_string()]
        );

        let linux = LocalExternalToolResolver::from_toml(include_str!(
            "../../../specs/handlers-linux.toml"
        ))
        .unwrap();
        assert_eq!(
            linux
                .resolve_explained(ExternalAction::Open, &resource)
                .unwrap()
                .invocation
                .program
                .to_string_lossy(),
            "xdg-open"
        );

        let windows = LocalExternalToolResolver::from_toml(include_str!(
            "../../../specs/handlers-windows.toml"
        ))
        .unwrap();
        assert_eq!(
            windows
                .resolve_explained(ExternalAction::Open, &resource)
                .unwrap()
                .invocation
                .program
                .to_string_lossy(),
            "explorer.exe"
        );
        assert_eq!(
            windows
                .resolve_explained(ExternalAction::View, &resource)
                .unwrap()
                .invocation
                .program
                .to_string_lossy(),
            "notepad.exe"
        );
    }

    #[test]
    fn folder_navigation_store_round_trips_shortcuts_history_and_errors() {
        let fixture = Fixture::new();
        let store = LocalFolderNavigationStore::new(fixture.0.join("folder-navigation.toml"));
        let mut unavailable = near_core::FolderLocationEntry::new(
            ProviderId::from("near.missing"),
            Location::new("missing:///folder"),
            "Unavailable folder",
        );
        unavailable.last_error = Some("Provider unavailable".to_owned());
        let mut shortcuts = vec![None; 10];
        shortcuts[0] = Some(unavailable.clone());
        let state = FolderNavigationState {
            history: vec![unavailable.clone()],
            shortcuts,
            max_unlocked: 41,
        };
        store.save(&state).unwrap();

        assert_eq!(store.load().unwrap(), state);
        assert!(store.path().exists());
    }

    #[test]
    fn editor_position_store_round_trips_provider_locations() {
        let fixture = Fixture::new();
        let store = LocalEditorPositionStore::new(fixture.0.join("editor-positions.toml"));
        let entries = vec![EditorPositionEntry {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&fixture.0.join("document.txt")),
            row: 42,
            column: 7,
            top: 35,
        }];
        store.save(&entries).unwrap();
        assert_eq!(store.load().unwrap(), entries);
        assert!(store.path().exists());
    }

    #[test]
    fn viewer_state_store_round_trips_bookmarks_and_navigation() {
        let fixture = Fixture::new();
        let store = LocalViewerStateStore::new(fixture.0.join("viewer-state.toml"));
        let entries = vec![ViewerStateEntry {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&fixture.0.join("document.txt")),
            offset: 81,
            bookmarks: BTreeMap::from([(3, 21), (7, 63)]),
            navigation_history: vec![0, 21, 81],
            navigation_index: 2,
            encoding: Some("utf-16le".to_owned()),
            wrap: Some(true),
            hex: Some(false),
        }];
        store.save(&entries).unwrap();
        assert_eq!(store.load().unwrap(), entries);
        assert!(store.path().exists());
    }

    #[test]
    fn file_locations_and_display_names_round_trip_invalid_utf8() {
        let fixture = Fixture::new();
        let name = OsString::from_vec(vec![b'n', b'a', b'm', b'e', b'-', 0xff]);
        let path = fixture.0.join(&name);

        let location = LocalFileProvider::location(&path);
        assert_eq!(LocalFileProvider::path(&location).unwrap(), path);
        let display = escape_name(&name);
        assert_eq!(display, "name-\\xFF");
        assert_eq!(unescape_name(&display).unwrap(), name);
    }

    #[test]
    fn provider_write_replaces_file_contents_and_honors_cancellation() {
        let fixture = Fixture::new();
        let path = fixture.0.join("editable.txt");
        fs::write(&path, "before").unwrap();
        let resource = ResourceRef {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&path),
        };
        block_on(LocalFileProvider.write(
            &resource,
            WriteRequest {
                bytes: b"after".to_vec(),
                expected: None,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "after");

        let cancellation = CancellationToken::default();
        cancellation.cancel();
        assert_eq!(
            block_on(LocalFileProvider.write(
                &resource,
                WriteRequest {
                    bytes: b"discarded".to_vec(),
                    expected: None,
                    cancellation,
                },
            )),
            Err(ProviderError::Cancelled)
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "after");
    }

    #[test]
    fn provider_write_rejects_an_external_version_change() {
        let fixture = Fixture::new();
        let path = fixture.0.join("changed.txt");
        fs::write(&path, "initial").unwrap();
        let resource = ResourceRef {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&path),
        };
        let metadata = block_on(LocalFileProvider.stat(&resource)).unwrap();
        fs::write(&path, "external-change").unwrap();
        let result = block_on(LocalFileProvider.write(
            &resource,
            WriteRequest {
                bytes: b"editor-change".to_vec(),
                expected: Some(near_core::ResourceVersion {
                    size: metadata.size,
                    modified_unix_ms: metadata.modified_unix_ms,
                }),
                cancellation: CancellationToken::default(),
            },
        ));
        assert!(matches!(result, Err(ProviderError::Conflict(_))));
        assert_eq!(fs::read_to_string(path).unwrap(), "external-change");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unavailable_macos_metadata_is_absent_without_field_errors() {
        let fixture = Fixture::new();
        let path = fixture.0.join("portable.txt");
        fs::write(&path, "portable").unwrap();
        let resource = ResourceRef {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&path),
        };
        let metadata = block_on(LocalFileProvider.stat(&resource)).unwrap();
        assert!(
            metadata
                .extensions
                .keys()
                .all(|field| !field.starts_with("macos."))
        );
        assert!(
            metadata
                .field_errors
                .keys()
                .all(|field| !field.starts_with("macos."))
        );
        assert!(metadata.permissions.is_some());
        assert!(metadata.owner.is_some());
    }

    #[test]
    fn list_is_paged_name_first_and_distinguishes_packages_and_symlinks() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("Example.app")).unwrap();
        fs::write(fixture.0.join("plain.txt"), b"0123456789").unwrap();
        symlink("plain.txt", fixture.0.join("link")).unwrap();
        let provider = LocalFileProvider;
        let first = block_on(provider.list(
            &LocalFileProvider::location(&fixture.0),
            ListRequest {
                generation: ListingGeneration(4),
                continuation: None,
                page_size: 2,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(first.generation, ListingGeneration(4));
        assert_eq!(first.entries.len(), 2);
        assert!(first.continuation.is_some());
        assert!(
            first
                .entries
                .iter()
                .any(|entry| entry.metadata.kind == ResourceKind::Package)
        );
        let first_has_symlink = first
            .entries
            .iter()
            .any(|entry| entry.metadata.kind == ResourceKind::Symlink);
        assert!(
            first
                .entries
                .iter()
                .all(|entry| entry.metadata.size.is_none())
        );

        let second = block_on(provider.list(
            &LocalFileProvider::location(&fixture.0),
            ListRequest {
                generation: ListingGeneration(4),
                continuation: first.continuation,
                page_size: 2,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert!(second.complete);
        let second_has_symlink = second
            .entries
            .iter()
            .any(|entry| entry.metadata.kind == ResourceKind::Symlink);
        assert!(first_has_symlink || second_has_symlink);
    }

    #[test]
    fn stat_reports_identity_permissions_links_and_field_errors() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("target"), b"target").unwrap();
        let link = fixture.0.join("link");
        symlink("target", &link).unwrap();
        let provider = LocalFileProvider;
        let resource = LocalFileProvider::resource(&link);
        let metadata = block_on(provider.stat(&resource)).unwrap();
        assert_eq!(metadata.kind, ResourceKind::Symlink);
        assert!(metadata.stable_id.as_deref().unwrap().starts_with("unix:"));
        assert!(metadata.permissions.is_some());
        assert!(metadata.owner.is_some());
        assert_eq!(
            LocalFileProvider::path(metadata.link_target.as_ref().unwrap()).unwrap(),
            fixture.0.join("target")
        );
        #[cfg(target_os = "macos")]
        {
            assert!(metadata.extensions.contains_key("macos.xattrs"));
            assert!(metadata.extensions.contains_key("macos.acl"));
            let mut failed_field = ResourceMetadata::default();
            collect_command_field(
                &link,
                &mut failed_field,
                "test.denied-field",
                "/definitely/not/a/program",
                &[],
            );
            assert!(failed_field.field_errors.contains_key("test.denied-field"));
        }
        #[cfg(target_os = "linux")]
        assert!(
            metadata
                .extensions
                .keys()
                .all(|field| !field.starts_with("macos."))
        );
    }

    #[test]
    fn open_reads_bounded_ranges_and_honors_cancellation() {
        let fixture = Fixture::new();
        let path = fixture.0.join("data.bin");
        fs::write(&path, b"0123456789").unwrap();
        let provider = LocalFileProvider;
        let resource = LocalFileProvider::resource(&path);
        let stream = block_on(provider.open(
            &resource,
            OpenRequest {
                offset: 2,
                length: 4,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(stream.bytes, b"2345");
        assert_eq!(stream.total_size, Some(10));
        assert!(!stream.complete);

        let cancellation = CancellationToken::default();
        cancellation.cancel();
        assert_eq!(
            block_on(provider.open(
                &resource,
                OpenRequest {
                    offset: 0,
                    length: 4,
                    cancellation,
                },
            )),
            Err(ProviderError::Cancelled)
        );
    }

    #[test]
    fn capabilities_exclude_unsupported_file_operations() {
        let fixture = Fixture::new();
        let directory = LocalFileProvider::resource(&fixture.0);
        let file_path = fixture.0.join("read-only.txt");
        fs::write(&file_path, b"data").unwrap();
        let mut permissions = fs::metadata(&file_path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file_path, permissions).unwrap();
        let file = LocalFileProvider::resource(&file_path);
        let provider = LocalFileProvider;

        let directory_capabilities = provider.capabilities(&directory);
        assert!(directory_capabilities.contains(&"resource.list".into()));
        assert!(directory_capabilities.contains(&"resource.create-directory".into()));
        let file_capabilities = provider.capabilities(&file);
        assert!(file_capabilities.contains(&"resource.read".into()));
        assert!(file_capabilities.contains(&"resource.delete".into()));
        assert!(!file_capabilities.contains(&"resource.wipe".into()));
        assert!(!file_capabilities.contains(&"resource.write".into()));
        assert!(!file_capabilities.contains(&"resource.list".into()));
    }

    #[test]
    fn external_handler_preserves_hostile_path_as_one_argument() {
        let fixture = Fixture::new();
        let name = OsString::from_vec(b"spaces $()[]\n\xff.txt".to_vec());
        let path = fixture.0.join(name);
        let resolver = LocalExternalToolResolver::new("editor", ["--wait"]);
        let invocation = resolver
            .resolve(
                ExternalAction::Edit,
                &LocalFileProvider::resource_for_path(&path),
            )
            .unwrap();
        assert_eq!(invocation.program, OsString::from("editor"));
        assert_eq!(invocation.arguments.len(), 2);
        assert_eq!(invocation.arguments[0], OsString::from("--wait"));
        assert_eq!(invocation.arguments[1], path.as_os_str());
        assert_eq!(invocation.current_directory, Some(fixture.0.clone()));
    }

    struct ReplaceRemaining;

    impl ConflictResolver for ReplaceRemaining {
        fn decide(&mut self, _plan: &OperationPlan, _item: &PlannedItem) -> ConflictDecision {
            ConflictDecision {
                action: ConflictAction::Replace,
                scope: DecisionScope::Remaining,
            }
        }
    }

    fn execute(plan: OperationPlan, backend: LocalOperationBackend) -> near_ops::ExecutionSummary {
        let generation = plan.context_generation();
        let mut engine = OperationEngine::new(backend, OperationJournal::memory());
        let id = engine.record(plan).unwrap();
        engine
            .execute(
                &id,
                ExecutionAuthorization {
                    context_generation: generation,
                    confirmed: true,
                    high_impact_confirmed: true,
                },
                &CancellationToken::default(),
                &mut ReplaceRemaining,
            )
            .unwrap()
    }

    #[test]
    fn copy_plan_previews_conflicts_and_preserves_recovery_backup() {
        let fixture = Fixture::new();
        let source_directory = fixture.0.join("source");
        let destination = fixture.0.join("destination");
        fs::create_dir_all(&source_directory).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = source_directory.join("item.txt");
        fs::write(&source, b"new").unwrap();
        fs::write(destination.join("item.txt"), b"old").unwrap();
        let planner = LocalOperationPlanner::new(fixture.0.join("Trash"));
        let plan = planner
            .copy(
                &[LocalFileProvider::resource_for_path(&source)],
                &LocalFileProvider::location(&destination),
                ListingGeneration(3),
                ConflictPolicy::Ask,
            )
            .unwrap();
        assert_eq!(plan.conflict_count(), 1);
        assert!(
            plan.preview_lines()
                .iter()
                .any(|line| line.contains("Backup"))
        );
        let summary = execute(plan, LocalOperationBackend::new(fixture.0.join("Trash")));
        assert_eq!(summary.completed(), 1);
        assert_eq!(fs::read(destination.join("item.txt")).unwrap(), b"new");
        assert!(fs::read_dir(&destination).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".near-backup")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn trash_rejects_filesystem_roots_before_recording_a_plan() {
        let fixture = Fixture::new();
        let planner = LocalOperationPlanner::new(fixture.0.join("Trash"));
        let error = planner
            .trash(
                &[LocalFileProvider::resource_for_path(Path::new("/"))],
                ListingGeneration(3),
            )
            .unwrap_err();
        assert!(error.to_string().contains("eject or unmount it instead"));

        assert!(
            planner
                .delete(
                    &[LocalFileProvider::resource_for_path(Path::new("/"))],
                    ListingGeneration(3),
                    true,
                )
                .unwrap_err()
                .to_string()
                .contains("filesystem root")
        );
        assert!(
            planner
                .wipe(
                    &[LocalFileProvider::resource_for_path(Path::new("/"))],
                    ListingGeneration(3),
                    1,
                )
                .unwrap_err()
                .to_string()
                .contains("filesystem root")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_volume_namespace_roots_are_protected_even_when_unmounted() {
        let resource =
            LocalFileProvider::resource_for_path(Path::new("/Volumes/Unmounted\\ Volume"));
        assert_eq!(
            LocalFileProvider::classify_resource(&resource).unwrap(),
            ResourceClassification::MountRoot
        );
        assert!(matches!(
            LocalFileProvider::mutation_eligibility(&resource, MutationKind::Trash),
            MutationEligibility::Denied(MutationDenial {
                alternative: Some(MutationAlternative::Unmount),
                ..
            })
        ));
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires a disposable mounted filesystem harness"]
    fn mounted_volume_root_is_rejected_before_recording_a_plan() {
        let mount_root = std::env::var_os("NEAR_TEST_MOUNT_ROOT")
            .map(PathBuf::from)
            .expect("NEAR_TEST_MOUNT_ROOT must name the disposable mounted volume");
        #[cfg(target_os = "macos")]
        assert!(mount_root.starts_with("/Volumes/"));
        let resource = LocalFileProvider::resource_for_path(&mount_root);
        assert_eq!(
            LocalFileProvider::classify_resource(&resource).unwrap(),
            ResourceClassification::MountRoot
        );
        for mutation in [
            MutationKind::Trash,
            MutationKind::Delete,
            MutationKind::Wipe,
        ] {
            assert_eq!(
                LocalFileProvider::mutation_eligibility(&resource, mutation),
                MutationEligibility::Denied(MutationDenial {
                    reason: format!(
                        "cannot mutate mounted volume {}; eject or unmount it instead",
                        resource.location.as_str()
                    ),
                    alternative: Some(MutationAlternative::Unmount),
                })
            );
        }

        let fixture = Fixture::new();
        let planner = LocalOperationPlanner::new(fixture.0.join("Trash"));
        assert!(
            planner
                .trash(std::slice::from_ref(&resource), ListingGeneration(17))
                .unwrap_err()
                .to_string()
                .contains("cannot mutate mounted volume")
        );
        assert!(
            planner
                .delete(std::slice::from_ref(&resource), ListingGeneration(17), true)
                .unwrap_err()
                .to_string()
                .contains("cannot mutate mounted volume")
        );
        assert!(
            planner
                .wipe(std::slice::from_ref(&resource), ListingGeneration(17), 1)
                .unwrap_err()
                .to_string()
                .contains("cannot mutate mounted volume")
        );
        assert!(mount_root.join("near-mount-sentinel.txt").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn mutation_classification_distinguishes_symlinks_and_ordinary_entries() {
        let fixture = Fixture::new();
        let file = fixture.0.join("file.txt");
        let link = fixture.0.join("link.txt");
        fs::write(&file, b"content").unwrap();
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let planner = LocalOperationPlanner::new(fixture.0.join("Trash"));

        assert_eq!(
            planner
                .classify(&LocalFileProvider::resource_for_path(&file))
                .unwrap(),
            ResourceClassification::Ordinary
        );
        assert_eq!(
            planner
                .classify(&LocalFileProvider::resource_for_path(&link))
                .unwrap(),
            ResourceClassification::Symlink
        );
        assert!(
            planner
                .eligibility(
                    &LocalFileProvider::resource_for_path(&link),
                    MutationKind::Trash,
                )
                .is_allowed()
        );
    }

    #[test]
    fn move_mkdir_and_trash_execute_only_from_recorded_plans() {
        let fixture = Fixture::new();
        let source_directory = fixture.0.join("source");
        let destination = fixture.0.join("destination");
        let trash = fixture.0.join("Trash");
        fs::create_dir_all(&source_directory).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = source_directory.join("move.txt");
        fs::write(&source, b"move").unwrap();
        let planner = LocalOperationPlanner::new(trash.clone());
        let move_plan = planner
            .move_resources(
                &[LocalFileProvider::resource_for_path(&source)],
                &LocalFileProvider::location(&destination),
                ListingGeneration(4),
                ConflictPolicy::Ask,
            )
            .unwrap();
        assert_eq!(
            move_plan.policies().cross_device,
            CrossDeviceBehavior::AtomicRename
        );
        assert_eq!(
            execute(move_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        let moved = destination.join("move.txt");
        assert!(!source.exists());
        assert!(moved.exists());

        let directory = destination.join("created");
        let mkdir_plan = planner
            .create_directory(
                LocalFileProvider::location(&directory),
                ListingGeneration(4),
            )
            .unwrap();
        assert_eq!(
            execute(mkdir_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert!(directory.is_dir());

        let trash_plan = planner
            .trash(
                &[LocalFileProvider::resource_for_path(&moved)],
                ListingGeneration(4),
            )
            .unwrap();
        assert_eq!(trash_plan.policies().recovery, RecoveryPolicy::Trash);
        let trash_summary = execute(trash_plan, LocalOperationBackend::new(trash.clone()));
        assert_eq!(trash_summary.completed(), 1);
        assert!(!moved.exists());
        assert!(trash.join("move.txt").exists());

        let restore_plan = planner
            .restore(
                &[(
                    ResourceRef {
                        provider: ProviderId::from("near.local-fs"),
                        location: trash_summary.items[0].item.target.clone(),
                    },
                    LocalFileProvider::location(&moved),
                )],
                ListingGeneration(4),
            )
            .unwrap();
        assert_eq!(restore_plan.kind(), OperationKind::Restore);
        assert_eq!(
            execute(restore_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert_eq!(fs::read(&moved).unwrap(), b"move");
        assert!(!trash.join("move.txt").exists());
    }

    #[test]
    fn trash_collision_preserves_existing_and_incoming_resources() {
        let fixture = Fixture::new();
        let trash = fixture.0.join("Trash");
        fs::create_dir_all(&trash).unwrap();
        fs::write(trash.join("same.txt"), b"existing").unwrap();
        let source = fixture.0.join("same.txt");
        fs::write(&source, b"incoming").unwrap();
        let planner = LocalOperationPlanner::new(trash.clone());
        let plan = planner
            .trash(
                &[LocalFileProvider::resource_for_path(&source)],
                ListingGeneration(5),
            )
            .unwrap();

        assert_eq!(plan.conflict_count(), 1);
        assert_eq!(
            execute(plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert_eq!(fs::read(trash.join("same.txt")).unwrap(), b"existing");
        assert!(fs::read_dir(&trash).unwrap().any(|entry| {
            let entry = entry.unwrap();
            entry.file_name() != "same.txt" && fs::read(entry.path()).unwrap() == b"incoming"
        }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "moves disposable fixtures through the current user's native macOS Trash"]
    fn macos_native_trash_helper_preserves_colliding_items() {
        assert!(std::env::var_os("NEAR_NATIVE_TRASH_HELPER").is_some());
        let fixture = Fixture::new();
        let sequence = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("near-native-trash-{}-{sequence}.txt", std::process::id());
        let first_directory = fixture.0.join("first");
        let second_directory = fixture.0.join("second");
        fs::create_dir_all(&first_directory).unwrap();
        fs::create_dir_all(&second_directory).unwrap();
        let first = first_directory.join(&name);
        let second = second_directory.join(&name);
        fs::write(&first, b"first-native-trash-item").unwrap();
        fs::write(&second, b"second-native-trash-item").unwrap();
        let planner = LocalOperationPlanner::macos_default();
        let mut recorded_restorations = Vec::new();
        for (generation, source) in [(41, &first), (42, &second)] {
            let original_contents = fs::read(source).unwrap();
            let plan = planner
                .trash(
                    &[LocalFileProvider::resource_for_path(source)],
                    ListingGeneration(generation),
                )
                .unwrap();
            assert_eq!(plan.conflict_count(), 0);
            let summary = execute(plan, LocalOperationBackend::macos_default());
            assert_eq!(summary.completed(), 1);
            let outcome = &summary.items[0].item;
            let original = LocalFileProvider::path(
                &outcome
                    .source
                    .as_ref()
                    .expect("Trash outcome must retain its source")
                    .location,
            )
            .unwrap();
            assert_eq!(original, *source);
            recorded_restorations.push((
                LocalFileProvider::path(&outcome.target)
                    .expect("native Trash outcome must record its actual target"),
                original,
                original_contents,
            ));
            assert!(!source.exists());
        }
        let mut recorded_targets = recorded_restorations
            .iter()
            .map(|(target, _, _)| target.clone())
            .collect::<Vec<_>>();
        recorded_targets.sort();
        recorded_targets.dedup();
        assert_eq!(recorded_targets.len(), 2);
        assert!(recorded_targets.iter().all(|target| {
            target
                .file_name()
                .is_some_and(|value| value.to_string_lossy().starts_with(&name))
        }));
        let restore_plan = planner
            .restore(
                &recorded_restorations
                    .iter()
                    .map(|(target, original, _)| {
                        (
                            LocalFileProvider::resource_for_path(target),
                            LocalFileProvider::location(original),
                        )
                    })
                    .collect::<Vec<_>>(),
                ListingGeneration(43),
            )
            .unwrap();
        let restore_summary = execute(restore_plan, LocalOperationBackend::macos_default());
        assert_eq!(restore_summary.completed(), 2);
        for (_, original, original_contents) in recorded_restorations {
            assert_eq!(fs::read(original).unwrap(), original_contents);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_native_trash_helper_has_a_bounded_timeout() {
        let fixture = Fixture::new();
        let helper = fixture.0.join("stalled-trash-helper");
        fs::write(&helper, "#!/bin/sh\nexec /usr/bin/tail -f /dev/null\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();

        let started = std::time::Instant::now();
        let error = run_macos_trash_helper_with(
            &helper,
            Path::new("unused-source"),
            Duration::from_millis(50),
        )
        .unwrap_err();

        assert!(error.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_native_restore_helper_has_a_bounded_timeout() {
        let fixture = Fixture::new();
        let helper = fixture.0.join("stalled-restore-helper");
        fs::write(&helper, "#!/bin/sh\nexec /usr/bin/tail -f /dev/null\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();

        let started = std::time::Instant::now();
        let error = run_macos_restore_helper_with(
            &helper,
            Path::new("unused-source"),
            Path::new("unused-target"),
            Duration::from_millis(50),
        )
        .unwrap_err();

        assert!(error.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn permanent_delete_and_wipe_are_high_impact_recorded_plans() {
        let fixture = Fixture::new();
        let trash = fixture.0.join("Trash");
        let planner = LocalOperationPlanner::new(trash.clone());
        let delete_path = fixture.0.join("delete.txt");
        fs::write(&delete_path, b"delete").unwrap();
        let delete_plan = planner
            .delete(
                &[LocalFileProvider::resource_for_path(&delete_path)],
                ListingGeneration(8),
                false,
            )
            .unwrap();
        assert_eq!(delete_plan.kind(), OperationKind::Delete);
        assert!(delete_plan.high_impact());
        assert_eq!(
            execute(delete_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert!(!delete_path.exists());

        let wipe_path = fixture.0.join("wipe.bin");
        fs::write(&wipe_path, vec![0x5a_u8; 128 * 1024]).unwrap();
        let wipe_plan = planner
            .wipe(
                &[LocalFileProvider::resource_for_path(&wipe_path)],
                ListingGeneration(9),
                3,
            )
            .unwrap();
        assert_eq!(wipe_plan.kind(), OperationKind::Wipe);
        assert!(wipe_plan.high_impact());
        assert!(
            wipe_plan
                .preview_lines()
                .iter()
                .any(|line| line.contains("passes") && line.contains('3'))
        );
        assert_eq!(
            execute(wipe_plan, LocalOperationBackend::new(trash)).completed(),
            1
        );
        assert!(!wipe_path.exists());

        assert!(
            planner
                .wipe(
                    &[LocalFileProvider::resource_for_path(&fixture.0)],
                    ListingGeneration(10),
                    1,
                )
                .unwrap_err()
                .to_string()
                .contains("regular files only")
        );
    }

    #[test]
    fn failed_items_remain_precise_and_retryable() {
        let fixture = Fixture::new();
        let destination = fixture.0.join("destination");
        fs::create_dir_all(&destination).unwrap();
        let missing = fixture.0.join("missing.txt");
        let planner = LocalOperationPlanner::new(fixture.0.join("Trash"));
        let plan = planner
            .copy(
                &[LocalFileProvider::resource_for_path(&missing)],
                &LocalFileProvider::location(&destination),
                ListingGeneration(9),
                ConflictPolicy::Ask,
            )
            .unwrap();
        let summary = execute(plan, LocalOperationBackend::new(fixture.0.join("Trash")));
        assert_eq!(summary.failed(), 1);
        assert_eq!(summary.completed(), 0);
        assert_eq!(summary.pending(), 0);
    }

    #[test]
    fn rename_links_touch_and_attributes_all_execute_from_plans() {
        let fixture = Fixture::new();
        let trash = fixture.0.join("Trash");
        let original = fixture.0.join("original.txt");
        fs::write(&original, b"content").unwrap();
        let planner = LocalOperationPlanner::new(trash.clone());
        let renamed = fixture.0.join("renamed.txt");
        let rename_plan = planner
            .rename(
                LocalFileProvider::resource_for_path(&original),
                LocalFileProvider::location(&renamed),
                ListingGeneration(10),
            )
            .unwrap();
        assert_eq!(
            execute(rename_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert!(renamed.exists());

        let hard = fixture.0.join("hard.txt");
        let hard_plan = planner
            .hard_link(
                LocalFileProvider::resource_for_path(&renamed),
                LocalFileProvider::location(&hard),
                ListingGeneration(10),
            )
            .unwrap();
        assert_eq!(
            execute(hard_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert_eq!(
            fs::metadata(&renamed).unwrap().ino(),
            fs::metadata(&hard).unwrap().ino()
        );

        let symbolic = fixture.0.join("symbolic.txt");
        let symbolic_plan = planner
            .symbolic_link(
                LocalFileProvider::resource_for_path(&renamed),
                LocalFileProvider::location(&symbolic),
                ListingGeneration(10),
            )
            .unwrap();
        assert_eq!(
            execute(symbolic_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert!(
            fs::symlink_metadata(&symbolic)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        let touched = fixture.0.join("touched.txt");
        let touch_plan = planner
            .touch(LocalFileProvider::location(&touched), ListingGeneration(10))
            .unwrap();
        assert_eq!(
            execute(touch_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert!(touched.exists());

        let attributes = planner
            .set_unix_mode(
                &LocalFileProvider::resource_for_path(&renamed),
                0o600,
                ListingGeneration(10),
            )
            .unwrap();
        assert_eq!(
            execute(attributes, LocalOperationBackend::new(trash)).completed(),
            1
        );
        assert_eq!(fs::metadata(renamed).unwrap().mode() & 0o777, 0o600);
    }

    #[test]
    fn attribute_failures_remain_itemized_after_partial_execution() {
        let fixture = Fixture::new();
        let first = fixture.0.join("first.txt");
        let missing = fixture.0.join("missing.txt");
        fs::write(&first, b"first").unwrap();
        fs::write(&missing, b"missing").unwrap();
        let planner = LocalOperationPlanner::new(fixture.0.join("Trash"));
        let plan = planner
            .set_attributes(
                &[
                    LocalFileProvider::resource_for_path(&first),
                    LocalFileProvider::resource_for_path(&missing),
                ],
                &AttributeUpdate {
                    readonly: Some(true),
                    ..AttributeUpdate::default()
                },
                false,
                ListingGeneration(1),
            )
            .unwrap();
        fs::remove_file(&missing).unwrap();
        let summary = execute(plan, LocalOperationBackend::new(fixture.0.join("Trash")));
        assert_eq!(summary.completed(), 1);
        assert_eq!(summary.failed(), 1);
        assert!(matches!(
            summary.items[0].status,
            near_ops::ItemStatus::Completed
        ));
        assert!(matches!(
            summary.items[1].status,
            near_ops::ItemStatus::Failed(_)
        ));
        assert!(fs::metadata(first).unwrap().permissions().readonly());
    }

    #[test]
    fn permanent_delete_executes_only_from_a_confirmed_plan() {
        let fixture = Fixture::new();
        let file = fixture.0.join("delete.txt");
        let directory = fixture.0.join("delete-tree");
        fs::write(&file, b"delete me").unwrap();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("nested.txt"), b"delete me too").unwrap();
        let trash = fixture.0.join("Trash");
        let planner = LocalOperationPlanner::new(trash.clone());

        let file_plan = planner
            .delete(
                &[LocalFileProvider::resource_for_path(&file)],
                ListingGeneration(11),
                false,
            )
            .unwrap();
        assert_eq!(
            execute(file_plan, LocalOperationBackend::new(trash.clone())).completed(),
            1
        );
        assert!(!file.exists());

        let directory_plan = planner
            .delete(
                &[LocalFileProvider::resource_for_path(&directory)],
                ListingGeneration(11),
                true,
            )
            .unwrap();
        assert!(directory_plan.high_impact());
        assert_eq!(
            execute(directory_plan, LocalOperationBackend::new(trash)).completed(),
            1
        );
        assert!(!directory.exists());
    }

    #[test]
    fn described_provider_loads_edits_and_hides_utf8_bom_catalogs() {
        let fixture = Fixture::new();
        let file = fixture.0.join("file name.txt");
        fs::write(&file, "body").unwrap();
        fs::write(
            fixture.0.join("descript.ion"),
            b"\xef\xbb\xbf\"file name.txt\" Initial description\n",
        )
        .unwrap();
        let provider = DescribedLocalFileProvider::new(DescriptionSettings {
            encoding: DescriptionEncoding::Latin1,
            ..DescriptionSettings::default()
        });
        let page = block_on(provider.list(
            &LocalFileProvider::location(&fixture.0),
            ListRequest {
                generation: ListingGeneration(1),
                continuation: None,
                page_size: 50,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(
            page.entries[0]
                .metadata
                .extensions
                .get(RESOURCE_DESCRIPTION_KEY),
            Some(&MetadataValue::String("Initial description".to_owned()))
        );

        block_on(provider.set_description(
            &LocalFileProvider::resource_for_path(&file),
            Some("Updated description".to_owned()),
        ))
        .unwrap();
        assert_eq!(
            load_description_catalog(&fixture.0, provider.settings())
                .unwrap()
                .get("file name.txt"),
            Some(&"Updated description".to_owned())
        );
        assert!(
            block_on(provider.set_description(
                &LocalFileProvider::resource_for_path(&file),
                Some("snowman ☃".to_owned()),
            ))
            .is_err()
        );
    }

    #[test]
    fn folder_description_files_are_configurable_and_creatable() {
        let fixture = Fixture::new();
        let settings = DescriptionSettings {
            folder_description_files: vec!["ABOUT.txt".to_owned()],
            encoding: DescriptionEncoding::Utf8Bom,
            ..DescriptionSettings::default()
        };
        let provider = DescribedLocalFileProvider::new(settings);
        assert!(
            block_on(provider.folder_description(&LocalFileProvider::location(&fixture.0), false))
                .unwrap()
                .is_none()
        );
        let resource =
            block_on(provider.folder_description(&LocalFileProvider::location(&fixture.0), true))
                .unwrap()
                .unwrap();
        assert_eq!(
            LocalFileProvider::path(&resource.location).unwrap(),
            fixture.0.join("ABOUT.txt")
        );
        assert_eq!(
            fs::read(fixture.0.join("ABOUT.txt")).unwrap(),
            [0xef, 0xbb, 0xbf]
        );
    }

    #[test]
    fn copy_and_rename_operations_keep_description_catalogs_in_sync() {
        let fixture = Fixture::new();
        let source_directory = fixture.0.join("source");
        let destination = fixture.0.join("destination");
        fs::create_dir_all(&source_directory).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = source_directory.join("item.txt");
        fs::write(&source, "body").unwrap();
        let settings = DescriptionSettings::default();
        update_path_description(&source, Some("Catalog entry".to_owned()), &settings).unwrap();

        let planner = LocalOperationPlanner::new(fixture.0.join("trash"));
        let copy = planner
            .copy(
                &[LocalFileProvider::resource_for_path(&source)],
                &LocalFileProvider::location(&destination),
                ListingGeneration(1),
                ConflictPolicy::Ask,
            )
            .unwrap();
        let mut backend = LocalOperationBackend::new(fixture.0.join("trash"));
        backend.description_settings = Some(settings.clone());
        assert_eq!(execute(copy, backend).completed(), 1);
        assert_eq!(
            load_description_catalog(&destination, &settings)
                .unwrap()
                .get("item.txt"),
            Some(&"Catalog entry".to_owned())
        );

        let copied = destination.join("item.txt");
        let renamed = destination.join("renamed.txt");
        let rename = planner
            .rename_many(
                &[(
                    LocalFileProvider::resource_for_path(&copied),
                    "renamed.txt".to_owned(),
                )],
                ListingGeneration(2),
            )
            .unwrap();
        let mut backend = LocalOperationBackend::new(fixture.0.join("trash"));
        backend.description_settings = Some(settings.clone());
        assert_eq!(execute(rename, backend).completed(), 1);
        let catalog = load_description_catalog(&destination, &settings).unwrap();
        assert_eq!(
            catalog.get("renamed.txt"),
            Some(&"Catalog entry".to_owned())
        );
        assert!(!catalog.contains_key("item.txt"));
        assert!(renamed.exists());

        let delete = planner
            .delete(
                &[LocalFileProvider::resource_for_path(&renamed)],
                ListingGeneration(3),
                true,
            )
            .unwrap();
        let mut backend = LocalOperationBackend::new(fixture.0.join("trash"));
        backend.description_settings = Some(settings.clone());
        assert_eq!(execute(delete, backend).completed(), 1);
        assert!(
            load_description_catalog(&destination, &settings)
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn windows_paths_round_trip_without_changing_the_public_location_type() {
        for path in [
            PathBuf::from(r"C:\Users\Near\資料.txt"),
            PathBuf::from(r"\\server\share\folder\item.txt"),
        ] {
            assert_eq!(
                LocalFileProvider::path(&LocalFileProvider::location(&path)).unwrap(),
                path
            );
        }
    }

    #[test]
    fn windows_native_references_are_claimed_by_the_local_provider() {
        for path in [
            PathBuf::from(r"C:\Users\Near\資料.txt"),
            PathBuf::from(r"\\server\share\folder\item.txt"),
        ] {
            assert_eq!(
                LocalFileProvider
                    .parse_native_reference(path.to_str().unwrap())
                    .unwrap(),
                Some(LocalFileProvider::resource_for_path(&path))
            );
        }
        assert_eq!(
            LocalFileProvider
                .parse_native_reference("relative\\item.txt")
                .unwrap(),
            None
        );
    }

    #[test]
    fn windows_file_attributes_are_typed_metadata_extensions() {
        let path = std::env::temp_dir().join(format!(
            "near-windows-metadata-{}-fixture.txt",
            std::process::id()
        ));
        fs::write(&path, "fixture").unwrap();
        let stat = fs::symlink_metadata(&path).unwrap();
        let metadata = platform_metadata(&path, path.file_name().unwrap(), &stat);
        assert!(metadata.extensions.contains_key("windows.file-attributes"));
        assert!(metadata.extensions.contains_key("windows.reparse-point"));
        assert!(metadata.permissions.is_some());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn windows_optional_metadata_is_typed_or_a_field_local_error() {
        let path = std::env::temp_dir().join(format!(
            "near-windows-optional-metadata-{}-fixture.txt",
            std::process::id()
        ));
        fs::write(&path, "fixture").unwrap();
        let metadata = LocalFileProvider::rich_metadata(&path).unwrap();
        for field in ["windows.acl", "windows.alternate-streams"] {
            assert!(
                metadata.extensions.contains_key(field)
                    || metadata.field_errors.contains_key(field),
                "{field} should be represented without failing the resource"
            );
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn windows_recycle_bin_script_uses_a_structured_path_argument() {
        let file = windows_recycle_script(false);
        let directory = windows_recycle_script(true);
        assert!(file.contains("DeleteFile($args[0]"));
        assert!(directory.contains("DeleteDirectory($args[0]"));
        assert!(file.contains("SendToRecycleBin"));
        assert!(directory.contains("SendToRecycleBin"));
        assert!(!file.contains("C:\\hostile path"));
    }
}
