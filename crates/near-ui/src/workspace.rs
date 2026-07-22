#![allow(
    clippy::assigning_clones,
    clippy::format_push_string,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    clippy::items_after_statements,
    clippy::obfuscated_if_else,
    clippy::redundant_clone,
    clippy::cloned_ref_to_slice_refs,
    clippy::needless_raw_string_hashes,
    clippy::field_reassign_with_default,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

use std::{
    cell::Cell,
    collections::BTreeMap,
    io, mem,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

use near_core::{
    ActionContext, ApplyCommandMode, ApplyCommandTarget, ApplyCommandTemplate, ArgumentKind,
    ArgumentSchema, Availability, CancellationToken, CapabilityId, CapabilitySet, Clipboard,
    CommandDescriptor, CommandExtension, CommandHistoryStore, CommandId, CommandInvocation,
    CommandLineArgumentResolver, CommandLineExecutor, CommandLineOutput, CommandPrefixDescriptor,
    CommandValue, ContextId, DiagnosticDomain, DiagnosticPhase, EditorPositionEntry,
    EditorPositionStore, ExtensionEffect, ExtensionMenuItem, ExtensionSetting, ExternalAction,
    ExternalInvocation, ExternalToolResolver, FolderLocationEntry, FolderNavigationState,
    FolderNavigationStore, ListRequest, ListingGeneration, Location, MetadataValue,
    MutationAlternative, MutationDenial, MutationEligibility, MutationKind, OpenRequest,
    ProviderError, ProviderId, ProviderRegistry, RemovableDeviceService, ResourceHistoryEntry,
    ResourceHistoryState, ResourceHistoryStore, ResourceKind, ResourceMetadata, ResourceProvider,
    ResourceRef, ResourceStream, SafetyClass, StateDocumentStore, ViewerStateEntry,
    ViewerStateStore,
};
use near_handlers::{
    UserMenuCatalog, UserMenuContext, UserMenuInvocationTemplate, UserMenuResource, UserMenuScope,
};
use near_macros::{
    MACRO_SCHEMA_VERSION, MacroCondition, MacroContext, MacroDocument, MacroEngine, MacroHost,
    MacroRecorder, MacroStore, MacroTrust, PresenceCondition, SemanticMacro,
};
use near_ops::{
    AttributeUpdate, ConflictAction, ConflictDecision, DecisionScope, ExecutionAuthorization,
    ExecutionSummary, LinkKind, OperationIntent, OperationKind, OperationService,
};
use near_runtime::{TaskCompletion, TaskHandle, TaskOutcome, TaskPool, block_on};
use near_search::{
    AlternateStreamSearchPolicy, ArchiveSearchPolicy, ContentMatch, ContentPredicate, HiddenPolicy,
    IgnorePolicy, ResourcePredicate, ScopedSearchRequest, SearchDiagnostic, SearchEncoding,
    SearchError, SearchEvent, SearchHit, SearchProgress, SearchResultsProvider, SearchRoot,
    SearchService, SymlinkSearchPolicy, TextPredicate,
};
use near_terminal::{
    Key, KeyKind, KeyStroke, KeyboardMode, ModifierKey, Modifiers, MouseButton, MouseEvent,
    MouseEventKind, RuntimeWakeHandle, TerminalEvent, TerminalSessionError,
};
use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use near_config::{
    SettingApplyScope, SettingDescriptor, SettingPlatformAvailability, SettingProvenance,
    SettingState, SettingValue,
};

use crate::command_line::CommandLineState;
use crate::workspace_diagnostics::WorkspaceDiagnostics;
use crate::workspace_settings::WorkspaceSettings;
#[path = "workspace_lookup.rs"]
mod lookup_ui;
#[path = "workspace_screens.rs"]
mod screen_runtime;
#[path = "workspace_settings_ui.rs"]
mod settings_ui;
#[path = "workspace_terminal.rs"]
mod terminal_runtime;
use crate::{
    CollectionEntry, CollectionLookupMode, CollectionSurface, CollectionTargetScope, Command,
    CommandHistorySurface, CommandRegistry, ComparisonSelection, ConfirmationPolicy, DialogField,
    DialogSurface, DualSurfaceLayout, DualSurfaceSide, EditorEncoding, EditorLineEnding,
    EditorPosition, EditorSaveFormat, EditorSaveOutcome, EditorSettings, EditorSurface,
    FilterCatalog, FolderComparisonPolicy, FolderHistorySurface, HelpEntry, HelpLink, HelpSurface,
    HelpTopic, HighlightingCatalog, HistorySettings, InspectorField, InspectorSurface,
    InterfaceSettings, KeyBinding, Keymap, KeymapSettings, MenuItem, MenuSurface,
    OperationPreviewSurface, PaneSlot, PanelModeCatalog, RenderContext, ResolveResult,
    ResourceHistoryKind, ResourceHistorySurface, ResourceOpenPolicy, SceneRect, SemanticTheme,
    SettingSurfaceEntry, SettingsDocumentStore, SettingsSurface, SortMode, Surface, SurfaceEvent,
    SurfacePresentation, TabRegistry, TaskRecord, TaskState, TaskSurface, TerminalColorDepth,
    TerminalSurface, TreeNode, TreeSurface, UpdateContext, UpdateResult, ViewerRequestTracker,
    ViewerSettings, ViewerSurface, ZoomablePanePresentation, compare_folders, format_key_sequence,
    format_key_stroke, format_semantic_color, parse_key_stroke, parse_semantic_color,
    scene_renderer::render_scene,
    selection_search::SelectionSearch,
    semantic::{RoleBuffer, SemanticSnapshot},
};

const PARENT_ENTRY_EXTENSION: &str = "near.navigation.parent";
const TEMPORARY_PANEL_STATE_DOCUMENT: &str = "temporary-panels.toml";
const TEMPORARY_PANEL_STATE_SCHEMA: u16 = 1;

#[cfg(feature = "embedded-pty")]
use crate::{EmbeddedTerminalDockSurface, EmbeddedTerminalSession, EmbeddedTerminalSurface};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionItem {
    pub name: String,
    pub details: String,
    pub is_directory: bool,
    pub selected: bool,
}

impl CollectionItem {
    pub fn file(name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            details: details.into(),
            is_directory: false,
            selected: false,
        }
    }

    pub fn directory(name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            details: details.into(),
            is_directory: true,
            selected: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FocusedPanel {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MouseDrag {
    source: FocusedPanel,
    move_items: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PanelType {
    #[default]
    File,
    Tree,
    Information,
    QuickView,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MainMenuCategory {
    Left,
    Files,
    Commands,
    Options,
    Right,
}

impl MainMenuCategory {
    const ALL: [Self; 5] = [
        Self::Left,
        Self::Files,
        Self::Commands,
        Self::Options,
        Self::Right,
    ];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or(0)
    }

    fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }
}

enum Overlay {
    Menu(MenuSurface),
    CommandHistory(CommandHistorySurface),
    FolderHistory(FolderHistorySurface),
    ResourceHistory(ResourceHistorySurface),
    CommandPalette {
        selected: usize,
        entries: Vec<PaletteEntry>,
        search: SelectionSearch,
    },
    Surface(Box<dyn Surface>),
    Message {
        title: String,
        body: String,
    },
}

enum WorkspaceTaskResult {
    QuickView {
        ticket: crate::ViewerLoadTicket,
        title: String,
        provider: Arc<dyn ResourceProvider>,
        resource: ResourceRef,
        stream: Result<ResourceStream, ProviderError>,
    },
    QuickViewDirectory {
        ticket: crate::ViewerLoadTicket,
        title: String,
        location: Location,
        page: Result<near_core::ListPage, ProviderError>,
    },
    Operation {
        result: Result<ExecutionSummary, String>,
    },
    ListingPage {
        panel: FocusedPanel,
        generation: ListingGeneration,
        location: Location,
        provider: Arc<dyn ResourceProvider>,
        page: Result<near_core::ListPage, ProviderError>,
    },
    MetadataHydration {
        panel: FocusedPanel,
        generation: ListingGeneration,
        location: Location,
        results: Vec<(ResourceRef, Result<ResourceMetadata, String>)>,
    },
    GeneratedPanelRefresh {
        panel: FocusedPanel,
        session: u64,
        results: Vec<(ResourceRef, Result<ResourceMetadata, String>)>,
    },
    SearchComplete {
        panel: FocusedPanel,
        session: u64,
        result: Result<SearchProgress, SearchError>,
    },
    SearchRefined {
        panel: FocusedPanel,
        session: u64,
        result: Result<Vec<SearchHit>, SearchError>,
    },
    CommandLine {
        command: String,
        result: Result<CommandLineOutput, String>,
    },
    TemporaryPanelCommand {
        panel: FocusedPanel,
        slot: u8,
        replace: bool,
        allow_arbitrary: bool,
        command: String,
        result: Result<CommandLineOutput, String>,
    },
    ApplyCommand(ApplyCommandSummary),
    DescriptionUpdated {
        count: usize,
        result: Result<(), String>,
    },
}

struct ApplyCommandExecution {
    sources: Vec<ResourceRef>,
    labels: Vec<String>,
    command: String,
    result: Result<CommandLineOutput, String>,
}

struct ApplyCommandSummary {
    planned: usize,
    executions: Vec<ApplyCommandExecution>,
    cancelled: bool,
}

struct SearchUpdate {
    panel: FocusedPanel,
    session: u64,
    event: SearchEvent,
}

struct SearchState {
    session: u64,
    provider: Arc<SearchResultsProvider>,
    providers: ProviderRegistry,
    request: ScopedSearchRequest,
    diagnostics: Vec<SearchDiagnostic>,
    task: Option<TaskHandle>,
}

#[derive(Clone)]
struct SavedSearchPanel {
    session: u64,
    label: String,
    provider: Arc<SearchResultsProvider>,
    providers: ProviderRegistry,
    request: ScopedSearchRequest,
    diagnostics: Vec<SearchDiagnostic>,
}

#[derive(Clone)]
struct ExtensionPanelState {
    session: u64,
    extension: String,
    provider: Arc<SearchResultsProvider>,
}

#[derive(Clone)]
struct SavedExtensionPanel {
    session: u64,
    label: String,
    extension: String,
    provider: Arc<SearchResultsProvider>,
}

#[derive(Clone)]
struct TemporaryPanel {
    slot: u8,
    location: Location,
    hits: Vec<SearchHit>,
    safe_mode: bool,
    allow_arbitrary: bool,
    full_screen: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TemporaryPanelRecord {
    slot: u8,
    #[serde(default)]
    hits: Vec<TemporaryPanelHitRecord>,
    #[serde(default)]
    safe_mode: bool,
    #[serde(default)]
    allow_arbitrary: bool,
    #[serde(default)]
    full_screen: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TemporaryPanelHitRecord {
    source: ResourceRef,
    metadata: ResourceMetadata,
    details: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TemporaryPanelDocument {
    schema_version: u16,
    #[serde(default)]
    last_used_slot: u8,
    #[serde(default)]
    panels: Vec<TemporaryPanelRecord>,
}

impl TemporaryPanel {
    fn new(slot: u8) -> Self {
        Self {
            slot,
            location: Location::new(format!("temporary://slots/{slot}")),
            hits: Vec::new(),
            safe_mode: false,
            allow_arbitrary: false,
            full_screen: false,
        }
    }

    fn stale_count(&self) -> usize {
        self.hits
            .iter()
            .filter(|hit| {
                hit.metadata
                    .extensions
                    .contains_key("near.temporary-panel.stale")
            })
            .count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchMode {
    Replace,
    Append,
    Refine,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SearchScope {
    #[default]
    CurrentRoots,
    SelectedRoots,
    Providers,
    Archives,
}

impl SearchScope {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "current" | "current-roots" => Some(Self::CurrentRoots),
            "selected" | "selected-roots" => Some(Self::SelectedRoots),
            "providers" | "provider-roots" => Some(Self::Providers),
            "archives" | "archive-roots" => Some(Self::Archives),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SearchOptions {
    scope: SearchScope,
    archives: ArchiveSearchPolicy,
    symlinks: SymlinkSearchPolicy,
    streams: AlternateStreamSearchPolicy,
}

const SEARCH_DIALOG_FIELDS: &[(&str, &str, &str, bool)] = &[
    (
        "scope",
        "Scope: current | selected | providers | archives",
        "current",
        true,
    ),
    (
        "archives",
        "Search nested archives: exclude | include",
        "exclude",
        true,
    ),
    (
        "symlinks",
        "Symbolic links: skip | match | follow",
        "match",
        true,
    ),
    (
        "streams",
        "Alternate streams: exclude | include",
        "exclude",
        true,
    ),
    ("name", "Name pattern", "*", true),
    (
        "name_mode",
        "Name mode: glob | regex | contains | exact",
        "glob",
        true,
    ),
    ("content", "Containing", "", false),
    (
        "content_mode",
        "Content mode: text | regex | hex",
        "text",
        true,
    ),
    (
        "encoding",
        "Encoding: auto | utf8 | utf16le | utf16be | latin1",
        "auto",
        true,
    ),
    (
        "case_sensitive",
        "Content case sensitive: yes | no",
        "no",
        true,
    ),
    (
        "kinds",
        "Kinds: all | file,directory,package,symlink,virtual,other",
        "all",
        true,
    ),
    (
        "minimum_size",
        "Minimum size (B, K, M, G; blank any)",
        "",
        false,
    ),
    (
        "maximum_size",
        "Maximum size (B, K, M, G; blank any)",
        "",
        false,
    ),
    (
        "modified_after",
        "Modified after (YYYY-MM-DD or Unix ms)",
        "",
        false,
    ),
    (
        "modified_before",
        "Modified before (YYYY-MM-DD or Unix ms)",
        "",
        false,
    ),
    ("readonly", "Read only: any | yes | no", "any", true),
    ("executable", "Executable: any | yes | no", "any", true),
    (
        "hidden",
        "Hidden: exclude | include | only",
        "exclude",
        true,
    ),
    ("ignore", "Ignore: none | vcs | common", "none", true),
    ("mode", "Mode", "replace", true),
];

impl SearchMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "replace" | "new" => Some(Self::Replace),
            "append" | "add" => Some(Self::Append),
            "refine" | "filter" => Some(Self::Refine),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum CommandPrefixOwner {
    TemporaryPanel,
    Provider(ProviderId),
    Extension {
        extension: String,
        command: CommandId,
        argument: String,
    },
}

#[derive(Clone, Debug)]
struct RegisteredCommandPrefix {
    description: String,
    owner: CommandPrefixOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FilenameLookup {
    panel: FocusedPanel,
    original_cursor: usize,
    query: String,
    mode: CollectionLookupMode,
    matches: Vec<usize>,
    active_match: usize,
}

struct TerminalTab {
    surface: Box<dyn Surface>,
    #[cfg(feature = "embedded-pty")]
    session: Option<EmbeddedTerminalSession>,
}

struct ListingState {
    generation: ListingGeneration,
    location: Location,
    tasks: Vec<TaskHandle>,
    loaded: usize,
    retained: Option<crate::CollectionStateSnapshot>,
}

impl ListingState {
    fn cancel(&mut self) {
        for task in self.tasks.drain(..) {
            task.cancel();
        }
    }

    fn remove_task(&mut self, id: near_runtime::RuntimeTaskId) {
        self.tasks.retain(|task| task.id() != id);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceAction {
    Command(CommandInvocation),
    PendingSequence(String),
    Noop,
}

#[derive(Debug, Error)]
pub enum RunWorkspaceError {
    #[error(transparent)]
    TerminalSession(#[from] TerminalSessionError),
    #[error("terminal I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("terminated by signal {0}")]
    Terminated(i32),
}

#[derive(Clone)]
struct ElevatedRetry {
    plan: near_core::OperationId,
    authorization: ExecutionAuthorization,
    conflict: ConflictDecision,
    elevated: bool,
}

fn permission_failure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("permission denied")
        || error.contains("operation not permitted")
        || error.contains("access denied")
}

pub struct FarWorkspace {
    left: CollectionSurface,
    right: CollectionSurface,
    left_panel_type: PanelType,
    right_panel_type: PanelType,
    quick_view_replaced: Option<(FocusedPanel, PanelType)>,
    focused: FocusedPanel,
    editors: Vec<EditorSurface>,
    active_editor: Option<usize>,
    editor_position_store: Option<Arc<dyn EditorPositionStore>>,
    editor_positions: BTreeMap<String, EditorPositionEntry>,
    viewer_state_store: Option<Arc<dyn ViewerStateStore>>,
    viewer_states: BTreeMap<String, ViewerStateEntry>,
    settings: WorkspaceSettings,
    active_settings_category: Option<String>,
    clipboard: Option<Arc<dyn Clipboard>>,
    overlay: Option<Overlay>,
    overlay_history: Vec<Overlay>,
    suspended_overlay: Option<Overlay>,
    terminals: TabRegistry<TerminalTab>,
    terminal_presentation: ZoomablePanePresentation,
    status: String,
    pending_sequence: String,
    keyboard_mode: KeyboardMode,
    held_modifiers: Modifiers,
    viewport: Cell<(u16, u16)>,
    panel_layout: DualSurfaceLayout,
    mouse_drag: Option<MouseDrag>,
    command_line: CommandLineState,
    filename_lookup: Option<FilenameLookup>,
    left_saved_selection: Vec<ResourceRef>,
    right_saved_selection: Vec<ResourceRef>,
    filter_catalog: FilterCatalog,
    active_filters: BTreeMap<FocusedPanel, Vec<String>>,
    command_line_executor: Option<Arc<dyn CommandLineExecutor>>,
    command_line_arguments: Option<Arc<dyn CommandLineArgumentResolver>>,
    command_history_store: Option<Arc<dyn CommandHistoryStore>>,
    folder_navigation_store: Option<Arc<dyn FolderNavigationStore>>,
    folder_navigation: FolderNavigationState,
    resource_history_store: Option<Arc<dyn ResourceHistoryStore>>,
    resource_history: ResourceHistoryState,
    command_line_task: Option<TaskHandle>,
    apply_command_task: Option<TaskHandle>,
    should_quit: bool,
    registry: CommandRegistry,
    providers: ProviderRegistry,
    operations: Option<Arc<Mutex<Box<dyn OperationService>>>>,
    configuration_diagnostics: String,
    theme_presets: BTreeMap<String, SemanticTheme>,
    committed_theme: Option<SemanticTheme>,
    working_theme: Option<SemanticTheme>,
    external_tools: Option<Arc<dyn ExternalToolResolver>>,
    removable_devices: Option<Arc<dyn RemovableDeviceService>>,
    user_menus: UserMenuCatalog,
    pending_external: Option<ExternalInvocation>,
    quick_view: Option<ViewerSurface>,
    quick_view_interactive: bool,
    quick_view_requests: ViewerRequestTracker,
    quick_view_task: Option<TaskHandle>,
    operation_task: Option<TaskHandle>,
    operation_contexts: BTreeMap<u64, ElevatedRetry>,
    elevated_retry: Option<ElevatedRetry>,
    last_trash_restoration: Vec<(ResourceRef, Location)>,
    task_records: BTreeMap<u64, TaskRecord>,
    left_listing: Option<ListingState>,
    right_listing: Option<ListingState>,
    searches: BTreeMap<FocusedPanel, SearchState>,
    saved_search_panels: Vec<SavedSearchPanel>,
    extension_panels: BTreeMap<FocusedPanel, ExtensionPanelState>,
    saved_extension_panels: Vec<SavedExtensionPanel>,
    temporary_panels: BTreeMap<u8, TemporaryPanel>,
    active_temporary_panels: BTreeMap<FocusedPanel, u8>,
    last_temporary_panel_slot: u8,
    state_document_store: Option<Arc<dyn StateDocumentStore>>,
    pending_reveal_targets: BTreeMap<FocusedPanel, ResourceRef>,
    search_updates: mpsc::Receiver<SearchUpdate>,
    search_update_sender: mpsc::Sender<SearchUpdate>,
    next_search_session: u64,
    macro_recorder: MacroRecorder,
    macro_engine: MacroEngine,
    macro_catalog: BTreeMap<String, SemanticMacro>,
    macro_store: Option<Arc<dyn MacroStore>>,
    last_macro: Option<SemanticMacro>,
    macro_replaying: bool,
    extensions: BTreeMap<String, Arc<dyn CommandExtension>>,
    extension_commands: BTreeMap<CommandId, String>,
    extension_settings_open: BTreeMap<CommandId, String>,
    extension_settings_save: BTreeMap<CommandId, String>,
    command_prefixes: BTreeMap<String, RegisteredCommandPrefix>,
    embedded_pty_enabled: bool,
    tasks: TaskPool<WorkspaceTaskResult>,
    pub(crate) diagnostics: WorkspaceDiagnostics,
    generation: ListingGeneration,
}

impl FarWorkspace {
    pub fn new(left: CollectionSurface, right: CollectionSurface) -> Self {
        let (search_update_sender, search_updates) = mpsc::channel();
        let mut workspace = Self {
            left,
            right,
            left_panel_type: PanelType::File,
            right_panel_type: PanelType::File,
            quick_view_replaced: None,
            focused: FocusedPanel::Left,
            editors: Vec::new(),
            active_editor: None,
            editor_position_store: None,
            editor_positions: BTreeMap::new(),
            viewer_state_store: None,
            viewer_states: BTreeMap::new(),
            settings: WorkspaceSettings::default(),
            active_settings_category: None,
            clipboard: None,
            overlay: None,
            overlay_history: Vec::new(),
            suspended_overlay: None,
            terminals: TabRegistry::default(),
            terminal_presentation: ZoomablePanePresentation::default(),
            status: "Near workspace — semantic commands, providers, and surfaces".to_owned(),
            pending_sequence: String::new(),
            keyboard_mode: KeyboardMode::Enhanced,
            held_modifiers: Modifiers::default(),
            viewport: Cell::new((80, 24)),
            panel_layout: DualSurfaceLayout::default(),
            mouse_drag: None,
            command_line: CommandLineState::default(),
            filename_lookup: None,
            left_saved_selection: Vec::new(),
            right_saved_selection: Vec::new(),
            filter_catalog: FilterCatalog::default(),
            active_filters: BTreeMap::new(),
            command_line_executor: None,
            command_line_arguments: None,
            command_history_store: None,
            folder_navigation_store: None,
            folder_navigation: FolderNavigationState {
                history: Vec::new(),
                shortcuts: vec![None; 10],
                max_unlocked: 200,
            },
            resource_history_store: None,
            resource_history: ResourceHistoryState::default(),
            command_line_task: None,
            apply_command_task: None,
            should_quit: false,
            registry: CommandRegistry::default(),
            providers: ProviderRegistry::default(),
            operations: None,
            configuration_diagnostics: "Built-in configuration is active".to_owned(),
            theme_presets: BTreeMap::new(),
            committed_theme: None,
            working_theme: None,
            external_tools: None,
            removable_devices: None,
            user_menus: UserMenuCatalog::default(),
            pending_external: None,
            quick_view: None,
            quick_view_interactive: false,
            quick_view_requests: ViewerRequestTracker::default(),
            quick_view_task: None,
            operation_task: None,
            operation_contexts: BTreeMap::new(),
            elevated_retry: None,
            last_trash_restoration: Vec::new(),
            task_records: BTreeMap::new(),
            left_listing: None,
            right_listing: None,
            searches: BTreeMap::new(),
            saved_search_panels: Vec::new(),
            extension_panels: BTreeMap::new(),
            saved_extension_panels: Vec::new(),
            temporary_panels: BTreeMap::new(),
            active_temporary_panels: BTreeMap::new(),
            last_temporary_panel_slot: 0,
            state_document_store: None,
            pending_reveal_targets: BTreeMap::new(),
            search_updates,
            search_update_sender,
            next_search_session: 1,
            macro_recorder: MacroRecorder::default(),
            macro_engine: MacroEngine::default(),
            macro_catalog: BTreeMap::new(),
            macro_store: None,
            last_macro: None,
            macro_replaying: false,
            extensions: BTreeMap::new(),
            extension_commands: BTreeMap::new(),
            extension_settings_open: BTreeMap::new(),
            extension_settings_save: BTreeMap::new(),
            command_prefixes: BTreeMap::from([(
                "tmp".to_owned(),
                RegisteredCommandPrefix {
                    description: "Open or populate a Temporary Panel slot".to_owned(),
                    owner: CommandPrefixOwner::TemporaryPanel,
                },
            )]),
            embedded_pty_enabled: true,
            tasks: TaskPool::new(2, 32),
            diagnostics: WorkspaceDiagnostics::default(),
            generation: ListingGeneration(1),
        };
        workspace.register_commands();
        workspace
    }

    /// Registers a resource provider used by workspace navigation and viewing.
    ///
    /// # Panics
    ///
    /// Panics when another provider with the same stable ID is already registered.
    #[must_use]
    pub fn with_provider(mut self, provider: Arc<dyn ResourceProvider>) -> Self {
        self.register_provider(provider)
            .expect("workspace provider IDs and command prefixes must be unique");
        self
    }

    /// Registers a resource provider while preserving duplicate-ID failures for callers.
    ///
    /// # Errors
    ///
    /// Returns an error when another provider already owns the stable ID.
    pub fn try_with_provider(
        mut self,
        provider: Arc<dyn ResourceProvider>,
    ) -> Result<Self, String> {
        self.register_provider(provider)?;
        Ok(self)
    }

    /// Registers a provider without consuming the workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when another provider already owns the stable ID.
    pub fn register_provider(&mut self, provider: Arc<dyn ResourceProvider>) -> Result<(), String> {
        let provider_id = provider.id();
        let prefixes = provider.command_prefixes();
        self.validate_prefixes(&prefixes)?;
        self.providers
            .register(provider)
            .map_err(|error| error.to_string())?;
        for prefix in prefixes {
            self.command_prefixes.insert(
                prefix.name,
                RegisteredCommandPrefix {
                    description: prefix.description,
                    owner: CommandPrefixOwner::Provider(provider_id.clone()),
                },
            );
        }
        Ok(())
    }

    /// Starts paged background listings for both collection locations.
    #[must_use]
    pub fn with_initial_listings(mut self) -> Self {
        self.refresh_collections();
        self
    }

    #[must_use]
    pub fn with_operation_service(mut self, service: impl OperationService + 'static) -> Self {
        self.operations = Some(Arc::new(Mutex::new(Box::new(service))));
        self
    }

    #[must_use]
    pub fn with_confirmation_policy(mut self, policy: ConfirmationPolicy) -> Self {
        self.settings.confirmations = policy;
        self
    }

    #[must_use]
    pub fn with_configuration_diagnostics(mut self, diagnostics: impl Into<String>) -> Self {
        self.configuration_diagnostics = diagnostics.into();
        self
    }

    #[must_use]
    pub fn with_keyboard_mode(mut self, mode: KeyboardMode) -> Self {
        self.set_keyboard_mode(mode);
        self
    }

    pub fn keyboard_mode(&self) -> KeyboardMode {
        self.keyboard_mode
    }

    pub(crate) fn set_keyboard_mode(&mut self, mode: KeyboardMode) {
        self.keyboard_mode = mode;
        self.held_modifiers = Modifiers::default();
        let diagnostic = match mode {
            KeyboardMode::Enhanced => {
                "keyboard=enhanced; modifier press/release layers and held-key projection enabled"
            }
            KeyboardMode::Legacy => {
                "keyboard=legacy; modifier chords execute normally but hold-only keybar layers are unavailable"
            }
        };
        if !self.configuration_diagnostics.is_empty() {
            self.configuration_diagnostics.push('\n');
        }
        self.configuration_diagnostics.push_str(diagnostic);
    }

    #[must_use]
    pub fn with_theme_presets(
        mut self,
        initial: SemanticTheme,
        presets: impl IntoIterator<Item = SemanticTheme>,
    ) -> Self {
        self.theme_presets
            .insert(initial.name().to_owned(), initial.clone());
        for preset in presets {
            self.theme_presets.insert(preset.name().to_owned(), preset);
        }
        self.committed_theme = Some(initial.clone());
        self.working_theme = Some(initial);
        self
    }

    pub(crate) fn effective_theme(&self, fallback: &SemanticTheme) -> SemanticTheme {
        self.working_theme
            .clone()
            .unwrap_or_else(|| fallback.clone())
            .with_depth(fallback.terminal_depth())
    }

    pub(crate) fn set_theme_depth(&mut self, depth: TerminalColorDepth) {
        for theme in self.theme_presets.values_mut() {
            *theme = theme.clone().with_depth(depth);
        }
        if let Some(theme) = self.committed_theme.as_mut() {
            *theme = theme.clone().with_depth(depth);
        }
        if let Some(theme) = self.working_theme.as_mut() {
            *theme = theme.clone().with_depth(depth);
        }
    }

    #[must_use]
    pub fn with_macros(mut self, macros: impl IntoIterator<Item = SemanticMacro>) -> Self {
        for semantic_macro in macros {
            self.last_macro = Some(semantic_macro.clone());
            self.macro_catalog
                .insert(semantic_macro.id.clone(), semantic_macro);
        }
        self
    }

    #[must_use]
    pub fn with_macro_store(mut self, store: impl MacroStore + 'static) -> Self {
        self.macro_store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_external_tool_resolver(
        mut self,
        resolver: impl ExternalToolResolver + 'static,
    ) -> Self {
        self.external_tools = Some(Arc::new(resolver));
        self
    }

    #[must_use]
    pub fn with_removable_device_service(
        mut self,
        service: Arc<dyn RemovableDeviceService>,
    ) -> Self {
        self.removable_devices = Some(service);
        self
    }

    #[must_use]
    pub fn with_user_menus(mut self, catalog: UserMenuCatalog) -> Self {
        self.user_menus = catalog;
        self
    }

    #[must_use]
    pub fn with_command_line_executor(
        mut self,
        executor: impl CommandLineExecutor + 'static,
    ) -> Self {
        self.command_line_executor = Some(Arc::new(executor));
        self
    }

    #[must_use]
    pub fn with_command_line_argument_resolver(
        mut self,
        resolver: impl CommandLineArgumentResolver + 'static,
    ) -> Self {
        self.command_line_arguments = Some(Arc::new(resolver));
        self
    }

    #[must_use]
    pub fn with_command_history_store(mut self, store: impl CommandHistoryStore + 'static) -> Self {
        match store.load() {
            Ok(entries) => self.command_line.load_history(entries),
            Err(error) => self.status = format!("Cannot load command history: {error}"),
        }
        self.command_history_store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_folder_navigation_store(
        mut self,
        store: impl FolderNavigationStore + 'static,
    ) -> Self {
        match store.load() {
            Ok(mut state) => {
                state.shortcuts.resize(10, None);
                state.shortcuts.truncate(10);
                self.folder_navigation = state;
            }
            Err(error) => self.status = format!("Cannot load folder navigation: {error}"),
        }
        self.folder_navigation_store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_resource_history_store(
        mut self,
        store: impl ResourceHistoryStore + 'static,
    ) -> Self {
        match store.load() {
            Ok(state) => self.resource_history = state,
            Err(error) => self.status = format!("Cannot load resource history: {error}"),
        }
        self.resource_history_store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_state_document_store(mut self, store: impl StateDocumentStore + 'static) -> Self {
        match store.load(TEMPORARY_PANEL_STATE_DOCUMENT) {
            Ok(Some(source)) => match toml::from_str::<TemporaryPanelDocument>(&source) {
                Ok(document) if document.schema_version == TEMPORARY_PANEL_STATE_SCHEMA => {
                    self.last_temporary_panel_slot = document.last_used_slot.min(9);
                    self.temporary_panels = document
                        .panels
                        .into_iter()
                        .filter(|panel| panel.slot <= 9)
                        .map(|panel| {
                            let slot = panel.slot;
                            (
                                slot,
                                TemporaryPanel {
                                    slot,
                                    location: Location::new(format!("temporary://slots/{slot}")),
                                    hits: panel
                                        .hits
                                        .into_iter()
                                        .map(|hit| SearchHit {
                                            source: hit.source,
                                            metadata: hit.metadata,
                                            details: hit.details,
                                        })
                                        .collect(),
                                    safe_mode: panel.safe_mode,
                                    allow_arbitrary: panel.allow_arbitrary,
                                    full_screen: panel.full_screen,
                                },
                            )
                        })
                        .collect();
                    self.state_document_store = Some(Arc::new(store));
                }
                Ok(document) => {
                    self.status = format!(
                        "Cannot load Temporary Panels: unsupported schema {}; persisted state was left untouched",
                        document.schema_version
                    );
                }
                Err(error) => {
                    self.status = format!(
                        "Cannot load Temporary Panels: {error}; persisted state was left untouched"
                    );
                }
            },
            Ok(None) => self.state_document_store = Some(Arc::new(store)),
            Err(error) => {
                self.status = format!(
                    "Cannot load Temporary Panels: {error}; persisted state was left untouched"
                );
            }
        }
        self
    }

    #[must_use]
    pub fn with_history_settings(mut self, settings: HistorySettings) -> Self {
        self.settings.history = settings;
        self.command_line
            .set_max_unlocked_history(settings.command_max_unlocked);
        self.folder_navigation.max_unlocked = settings.folder_max_unlocked;
        self.resource_history.max_unlocked = settings.resource_max_unlocked;
        self.trim_folder_history();
        self.trim_resource_history(ResourceHistoryKind::Viewed);
        self.trim_resource_history(ResourceHistoryKind::Edited);
        self
    }

    #[must_use]
    pub fn with_interface_settings(mut self, settings: InterfaceSettings) -> Self {
        self.focused = match settings.startup_panel {
            crate::StartupPanel::Left => FocusedPanel::Left,
            crate::StartupPanel::Right => FocusedPanel::Right,
        };
        self.settings.interface = settings;
        self
    }

    #[must_use]
    pub fn with_editor_position_store(mut self, store: impl EditorPositionStore + 'static) -> Self {
        match store.load() {
            Ok(entries) => {
                self.editor_positions = entries
                    .into_iter()
                    .map(|entry| (editor_position_key(&entry.provider, &entry.location), entry))
                    .collect();
            }
            Err(error) => self.status = format!("Cannot load editor positions: {error}"),
        }
        self.editor_position_store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_viewer_state_store(mut self, store: impl ViewerStateStore + 'static) -> Self {
        match store.load() {
            Ok(entries) => {
                self.viewer_states = entries
                    .into_iter()
                    .map(|entry| (viewer_state_key(&entry.provider, &entry.location), entry))
                    .collect();
            }
            Err(error) => self.status = format!("Cannot load viewer state: {error}"),
        }
        self.viewer_state_store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_viewer_settings(mut self, settings: ViewerSettings) -> Self {
        self.settings.viewer = settings;
        self
    }

    #[must_use]
    pub fn with_clipboard(mut self, clipboard: impl Clipboard + 'static) -> Self {
        self.clipboard = Some(Arc::new(clipboard));
        self
    }

    #[must_use]
    pub fn with_editor_settings(mut self, settings: EditorSettings) -> Self {
        self.settings.editor = settings;
        self
    }

    #[must_use]
    pub fn with_settings_document_store(
        mut self,
        store: impl SettingsDocumentStore + 'static,
    ) -> Self {
        self.settings.store = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn with_keymap_document(mut self, source: String, settings: KeymapSettings) -> Self {
        self.settings.keymap = settings;
        self.settings.keymap_source = Some(source);
        self
    }

    #[must_use]
    pub fn with_setting_provenance(
        mut self,
        provenance: impl IntoIterator<Item = (String, SettingProvenance)>,
    ) -> Self {
        self.settings.provenance.extend(provenance);
        self
    }

    pub fn take_keymap_reload(&mut self) -> Option<String> {
        self.settings.pending_keymap_source.take()
    }

    pub fn report_keymap_reload(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.status = format!("Keymap reload failed; last-valid bindings retained: {error}");
        }
    }

    #[must_use]
    pub fn with_panel_modes(mut self, catalog: PanelModeCatalog) -> Self {
        if let Some(mode) = catalog.mode(catalog.left_default()).cloned() {
            self.left.set_view_mode(mode);
        }
        if let Some(mode) = catalog.mode(catalog.right_default()).cloned() {
            self.right.set_view_mode(mode);
        }
        self.settings.panel_modes = catalog;
        self
    }

    #[must_use]
    pub fn with_filters(mut self, catalog: FilterCatalog) -> Self {
        self.filter_catalog = catalog;
        self
    }

    #[must_use]
    pub fn with_highlighting(mut self, catalog: HighlightingCatalog) -> Self {
        self.left.set_highlighting(catalog.clone());
        self.right.set_highlighting(catalog);
        self
    }

    /// Enables or disables native embedded PTY sessions at runtime.
    #[must_use]
    pub fn with_embedded_pty(mut self, enabled: bool) -> Self {
        self.embedded_pty_enabled = enabled;
        self
    }

    #[cfg(feature = "embedded-pty")]
    #[must_use]
    pub fn with_shell_profile(mut self, profile: near_pty::ShellProfile) -> Self {
        self.settings.shell = profile;
        self
    }

    /// Registers an isolated extension through the shared semantic command path.
    ///
    /// # Errors
    ///
    /// Returns descriptor loading or duplicate command registration failures.
    pub fn try_with_extension(
        mut self,
        extension: Arc<dyn CommandExtension>,
    ) -> Result<Self, String> {
        self.register_extension(extension)?;
        Ok(self)
    }

    /// Registers an extension without consuming the workspace.
    ///
    /// # Errors
    ///
    /// Returns descriptor loading or duplicate command registration failures.
    pub fn register_extension(
        &mut self,
        extension: Arc<dyn CommandExtension>,
    ) -> Result<(), String> {
        let extension_id = extension.id().to_owned();
        if self.extensions.contains_key(&extension_id) {
            return Err(format!("duplicate extension ID {extension_id}"));
        }
        let commands = extension.commands()?;
        let prefixes = extension.command_prefixes()?;
        let menu_items = extension.menu_items()?;
        let settings = extension.settings()?;
        self.validate_prefixes(
            &prefixes
                .iter()
                .map(|prefix| prefix.prefix.clone())
                .collect::<Vec<_>>(),
        )?;
        for prefix in &prefixes {
            let Some(command) = commands.iter().find(|command| command.id == prefix.command) else {
                return Err(format!(
                    "extension {extension_id} prefix {} targets unknown command {}",
                    prefix.prefix.name, prefix.command
                ));
            };
            if !command.arguments.contains_key(&prefix.argument) {
                return Err(format!(
                    "extension {extension_id} prefix {} targets undeclared argument {}",
                    prefix.prefix.name, prefix.argument
                ));
            }
        }
        for item in &menu_items {
            if !commands.iter().any(|command| command.id == item.command) {
                return Err(format!(
                    "extension {extension_id} menu item {} targets unknown command {}",
                    item.label, item.command
                ));
            }
        }
        let mut setting_ids = std::collections::BTreeSet::new();
        for setting in &settings {
            if setting.id.trim().is_empty() || !setting_ids.insert(setting.id.clone()) {
                return Err(format!(
                    "extension {extension_id} has an empty or duplicate setting ID {}",
                    setting.id
                ));
            }
        }
        for descriptor in commands {
            let command_id = descriptor.id.clone();
            self.registry
                .register(StaticCommand(descriptor))
                .map_err(|error| error.to_string())?;
            self.extension_commands
                .insert(command_id, extension_id.clone());
        }
        for prefix in prefixes {
            self.command_prefixes.insert(
                prefix.prefix.name,
                RegisteredCommandPrefix {
                    description: prefix.prefix.description,
                    owner: CommandPrefixOwner::Extension {
                        extension: extension_id.clone(),
                        command: prefix.command,
                        argument: prefix.argument,
                    },
                },
            );
        }
        if !settings.is_empty() {
            let open_id = CommandId::from(format!("near.extension.{extension_id}.settings"));
            let save_id = CommandId::from(format!("near.extension.{extension_id}.settings-save"));
            self.registry
                .register(StaticCommand(CommandDescriptor {
                    id: open_id.clone(),
                    title: format!("Configure {extension_id}"),
                    description: "Edit extension settings".to_owned(),
                    category: vec!["Extensions".to_owned(), extension_id.clone()],
                    safety: SafetyClass::ReadOnly,
                    arguments: BTreeMap::new(),
                }))
                .map_err(|error| error.to_string())?;
            self.registry
                .register(StaticCommand(CommandDescriptor {
                    id: save_id.clone(),
                    title: format!("Save {extension_id} settings"),
                    description: "Validate and persist extension settings".to_owned(),
                    category: vec!["Extensions".to_owned(), extension_id.clone()],
                    safety: SafetyClass::Confirmable,
                    arguments: settings
                        .iter()
                        .map(|setting| {
                            (
                                setting.id.clone(),
                                ArgumentSchema {
                                    kind: ArgumentKind::String,
                                    required: setting.required,
                                    description: setting.description.clone(),
                                    default: None,
                                },
                            )
                        })
                        .collect(),
                }))
                .map_err(|error| error.to_string())?;
            self.extension_settings_open
                .insert(open_id, extension_id.clone());
            self.extension_settings_save
                .insert(save_id, extension_id.clone());
        }
        self.extensions.insert(extension_id, extension);
        Ok(())
    }

    fn validate_prefixes(&self, prefixes: &[CommandPrefixDescriptor]) -> Result<(), String> {
        let mut names = std::collections::BTreeSet::new();
        for prefix in prefixes {
            let name = prefix.name.as_str();
            if name.len() < 2
                || !name.chars().enumerate().all(|(index, character)| {
                    character.is_ascii_alphabetic()
                        || (index > 0
                            && (character.is_ascii_digit() || character == '-' || character == '_'))
                })
            {
                return Err(format!("invalid command prefix {name}"));
            }
            if self.command_prefixes.contains_key(name) || !names.insert(name) {
                return Err(format!("duplicate command prefix {name}"));
            }
        }
        Ok(())
    }

    pub fn take_external_invocation(&mut self) -> Option<ExternalInvocation> {
        self.pending_external.take()
    }

    pub fn report_external_exit(&mut self, status: std::process::ExitStatus) {
        self.status = match status.code() {
            Some(code) => format!("External tool exited with status {code}"),
            None => "External tool terminated by signal".to_owned(),
        };
    }

    pub fn report_external_error(&mut self, error: &io::Error) {
        self.status = format!("External tool failed: {error}");
    }

    pub fn poll_background_tasks(&mut self) -> bool {
        let mut changed = self.poll_search_updates();
        let tasks_open = matches!(
            &self.overlay,
            Some(Overlay::Surface(surface))
                if surface
                    .contexts()
                    .iter()
                    .any(|context| context.as_str() == "surface.tasks")
        );
        while let Some(completion) = self.tasks.try_completion() {
            changed = true;
            self.handle_task_completion(completion);
        }
        if tasks_open && changed {
            self.overlay = Some(Overlay::Surface(Box::new(self.task_surface())));
        }
        changed |= self.poll_search_updates();
        #[cfg(feature = "embedded-pty")]
        {
            changed |= self.reconcile_embedded_terminal_exit();
        }
        changed
    }

    pub(crate) fn install_runtime_wake(&self, wake: RuntimeWakeHandle) {
        self.tasks.set_completion_wake(move || {
            let _ = wake.wake();
        });
    }

    pub(crate) fn clear_runtime_wake(&self) {
        self.tasks.clear_completion_wake();
    }

    #[allow(clippy::too_many_lines)]
    fn handle_task_completion(&mut self, completion: TaskCompletion<WorkspaceTaskResult>) {
        if let Some((correlation, parent)) = self.diagnostics.tasks.remove(&completion.id.0) {
            let (domain, name) = match &completion.outcome {
                TaskOutcome::Completed(WorkspaceTaskResult::Operation { .. }) => {
                    (DiagnosticDomain::Operation, "operation")
                }
                TaskOutcome::Completed(
                    WorkspaceTaskResult::QuickView { .. }
                    | WorkspaceTaskResult::QuickViewDirectory { .. }
                    | WorkspaceTaskResult::ListingPage { .. }
                    | WorkspaceTaskResult::MetadataHydration { .. }
                    | WorkspaceTaskResult::GeneratedPanelRefresh { .. }
                    | WorkspaceTaskResult::SearchComplete { .. }
                    | WorkspaceTaskResult::SearchRefined { .. },
                ) => (DiagnosticDomain::Provider, "provider-task"),
                TaskOutcome::Completed(WorkspaceTaskResult::CommandLine { .. }) => {
                    (DiagnosticDomain::Terminal, "command-line")
                }
                TaskOutcome::Completed(WorkspaceTaskResult::TemporaryPanelCommand { .. }) => {
                    (DiagnosticDomain::Terminal, "temporary-panel-command")
                }
                TaskOutcome::Completed(WorkspaceTaskResult::ApplyCommand(_)) => {
                    (DiagnosticDomain::Terminal, "apply-command")
                }
                TaskOutcome::Completed(WorkspaceTaskResult::DescriptionUpdated { .. }) => {
                    (DiagnosticDomain::Provider, "description-update")
                }
                TaskOutcome::Cancelled | TaskOutcome::Panicked => {
                    (DiagnosticDomain::Task, "background-task")
                }
            };
            let phase = match &completion.outcome {
                TaskOutcome::Completed(_) => DiagnosticPhase::Completed,
                TaskOutcome::Cancelled => DiagnosticPhase::Cancelled,
                TaskOutcome::Panicked => DiagnosticPhase::Failed,
            };
            self.diagnostics.journal.record(
                correlation,
                parent,
                DiagnosticDomain::Task,
                phase,
                "background-task",
                BTreeMap::new(),
            );
            self.diagnostics.journal.record(
                correlation,
                parent,
                domain,
                phase,
                name,
                BTreeMap::new(),
            );
        }
        if let Some(state) = &mut self.left_listing {
            state.remove_task(completion.id);
        }
        if let Some(state) = &mut self.right_listing {
            state.remove_task(completion.id);
        }
        let is_current_quick_view = self
            .quick_view_task
            .as_ref()
            .is_some_and(|task| task.id() == completion.id);
        let is_current_operation = self
            .operation_task
            .as_ref()
            .is_some_and(|task| task.id() == completion.id);
        let is_current_command_line = self
            .command_line_task
            .as_ref()
            .is_some_and(|task| task.id() == completion.id);
        let is_current_apply_command = self
            .apply_command_task
            .as_ref()
            .is_some_and(|task| task.id() == completion.id);
        if is_current_quick_view {
            self.quick_view_task = None;
        }
        if is_current_operation {
            self.operation_task = None;
        }
        if is_current_command_line {
            self.command_line_task = None;
        }
        if is_current_apply_command {
            self.apply_command_task = None;
        }
        if let Some(record) = self.task_records.get_mut(&completion.id.0) {
            match &completion.outcome {
                TaskOutcome::Cancelled => record.cancel("Cancelled before completion"),
                TaskOutcome::Panicked => record.fail("Background task panicked"),
                TaskOutcome::Completed(_) => {}
            }
        }
        match completion.outcome {
            TaskOutcome::Completed(WorkspaceTaskResult::QuickView {
                ticket,
                title,
                provider,
                resource,
                stream,
            }) if self.quick_view_requests.is_current(&ticket) => match stream {
                Ok(stream) => {
                    let viewer = ViewerSurface::from_stream(
                        "near-fm.quick-view",
                        title,
                        provider,
                        resource,
                        ticket.cancellation(),
                        stream,
                    );
                    self.quick_view = Some(self.with_viewer_clipboard(viewer));
                    "Quick view updated".clone_into(&mut self.status);
                }
                Err(error) => {
                    self.quick_view = Some(ViewerSurface::text(
                        "near-fm.quick-view",
                        title,
                        error.to_string(),
                    ));
                }
            },
            TaskOutcome::Completed(WorkspaceTaskResult::QuickViewDirectory {
                ticket,
                title,
                location,
                page,
            }) if self.quick_view_requests.is_current(&ticket) => match page {
                Ok(page) => {
                    let viewer = ViewerSurface::text(
                        "near-fm.quick-view",
                        title,
                        directory_quick_view_summary(&location, &page),
                    );
                    self.quick_view = Some(self.with_viewer_clipboard(viewer));
                    "Directory quick view updated".clone_into(&mut self.status);
                }
                Err(error) => {
                    self.quick_view = Some(ViewerSurface::text(
                        "near-fm.quick-view",
                        title,
                        error.to_string(),
                    ));
                }
            },
            TaskOutcome::Panicked if is_current_quick_view => {
                self.quick_view = Some(ViewerSurface::text(
                    "near-fm.quick-view",
                    "Quick view",
                    "Background viewer task panicked",
                ));
            }
            TaskOutcome::Completed(WorkspaceTaskResult::Operation { result }) => {
                self.finish_operation_task(completion.id.0, result);
            }
            TaskOutcome::Completed(WorkspaceTaskResult::ListingPage {
                panel,
                generation,
                location,
                provider,
                page,
            }) => self.apply_listing_page(panel, generation, &location, &provider, page),
            TaskOutcome::Completed(WorkspaceTaskResult::MetadataHydration {
                panel,
                generation,
                location,
                results,
            }) => self.apply_metadata_hydration(panel, generation, &location, results),
            TaskOutcome::Completed(WorkspaceTaskResult::GeneratedPanelRefresh {
                panel,
                session,
                results,
            }) => self.finish_extension_panel_refresh(panel, session, results),
            TaskOutcome::Completed(WorkspaceTaskResult::SearchComplete {
                panel,
                session,
                result,
            }) => self.finish_search(panel, session, result),
            TaskOutcome::Completed(WorkspaceTaskResult::SearchRefined {
                panel,
                session,
                result,
            }) => self.finish_refined_search(panel, session, result),
            TaskOutcome::Completed(WorkspaceTaskResult::CommandLine { command, result }) => {
                self.finish_command_line(command, result);
            }
            TaskOutcome::Completed(WorkspaceTaskResult::TemporaryPanelCommand {
                panel,
                slot,
                replace,
                allow_arbitrary,
                command,
                result,
            }) => self.finish_temporary_panel_command(
                completion.id.0,
                panel,
                slot,
                (replace, allow_arbitrary),
                command,
                result,
            ),
            TaskOutcome::Completed(WorkspaceTaskResult::ApplyCommand(summary)) => {
                self.finish_apply_command(completion.id.0, summary);
            }
            TaskOutcome::Completed(WorkspaceTaskResult::DescriptionUpdated { count, result }) => {
                match result {
                    Ok(()) => {
                        self.status = format!("Updated descriptions for {count} resource(s)");
                        self.refresh_collections();
                    }
                    Err(error) => self.status = format!("Description update failed: {error}"),
                }
            }
            TaskOutcome::Cancelled if is_current_operation => {
                "Operation cancelled before execution".clone_into(&mut self.status);
            }
            TaskOutcome::Panicked if is_current_operation => {
                "Background operation panicked".clone_into(&mut self.status);
            }
            TaskOutcome::Panicked if is_current_command_line => {
                "Command execution task panicked".clone_into(&mut self.status);
            }
            TaskOutcome::Cancelled if is_current_apply_command => {
                self.finish_cancelled_apply_command(completion.id.0);
            }
            TaskOutcome::Panicked if is_current_apply_command => {
                "Apply-command task panicked".clone_into(&mut self.status);
            }
            TaskOutcome::Completed(_) | TaskOutcome::Cancelled | TaskOutcome::Panicked => {}
        }
    }

    fn poll_search_updates(&mut self) -> bool {
        let mut changed = false;
        while let Ok(update) = self.search_updates.try_recv() {
            let Some(state) = self.searches.get(&update.panel) else {
                continue;
            };
            if state.session != update.session {
                continue;
            }
            changed = true;
            match update.event {
                SearchEvent::Batch(hits) => {
                    let provider = Arc::clone(&state.provider);
                    let hits = provider.append_unique(hits);
                    let session = update.session.to_string();
                    if self.panel(update.panel).location() == provider.location() {
                        self.panel_mut(update.panel)
                            .append(hits.into_iter().map(|hit| {
                                let entry = hit.resource_entry(&session);
                                CollectionEntry::new(entry.resource, entry.metadata, entry.details)
                            }));
                    }
                    self.status = format!("Search found {} resources…", provider.len());
                }
                SearchEvent::Progress(progress) => {
                    self.status = format!(
                        "Search visited {} resources; {} matched…",
                        progress.visited, progress.matched
                    );
                }
                SearchEvent::Diagnostic(diagnostic) => {
                    self.status = format!(
                        "Search capability {} unavailable at {}: {}",
                        diagnostic.capability,
                        diagnostic.location.as_str(),
                        diagnostic.message
                    );
                    if let Some(state) = self.searches.get_mut(&update.panel) {
                        state.diagnostics.push(diagnostic);
                    }
                }
                SearchEvent::Finished(progress) => {
                    let diagnostics = self
                        .searches
                        .get(&update.panel)
                        .map_or(0, |state| state.diagnostics.len());
                    self.status = if diagnostics == 0 {
                        format!(
                            "Search complete: {} matches from {} resources",
                            progress.matched, progress.visited
                        )
                    } else {
                        format!(
                            "Search complete: {} matches from {} resources; {} capability diagnostics",
                            progress.matched, progress.visited, diagnostics
                        )
                    };
                }
            }
        }
        changed
    }

    fn finish_search(
        &mut self,
        panel: FocusedPanel,
        session: u64,
        result: Result<SearchProgress, SearchError>,
    ) {
        let Some(state) = self.searches.get_mut(&panel) else {
            return;
        };
        if state.session != session {
            return;
        }
        state.task = None;
        match result {
            Ok(progress) => {
                self.status = if state.diagnostics.is_empty() {
                    format!(
                        "Search complete: {} matches from {} resources",
                        progress.matched, progress.visited
                    )
                } else {
                    format!(
                        "Search complete: {} matches from {} resources; {} capability diagnostics",
                        progress.matched,
                        progress.visited,
                        state.diagnostics.len()
                    )
                };
            }
            Err(SearchError::Cancelled) => {
                self.status = format!(
                    "Search cancelled; retained {} results",
                    state.provider.len()
                );
            }
            Err(error) => self.status = format!("Search stopped: {error}"),
        }
    }

    fn finish_refined_search(
        &mut self,
        panel: FocusedPanel,
        session: u64,
        result: Result<Vec<SearchHit>, SearchError>,
    ) {
        let provider = {
            let Some(state) = self.searches.get_mut(&panel) else {
                return;
            };
            if state.session != session {
                return;
            }
            state.task = None;
            Arc::clone(&state.provider)
        };
        match result {
            Ok(hits) => {
                provider.replace(hits.clone());
                let location = provider.location().clone();
                if self.panel(panel).location() == &location {
                    self.panel_mut(panel).replace(
                        location,
                        hits.into_iter()
                            .map(|hit| {
                                let entry = hit.resource_entry(&session.to_string());
                                CollectionEntry::new(entry.resource, entry.metadata, entry.details)
                            })
                            .collect(),
                    );
                }
                self.status = format!("Refined search to {} results", provider.len());
            }
            Err(SearchError::Cancelled) => {
                "Search refinement cancelled; original results retained"
                    .clone_into(&mut self.status);
            }
            Err(error) => self.status = format!("Search refinement stopped: {error}"),
        }
    }

    fn finish_operation_task(&mut self, task: u64, result: Result<ExecutionSummary, String>) {
        let context = self.operation_contexts.remove(&task);
        match result {
            Ok(summary) => {
                if summary.kind == OperationKind::Trash {
                    let restorations = summary
                        .items
                        .iter()
                        .filter(|outcome| outcome.status == near_ops::ItemStatus::Completed)
                        .filter_map(|outcome| {
                            let original = outcome.item.source.as_ref()?;
                            Some((
                                ResourceRef {
                                    provider: original.provider.clone(),
                                    location: outcome.item.target.clone(),
                                },
                                original.location.clone(),
                            ))
                        })
                        .collect::<Vec<_>>();
                    if !restorations.is_empty() {
                        self.last_trash_restoration = restorations;
                    }
                } else if summary.kind == OperationKind::Restore {
                    self.last_trash_restoration = summary
                        .items
                        .iter()
                        .filter(|outcome| outcome.status != near_ops::ItemStatus::Completed)
                        .filter_map(|outcome| {
                            let source = outcome.item.source.clone()?;
                            Some((source, outcome.item.target.clone()))
                        })
                        .collect();
                }
                if let Some(record) = self.task_records.get_mut(&task) {
                    record.completed = u64::try_from(summary.completed()).unwrap_or(u64::MAX);
                    record.total = Some(u64::try_from(summary.items.len()).unwrap_or(u64::MAX));
                    record.state = if summary.cancelled {
                        TaskState::Cancelled
                    } else if summary.failed() > 0 {
                        TaskState::Failed
                    } else {
                        TaskState::Completed
                    };
                    record.message = format!(
                        "{} completed, {} skipped, {} failed, {} pending",
                        summary.completed(),
                        summary.skipped(),
                        summary.failed(),
                        summary.pending()
                    );
                }
                self.refresh_collections();
                self.status = format!(
                    "Operation {}: {} completed, {} skipped, {} failed, {} pending",
                    summary.plan,
                    summary.completed(),
                    summary.skipped(),
                    summary.failed(),
                    summary.pending()
                );
                let retry_available = summary.failed() > 0
                    && summary.items.iter().any(|item| match &item.status {
                        near_ops::ItemStatus::Failed(error) => permission_failure(error),
                        _ => false,
                    })
                    && context.as_ref().is_some_and(|context| !context.elevated);
                if retry_available {
                    let context = context.expect("retry availability requires operation context");
                    self.elevated_retry = Some(context);
                    self.status.push_str(
                        "; permission denied — run near.operation.retry-elevated to authorize the exact plan",
                    );
                } else if summary.failed() == 0 {
                    self.elevated_retry = None;
                }
                if summary.failed() > 0 {
                    let failure = summary.failure_presentation(retry_available).unwrap();
                    self.overlay = Some(Overlay::Message {
                        title: failure.title,
                        body: failure.body,
                    });
                }
            }
            Err(error) => {
                if let Some(record) = self.task_records.get_mut(&task) {
                    record.state = TaskState::Failed;
                    error.clone_into(&mut record.message);
                }
                self.status = error.clone();
                let failure = near_ops::OperationFailurePresentation::execution_error(error);
                self.overlay = Some(Overlay::Message {
                    title: failure.title,
                    body: failure.body,
                });
            }
        }
    }

    fn apply_listing_page(
        &mut self,
        panel: FocusedPanel,
        generation: ListingGeneration,
        location: &Location,
        provider: &Arc<dyn ResourceProvider>,
        page: Result<near_core::ListPage, ProviderError>,
    ) {
        let is_current = self
            .listing_state(panel)
            .is_some_and(|state| state.generation == generation && state.location == *location);
        if !is_current {
            return;
        }
        let page = match page {
            Ok(page) if page.generation == generation => page,
            Ok(_) | Err(ProviderError::Cancelled) => return,
            Err(error) => {
                self.status = format!("Partial listing failure at {}: {error}", location.as_str());
                self.mark_folder_error(&provider.id(), location, Some(error.to_string()));
                return;
            }
        };
        let resources = page
            .entries
            .iter()
            .map(|entry| entry.resource.clone())
            .collect::<Vec<_>>();
        let mut entries = page
            .entries
            .into_iter()
            .map(|entry| CollectionEntry {
                resource: entry.resource,
                metadata: entry.metadata,
                details: entry.details,
                selected: false,
            })
            .collect::<Vec<_>>();
        let first = self
            .listing_state(panel)
            .is_some_and(|state| state.loaded == 0);
        let count = entries.len();
        if first {
            self.mark_folder_error(&provider.id(), location, None);
            if let Some(parent) = parent_collection_entry(provider, location) {
                entries.insert(0, parent);
            }
            self.panel_mut(panel).replace(location.clone(), entries);
        } else {
            self.panel_mut(panel).append(entries);
        }
        if let Some(retained) = self
            .listing_state(panel)
            .and_then(|state| state.retained.clone())
        {
            self.panel_mut(panel).restore_state(&retained);
        }
        if let Some(target) = self.pending_reveal_targets.get(&panel).cloned()
            && self.panel_mut(panel).focus_resource(&target)
        {
            self.pending_reveal_targets.remove(&panel);
            self.status = format!("Revealed {}", target.location.as_str());
        }
        if let Some(state) = self.listing_state_mut(panel) {
            state.loaded = state.loaded.saturating_add(count);
        }
        let loaded = self.listing_state(panel).map_or(0, |state| state.loaded);
        self.status = if page.complete {
            format!("Loaded {loaded} resources from {}", location.as_str())
        } else {
            format!(
                "Loaded first {loaded} resources from {}…",
                location.as_str()
            )
        };
        self.schedule_metadata_hydration(
            panel,
            generation,
            location.clone(),
            Arc::clone(provider),
            resources,
        );
        if let Some(continuation) = page.continuation {
            self.schedule_listing_page(panel, generation, location, provider, Some(continuation));
        }
    }

    fn apply_metadata_hydration(
        &mut self,
        panel: FocusedPanel,
        generation: ListingGeneration,
        location: &Location,
        results: Vec<(ResourceRef, Result<ResourceMetadata, String>)>,
    ) {
        let is_current = self
            .listing_state(panel)
            .is_some_and(|state| state.generation == generation && &state.location == location);
        if !is_current {
            return;
        }
        let failures = results.iter().filter(|(_, result)| result.is_err()).count();
        for (resource, result) in results {
            self.panel_mut(panel).hydrate(&resource, result);
        }
        if failures > 0 {
            self.status = format!(
                "Loaded {} with {failures} metadata failures",
                location.as_str()
            );
        }
    }

    pub fn demo() -> Self {
        let mut workspace = Self::new(
            demo_collection(
                "near-fm.left",
                "Macintosh HD",
                "/Users/alex/Projects/Near",
                vec![
                    CollectionItem::directory("..", "parent"),
                    CollectionItem::directory("crates", "6 items"),
                    CollectionItem::directory("docs", "18 items"),
                    CollectionItem::directory("project", "9 items"),
                    CollectionItem::file("Cargo.toml", "1.2 KB"),
                    CollectionItem::file("README.md", "3.4 KB"),
                    CollectionItem::file("rust-toolchain.toml", "92 B"),
                ],
                1,
            ),
            demo_collection(
                "near-fm.right",
                "Home",
                "/Users/alex",
                vec![
                    CollectionItem::directory("..", "parent"),
                    CollectionItem::directory("Applications", "12 items"),
                    CollectionItem::directory("Desktop", "4 items"),
                    CollectionItem::directory("Documents", "37 items"),
                    CollectionItem::directory("Downloads", "91 items"),
                    CollectionItem::file("notes.txt", "8.1 KB"),
                ],
                3,
            ),
        );
        "Near interaction laboratory — semantic commands, keymaps, and themes"
            .clone_into(&mut workspace.status);
        workspace
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn active_contexts(&self) -> Vec<ContextId> {
        match self.overlay {
            Some(Overlay::Menu(_) | Overlay::CommandPalette { .. }) => {
                vec![ContextId::from("overlay.menu")]
            }
            Some(Overlay::CommandHistory(ref surface)) => surface.contexts(),
            Some(Overlay::FolderHistory(ref surface)) => surface.contexts(),
            Some(Overlay::ResourceHistory(ref surface)) => surface.contexts(),
            Some(Overlay::Surface(ref surface)) => surface.contexts(),
            Some(Overlay::Message { .. }) => vec![ContextId::from("overlay.viewer")],
            None if self.terminal_owns_focus() => self.active_terminal_surface().map_or_else(
                || vec![ContextId::from("workspace.panel")],
                Surface::contexts,
            ),
            None if self.quick_view_interactive => self.quick_view.as_ref().map_or_else(
                || vec![ContextId::from("workspace.panel")],
                Surface::contexts,
            ),
            None => self.active_editor().map_or_else(
                || {
                    if self.active_temporary_panel_slot(self.focused).is_some() {
                        vec![ContextId::from("workspace.temporary-panel")]
                    } else {
                        vec![ContextId::from("workspace.panel")]
                    }
                },
                Surface::contexts,
            ),
        }
    }

    fn active_editor(&self) -> Option<&EditorSurface> {
        self.active_editor.and_then(|index| self.editors.get(index))
    }

    fn active_editor_mut(&mut self) -> Option<&mut EditorSurface> {
        self.active_editor
            .and_then(|index| self.editors.get_mut(index))
    }

    pub fn handle_terminal_event(
        &mut self,
        keymap: &mut Keymap,
        event: TerminalEvent,
    ) -> WorkspaceAction {
        self.handle_terminal_event_inner(keymap, event, None)
    }

    pub fn handle_terminal_event_at(
        &mut self,
        keymap: &mut Keymap,
        event: TerminalEvent,
        now: Duration,
    ) -> WorkspaceAction {
        self.handle_terminal_event_inner(keymap, event, Some(now))
    }

    fn handle_terminal_event_inner(
        &mut self,
        keymap: &mut Keymap,
        event: TerminalEvent,
        now: Option<Duration>,
    ) -> WorkspaceAction {
        if let TerminalEvent::Mouse(event) = event {
            return self.handle_mouse_event(event, keymap);
        }
        let TerminalEvent::Key(mut stroke) = event else {
            if matches!(event, TerminalEvent::FocusLost) {
                self.held_modifiers = Modifiers::default();
            }
            return self.handle_non_key_surface_event(event, keymap);
        };
        if let Key::Modifier(modifier) = stroke.key {
            self.update_held_modifier(modifier, stroke.kind);
            return WorkspaceAction::Noop;
        }
        if stroke.kind == KeyKind::Release {
            return WorkspaceAction::Noop;
        }
        if self.keyboard_mode == KeyboardMode::Enhanced {
            stroke.modifiers.shift |= self.held_modifiers.shift;
            stroke.modifiers.control |= self.held_modifiers.control;
            stroke.modifiers.alt |= self.held_modifiers.alt;
            stroke.modifiers.super_key |= self.held_modifiers.super_key;
        }
        if matches!(&stroke.key, Key::Character(character) if character.eq_ignore_ascii_case(&'q'))
            && stroke.modifiers.control
            && stroke.modifiers.alt
            && !stroke.modifiers.super_key
        {
            let invocation = CommandInvocation {
                id: CommandId::from("near.app.force-quit"),
                arguments: BTreeMap::new(),
            };
            self.dispatch_with_keymap(&invocation, Some(keymap));
            return WorkspaceAction::Command(invocation);
        }
        if self.overlay.is_some()
            && stroke.key == Key::Escape
            && stroke.modifiers == Modifiers::default()
        {
            let invocation = CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            };
            self.dispatch_with_keymap(&invocation, Some(keymap));
            return WorkspaceAction::Command(invocation);
        }
        if self.overlay.is_some()
            && stroke.modifiers.alt
            && !stroke.modifiers.control
            && !stroke.modifiers.super_key
        {
            let search_event = match stroke.key {
                Key::Character(character) => {
                    Some(SurfaceEvent::SelectionSearchText(character.to_string()))
                }
                Key::Backspace => Some(SurfaceEvent::SelectionSearchBackspace),
                _ => None,
            };
            if let Some(search_event) = search_event {
                self.route_surface_event(&search_event, Some(keymap));
                return WorkspaceAction::Noop;
            }
        }
        if self.overlay.is_none()
            && let Some(invocation) = self.macro_binding_invocation(&stroke)
        {
            self.dispatch_with_keymap(&invocation, Some(keymap));
            return WorkspaceAction::Command(invocation);
        }
        if self.route_panel_lookup_before_shell(&stroke, keymap) {
            return WorkspaceAction::Noop;
        }
        if self.overlay.is_none() && self.handle_active_command_line_key(&stroke) {
            return WorkspaceAction::Noop;
        }
        let surface_stroke = stroke.clone();
        let resolved = match now {
            Some(now) => keymap.resolve_at(&self.active_contexts(), stroke, now),
            None => keymap.resolve(&self.active_contexts(), stroke),
        };
        match resolved {
            ResolveResult::Matched(invocation) => {
                self.pending_sequence.clear();
                self.dispatch_with_keymap(&invocation, Some(keymap));
                WorkspaceAction::Command(invocation)
            }
            ResolveResult::Pending {
                sequence,
                continuations,
            } => {
                self.pending_sequence = keymap
                    .settings()
                    .show_pending_sequence
                    .then(|| {
                        format!(
                            "{} → {}",
                            format_key_sequence(&sequence),
                            format_key_sequence(&continuations)
                        )
                    })
                    .unwrap_or_default();
                WorkspaceAction::PendingSequence(self.pending_sequence.clone())
            }
            ResolveResult::NoMatch => {
                self.pending_sequence.clear();
                if self.overlay.is_none() && self.handle_filename_lookup_key(&surface_stroke) {
                    return WorkspaceAction::Noop;
                }
                self.handle_unmatched_key(&surface_stroke, keymap)
            }
        }
    }

    fn handle_mouse_event(&mut self, event: MouseEvent, keymap: &Keymap) -> WorkspaceAction {
        if matches!(
            event.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) && !matches!(self.overlay, Some(Overlay::Menu(_)))
        {
            return self.handle_mouse_wheel(event, keymap);
        }
        if let Some(invocation) = self.handle_overlay_mouse(event) {
            let parent = self.overlay.take();
            self.dispatch_with_keymap(&invocation, Some(keymap));
            if self.overlay.is_some()
                && let Some(Overlay::Menu(menu)) = parent
                && !is_main_menu_command(&invocation.id)
            {
                self.overlay_history.push(Overlay::Menu(menu));
            }
            return WorkspaceAction::Command(invocation);
        }
        if self.overlay.is_some() {
            return WorkspaceAction::Noop;
        }
        let (columns, rows) = self.viewport.get();
        let panel_height = self.panel_viewport_height(rows);
        if self.settings.interface.show_keybar
            && event.row == rows.saturating_sub(1)
            && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && let Some(invocation) = self.keybar_invocation_at(event.column, keymap)
        {
            self.dispatch_with_keymap(&invocation, Some(keymap));
            return WorkspaceAction::Command(invocation);
        }
        if self.terminal_owns_focus() || self.quick_view_interactive || self.active_editor.is_some()
        {
            return WorkspaceAction::Noop;
        }
        let hit = if self.temporary_panel_is_full_screen(self.focused) {
            full_screen_panel_item_at(self.focused, columns, panel_height, event.column, event.row)
        } else {
            panel_item_at(
                self.panel_layout,
                columns,
                panel_height,
                event.column,
                event.row,
            )
        };
        let Some((panel, item)) = hit else {
            if matches!(event.kind, MouseEventKind::Up(MouseButton::Left)) {
                self.mouse_drag = None;
            }
            return WorkspaceAction::Noop;
        };
        let item = item.and_then(|item| self.panel(panel).item_at_visible_row(item));
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.focused = panel;
                if let Some(item) = item {
                    self.panel_mut(panel).set_cursor(item);
                    self.mouse_drag = Some(MouseDrag {
                        source: panel,
                        move_items: event.modifiers.shift,
                    });
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.focused = panel;
                if let Some(item) = item {
                    self.panel_mut(panel).set_cursor(item);
                    if self.panel(panel).current().is_some_and(is_parent_entry) {
                        self.status = "The parent entry cannot be selected".to_owned();
                    } else {
                        self.panel_mut(panel).toggle_selection();
                        self.status = "Mouse selection toggled".to_owned();
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                self.focused = panel;
                if let Some(item) = item {
                    self.panel_mut(panel).set_cursor(item);
                    let invocation = CommandInvocation {
                        id: CommandId::from("near.resource.open"),
                        arguments: BTreeMap::new(),
                    };
                    self.dispatch_with_keymap(&invocation, Some(keymap));
                    return WorkspaceAction::Command(invocation);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(drag) = &mut self.mouse_drag {
                    drag.move_items = event.modifiers.shift;
                    if drag.source != panel {
                        self.status = format!(
                            "Drag preview: {} current selection to {} panel",
                            if drag.move_items { "move" } else { "copy" },
                            match panel {
                                FocusedPanel::Left => "left",
                                FocusedPanel::Right => "right",
                            }
                        );
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(drag) = self.mouse_drag.take() else {
                    return WorkspaceAction::Noop;
                };
                if drag.source != panel {
                    self.focused = drag.source;
                    let invocation = CommandInvocation {
                        id: CommandId::from(if drag.move_items {
                            "near.resource.move-to-peer"
                        } else {
                            "near.resource.copy-to-peer"
                        }),
                        arguments: BTreeMap::new(),
                    };
                    self.dispatch_with_keymap(&invocation, Some(keymap));
                    return WorkspaceAction::Command(invocation);
                }
            }
            _ => {}
        }
        WorkspaceAction::Noop
    }

    fn handle_overlay_mouse(&mut self, event: MouseEvent) -> Option<CommandInvocation> {
        let (columns, rows) = self.viewport.get();
        let area = Rect::new(0, 0, columns, rows);
        match &mut self.overlay {
            Some(Overlay::Menu(menu)) => {
                if matches!(event.kind, MouseEventKind::ScrollUp) {
                    menu.update(
                        &SurfaceEvent::Command(CommandInvocation {
                            id: CommandId::from("near.menu.up"),
                            arguments: BTreeMap::new(),
                        }),
                        &mut UpdateContext {
                            action: &ActionContext::default(),
                        },
                    );
                    return None;
                }
                if matches!(event.kind, MouseEventKind::ScrollDown) {
                    menu.update(
                        &SurfaceEvent::Command(CommandInvocation {
                            id: CommandId::from("near.menu.down"),
                            arguments: BTreeMap::new(),
                        }),
                        &mut UpdateContext {
                            action: &ActionContext::default(),
                        },
                    );
                    return None;
                }
                if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
                    return None;
                }
                if event.row == area.y
                    && let Some(index) = menu.main_menu_index_at(area.x, event.column)
                {
                    return Some(CommandInvocation {
                        id: CommandId::from(main_menu_command(MainMenuCategory::from_index(index))),
                        arguments: BTreeMap::new(),
                    });
                }
                let popup = menu_popup(area, menu);
                if event.column <= popup.x
                    || event.column >= popup.right().saturating_sub(1)
                    || event.row <= popup.y
                    || event.row >= popup.bottom().saturating_sub(1)
                {
                    return None;
                }
                let row = usize::from(event.row.saturating_sub(popup.y + 1));
                if !menu.select_visible_row(row) {
                    return None;
                }
                let result = menu.update(
                    &SurfaceEvent::Command(CommandInvocation {
                        id: CommandId::from("near.menu.activate"),
                        arguments: BTreeMap::new(),
                    }),
                    &mut UpdateContext {
                        action: &ActionContext::default(),
                    },
                );
                result.command
            }
            _ => None,
        }
    }

    fn handle_mouse_wheel(&mut self, event: MouseEvent, keymap: &Keymap) -> WorkspaceAction {
        let rows = if matches!(event.kind, MouseEventKind::ScrollUp) {
            -3
        } else {
            3
        };
        if self.overlay.is_none()
            && !self.terminal_owns_focus()
            && !self.quick_view_interactive
            && self.active_editor.is_none()
        {
            let (columns, viewport_rows) = self.viewport.get();
            let panel_height = self.panel_viewport_height(viewport_rows);
            let hit = if self.temporary_panel_is_full_screen(self.focused) {
                full_screen_panel_item_at(
                    self.focused,
                    columns,
                    panel_height,
                    event.column,
                    event.row,
                )
            } else {
                panel_item_at(
                    self.panel_layout,
                    columns,
                    panel_height,
                    event.column,
                    event.row,
                )
            };
            if let Some((panel, _)) = hit {
                self.focused = panel;
                self.panel_mut(panel).move_cursor(rows);
            }
            return WorkspaceAction::Noop;
        }
        let command = match self.active_contexts().first().map(ContextId::as_str) {
            Some("surface.viewer") => Some(if rows < 0 {
                "near.viewer.up"
            } else {
                "near.viewer.down"
            }),
            Some("surface.editor") => Some(if rows < 0 {
                "near.editor.up"
            } else {
                "near.editor.down"
            }),
            Some("surface.help") => Some(if rows < 0 {
                "near.help.up"
            } else {
                "near.help.down"
            }),
            _ => None,
        };
        let Some(command) = command else {
            return WorkspaceAction::Noop;
        };
        let invocation = CommandInvocation {
            id: CommandId::from(command),
            arguments: BTreeMap::new(),
        };
        self.dispatch_with_keymap(&invocation, Some(keymap));
        WorkspaceAction::Command(invocation)
    }

    fn keybar_invocation_at(&self, column: u16, keymap: &Keymap) -> Option<CommandInvocation> {
        if self.terminal_is_full_screen() || self.terminal_pane().is_some() {
            return self.terminal_keybar_invocation_at(column, keymap);
        }
        let modifiers = self.keybar_modifiers();
        let mut start = 0_u16;
        for (slot, binding) in
            keymap.function_hints_for_modifiers(&self.active_contexts(), modifiers)
        {
            let width = u16::try_from(
                format!("{slot}{} ", short_description(binding))
                    .chars()
                    .count(),
            )
            .unwrap_or(u16::MAX);
            if column >= start && column < start.saturating_add(width) {
                return Some(binding.invocation.clone());
            }
            start = start.saturating_add(width);
        }
        None
    }

    fn update_held_modifier(&mut self, modifier: ModifierKey, kind: KeyKind) {
        if self.keyboard_mode != KeyboardMode::Enhanced {
            self.held_modifiers = Modifiers::default();
            return;
        }
        let held = kind != KeyKind::Release;
        match modifier {
            ModifierKey::Shift => self.held_modifiers.shift = held,
            ModifierKey::Control => self.held_modifiers.control = held,
            ModifierKey::Alt => self.held_modifiers.alt = held,
            ModifierKey::Super => self.held_modifiers.super_key = held,
            ModifierKey::Other => {}
        }
    }

    fn keybar_modifiers(&self) -> Modifiers {
        (self.keyboard_mode == KeyboardMode::Enhanced)
            .then_some(self.held_modifiers)
            .unwrap_or_default()
    }

    fn handle_active_command_line_key(&mut self, stroke: &KeyStroke) -> bool {
        if self.terminal_owns_focus() || !self.command_line.is_active() {
            return false;
        }
        #[cfg(feature = "embedded-pty")]
        if let Some(handled) = self.handle_native_shell_dock_key(stroke) {
            return handled;
        }
        if let Some(character) = stroke.text_character() {
            self.command_line.insert(&character.to_string());
            return true;
        }
        if stroke.modifiers != Modifiers::default() {
            return false;
        }
        match stroke.key {
            Key::Enter => self.submit_command_line(),
            Key::Backspace => {
                self.command_line.backspace();
            }
            Key::Escape => {
                self.command_line.clear();
                "Command line cleared".clone_into(&mut self.status);
            }
            Key::Up => {
                self.command_line.previous();
            }
            Key::Down => {
                self.command_line.next();
            }
            Key::Tab => {
                if self.settings.interface.command_line_completion {
                    self.complete_command_line();
                }
            }
            _ => return false,
        }
        true
    }

    fn complete_command_line(&mut self) {
        let buffer = self.command_line.buffer().to_owned();
        let (head, prefix) = buffer.rfind(' ').map_or(("", buffer.as_str()), |index| {
            (&buffer[..=index], &buffer[index + 1..])
        });
        let mut candidates = self
            .command_line
            .entries()
            .iter()
            .map(|entry| entry.command.as_str())
            .chain(
                self.focused_panel()
                    .entries()
                    .iter()
                    .map(|entry| entry.metadata.name.as_str()),
            )
            .filter(|candidate| candidate.starts_with(prefix) && *candidate != prefix)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        if let Some(candidate) = candidates.first() {
            self.command_line.set_buffer(format!("{head}{candidate}"));
            self.status = format!(
                "Completed command line from {} candidates",
                candidates.len()
            );
        } else {
            "No command-line completion".clone_into(&mut self.status);
        }
    }

    fn handle_filename_lookup_key(&mut self, stroke: &KeyStroke) -> bool {
        if self.filename_lookup.is_some()
            && let Key::Character(character) = stroke.key
            && stroke.modifiers.alt
            && !stroke.modifiers.control
            && !stroke.modifiers.super_key
        {
            let text = character.to_string();
            if self.filename_lookup.as_ref().is_some_and(|lookup| {
                lookup.query.chars().count() == 1 && lookup.query.eq_ignore_ascii_case(&text)
            }) {
                self.cycle_filename_lookup(1);
            } else if self.filename_lookup.is_some() {
                self.extend_filename_lookup(character);
            } else {
                self.start_filename_lookup(&text);
            }
            return true;
        }

        if self.filename_lookup.is_none() {
            return false;
        }
        if stroke.modifiers != Modifiers::default() {
            return false;
        }
        match stroke.key {
            Key::Character(character) => {
                self.extend_filename_lookup(character);
            }
            Key::Backspace => self.backspace_filename_lookup(),
            Key::Up => self.cycle_filename_lookup(-1),
            Key::Down => self.cycle_filename_lookup(1),
            Key::Enter => self.accept_filename_lookup(),
            Key::Escape => self.cancel_filename_lookup(),
            _ => return false,
        }
        true
    }

    fn start_filename_lookup(&mut self, query: &str) {
        let panel = self.focused;
        let original_cursor = self.focused_panel().cursor();
        self.filename_lookup = Some(FilenameLookup {
            panel,
            original_cursor,
            query: query.to_owned(),
            mode: CollectionLookupMode::Prefix,
            matches: Vec::new(),
            active_match: 0,
        });
        self.refresh_filename_lookup();
    }

    fn extend_filename_lookup(&mut self, character: char) {
        if let Some(lookup) = &mut self.filename_lookup {
            lookup.query.push(character);
        }
        self.refresh_filename_lookup();
    }

    fn paste_filename_lookup(&mut self, text: &str) {
        if let Some(lookup) = &mut self.filename_lookup {
            lookup.query.push_str(text);
        }
        self.refresh_filename_lookup();
    }

    fn backspace_filename_lookup(&mut self) {
        let empty = self.filename_lookup.as_mut().is_none_or(|lookup| {
            lookup.query.pop();
            lookup.query.is_empty()
        });
        if empty {
            self.cancel_filename_lookup();
        } else {
            self.refresh_filename_lookup();
        }
    }

    fn cycle_filename_lookup(&mut self, direction: isize) {
        if self
            .filename_lookup
            .as_ref()
            .is_some_and(|lookup| lookup.matches.len() <= 1)
        {
            self.dismiss_filename_lookup();
            return;
        }
        let Some(lookup) = &mut self.filename_lookup else {
            return;
        };
        lookup.active_match = if direction < 0 {
            (lookup.active_match + lookup.matches.len() - 1) % lookup.matches.len()
        } else {
            (lookup.active_match + 1) % lookup.matches.len()
        };
        self.apply_filename_lookup_cursor();
    }

    fn dismiss_filename_lookup(&mut self) {
        if let Some(lookup) = self.filename_lookup.take() {
            self.panel_mut(lookup.panel).set_lookup_query(None);
        }
    }

    fn apply_filename_lookup_cursor(&mut self) {
        let target = self.filename_lookup.as_ref().and_then(|lookup| {
            lookup
                .matches
                .get(lookup.active_match)
                .copied()
                .map(|cursor| (lookup.panel, cursor))
        });
        if let Some((panel, cursor)) = target {
            self.panel_mut(panel).set_cursor(cursor);
            self.refresh_quick_view();
        }
    }

    fn accept_filename_lookup(&mut self) {
        if let Some(lookup) = self.filename_lookup.take() {
            self.panel_mut(lookup.panel).set_lookup_query(None);
        }
    }

    fn cancel_filename_lookup(&mut self) {
        if let Some(lookup) = self.filename_lookup.take() {
            self.panel_mut(lookup.panel).set_lookup_query(None);
            self.panel_mut(lookup.panel)
                .set_cursor(lookup.original_cursor);
            self.refresh_quick_view();
        }
    }

    fn handle_non_key_surface_event(
        &mut self,
        event: TerminalEvent,
        keymap: &Keymap,
    ) -> WorkspaceAction {
        let surface_event = match event {
            TerminalEvent::Paste(text) if self.filename_lookup.is_some() => {
                self.paste_filename_lookup(&text);
                return WorkspaceAction::Noop;
            }
            TerminalEvent::Paste(text)
                if self.overlay.is_none()
                    && self.terminal_presentation == ZoomablePanePresentation::Base
                    && !self.quick_view_interactive
                    && self.active_editor.is_none() =>
            {
                self.paste_command_text(&text);
                return WorkspaceAction::Noop;
            }
            TerminalEvent::Paste(text) => SurfaceEvent::Paste(text),
            TerminalEvent::FocusGained => SurfaceEvent::FocusGained,
            TerminalEvent::FocusLost => SurfaceEvent::FocusLost,
            _ => return WorkspaceAction::Noop,
        };
        self.route_surface_event(&surface_event, Some(keymap));
        WorkspaceAction::Noop
    }

    fn handle_unmatched_key(&mut self, stroke: &KeyStroke, keymap: &Keymap) -> WorkspaceAction {
        if self.overlay.is_none()
            && !self.terminal_owns_focus()
            && !self.quick_view_interactive
            && self.active_editor.is_none()
            && let Key::Character(character) = stroke.key
            && stroke.modifiers.alt
            && !stroke.modifiers.control
            && !stroke.modifiers.super_key
        {
            self.start_filename_lookup(&character.to_string());
            return WorkspaceAction::Noop;
        }
        if self.overlay.is_none()
            && self.terminal_presentation == ZoomablePanePresentation::Base
            && !self.quick_view_interactive
            && self.active_editor.is_none()
            && let Some(character) = stroke.text_character()
        {
            self.insert_command_text(&character.to_string());
            return WorkspaceAction::Noop;
        }
        let event = match stroke.key {
            Key::Character(character)
                if !stroke.modifiers.control
                    && !stroke.modifiers.alt
                    && !stroke.modifiers.super_key =>
            {
                SurfaceEvent::Text(character.to_string())
            }
            Key::Backspace => SurfaceEvent::Backspace,
            _ => return WorkspaceAction::Noop,
        };
        self.route_surface_event(&event, Some(keymap));
        WorkspaceAction::Noop
    }

    fn route_surface_event(&mut self, event: &SurfaceEvent, keymap: Option<&Keymap>) {
        let action = self.action_context();
        let interface = self.settings.interface;
        let terminal_owns_focus = self.terminal_owns_focus();
        let result = match &mut self.overlay {
            Some(Overlay::Surface(surface)) => {
                surface.configure_interaction(
                    interface.menu_wrap_navigation,
                    interface.dialog_wrap_focus,
                );
                surface.update(event, &mut UpdateContext { action: &action })
            }
            Some(Overlay::Menu(menu)) => {
                menu.configure_interaction(
                    interface.menu_wrap_navigation,
                    interface.dialog_wrap_focus,
                );
                menu.update(event, &mut UpdateContext { action: &action })
            }
            Some(Overlay::CommandHistory(surface)) => {
                surface.update(event, &mut UpdateContext { action: &action })
            }
            Some(Overlay::FolderHistory(surface)) => {
                surface.update(event, &mut UpdateContext { action: &action })
            }
            Some(Overlay::ResourceHistory(surface)) => {
                surface.update(event, &mut UpdateContext { action: &action })
            }
            Some(Overlay::CommandPalette {
                selected,
                entries,
                search,
            }) => {
                match event {
                    SurfaceEvent::Text(text)
                    | SurfaceEvent::Paste(text)
                    | SurfaceEvent::SelectionSearchText(text) => search.push(text),
                    SurfaceEvent::Backspace | SurfaceEvent::SelectionSearchBackspace => {
                        search.pop();
                    }
                    _ => return,
                }
                let visible = palette_visible_indices(entries, search);
                *selected = visible.first().copied().unwrap_or_default();
                UpdateResult::handled()
            }
            None if terminal_owns_focus => {
                let Some(surface) = self.active_terminal_surface_mut() else {
                    return;
                };
                surface.update(event, &mut UpdateContext { action: &action })
            }
            None if self.quick_view_interactive => {
                let Some(viewer) = self.quick_view.as_mut() else {
                    return;
                };
                viewer.update(event, &mut UpdateContext { action: &action })
            }
            None => {
                let Some(editor) = self.active_editor_mut() else {
                    return;
                };
                editor.update(event, &mut UpdateContext { action: &action })
            }
            _ => return,
        };
        if let Some(command) = result.command {
            self.dispatch_surface_command(&command, keymap);
        }
    }

    fn dispatch_surface_command(&mut self, command: &CommandInvocation, keymap: Option<&Keymap>) {
        let preserve_settings = command.id.as_str() == "near.settings.edit-value"
            && matches!(
                self.overlay.as_ref(),
                Some(Overlay::Surface(surface)) if surface.id().as_str() == "near-fm.settings"
            );
        let parent = preserve_settings.then(|| self.overlay.take()).flatten();
        self.dispatch_with_keymap(command, keymap);
        if let Some(parent) = parent {
            if self.overlay.is_some() {
                self.overlay_history.push(parent);
            } else {
                self.overlay = Some(parent);
            }
        }
    }

    pub fn handle_keymap_timeout(&mut self, keymap: &mut Keymap) -> WorkspaceAction {
        self.handle_keymap_timeout_result(keymap.expire_pending(), keymap)
    }

    pub fn handle_keymap_timeout_at(
        &mut self,
        keymap: &mut Keymap,
        now: Duration,
    ) -> WorkspaceAction {
        self.handle_keymap_timeout_result(keymap.expire_pending_at(now), keymap)
    }

    fn handle_keymap_timeout_result(
        &mut self,
        result: ResolveResult,
        keymap: &Keymap,
    ) -> WorkspaceAction {
        match result {
            ResolveResult::Matched(invocation) => {
                self.pending_sequence.clear();
                self.dispatch_with_keymap(&invocation, Some(keymap));
                WorkspaceAction::Command(invocation)
            }
            ResolveResult::NoMatch | ResolveResult::Pending { .. } => {
                self.pending_sequence.clear();
                WorkspaceAction::Noop
            }
        }
    }

    pub fn dispatch(&mut self, invocation: &CommandInvocation) {
        self.dispatch_with_keymap(invocation, None);
    }

    #[allow(clippy::too_many_lines)]
    fn dispatch_with_keymap(&mut self, invocation: &CommandInvocation, keymap: Option<&Keymap>) {
        let parent = self.diagnostics.active;
        let correlation = self.diagnostics.journal.begin(
            DiagnosticDomain::Command,
            invocation.id.as_str(),
            parent,
        );
        self.diagnostics.active = Some(correlation);
        self.dispatch_checked(invocation, keymap);
        if self.overlay.is_none() && invocation.id.as_str() != "near.overlay.cancel" {
            self.overlay_history.clear();
        }
        self.diagnostics.journal.record(
            correlation,
            parent,
            DiagnosticDomain::Command,
            DiagnosticPhase::Completed,
            invocation.id.as_str(),
            BTreeMap::new(),
        );
        self.diagnostics.active = parent;
    }

    #[allow(clippy::too_many_lines)]
    fn dispatch_checked(&mut self, invocation: &CommandInvocation, keymap: Option<&Keymap>) {
        let action = self.action_context();
        if let Err(error) = self.registry.check(invocation, &action) {
            self.status = error.to_string();
            return;
        }
        self.record_macro_invocation(invocation);
        if invocation.id.as_str().starts_with("near.command-history.")
            && let Some(Overlay::CommandHistory(surface)) = &mut self.overlay
        {
            let result = surface.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            if result.handled {
                if let Some(command) = result.command {
                    self.dispatch_with_keymap(&command, keymap);
                }
                return;
            }
        }
        if invocation.id.as_str().starts_with("near.folder-history.")
            && let Some(Overlay::FolderHistory(surface)) = &mut self.overlay
        {
            let result = surface.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            if result.handled {
                if let Some(command) = result.command {
                    self.dispatch_with_keymap(&command, keymap);
                }
                return;
            }
        }
        if invocation.id.as_str().starts_with("near.resource-history.")
            && matches!(
                invocation.id.as_str(),
                "near.resource-history.up"
                    | "near.resource-history.down"
                    | "near.resource-history.open"
                    | "near.resource-history.toggle-lock"
                    | "near.resource-history.clear-unlocked"
            )
            && let Some(Overlay::ResourceHistory(surface)) = &mut self.overlay
        {
            let result = surface.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            if result.handled {
                if let Some(command) = result.command {
                    self.dispatch_with_keymap(&command, keymap);
                }
                return;
            }
        }
        if !matches!(
            invocation.id.as_str(),
            "near.overlay.cancel" | "near.app.quit" | "near.app.force-quit"
        ) && let Some(Overlay::Surface(surface)) = &mut self.overlay
        {
            surface.configure_interaction(
                self.settings.interface.menu_wrap_navigation,
                self.settings.interface.dialog_wrap_focus,
            );
            let result = surface.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            let viewer_state = surface.viewer_state();
            if result.handled {
                if let Some(state) = viewer_state {
                    self.record_viewer_state(state);
                }
                if let Some(command) = result.command {
                    self.dispatch_surface_command(&command, keymap);
                }
                return;
            }
        }
        if self.overlay.is_none()
            && self.terminal_owns_focus()
            && !matches!(
                invocation.id.as_str(),
                "near.terminal.open"
                    | "near.terminal.close"
                    | "near.app.quit"
                    | "near.app.force-quit"
            )
            && let Some(surface) = self.active_terminal_surface_mut()
        {
            let result = surface.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            if result.handled {
                if let Some(command) = result.command {
                    self.dispatch_with_keymap(&command, keymap);
                }
                return;
            }
        }
        if self.overlay.is_none()
            && !self.terminal_owns_focus()
            && self.quick_view_interactive
            && !matches!(
                invocation.id.as_str(),
                "near.panel.quick-view-control"
                    | "near.overlay.cancel"
                    | "near.app.quit"
                    | "near.app.force-quit"
            )
            && let Some(viewer) = &mut self.quick_view
        {
            let result = viewer.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            if result.handled {
                if let Some(command) = result.command {
                    self.dispatch_with_keymap(&command, keymap);
                }
                return;
            }
        }
        if self.overlay.is_none()
            && !self.terminal_owns_focus()
            && let Some(editor) = self.active_editor_mut()
        {
            let result = editor.update(
                &SurfaceEvent::Command(invocation.clone()),
                &mut UpdateContext { action: &action },
            );
            if result.handled {
                if let Some(command) = result.command {
                    self.dispatch_with_keymap(&command, keymap);
                }
                return;
            }
        }
        match invocation.id.as_str() {
            "near.help.context" | "near.help.contents" | "near.help.extensions" => {
                let start = match invocation.id.as_str() {
                    "near.help.contents" => "contents",
                    "near.help.extensions" => "extensions",
                    _ => "context",
                };
                self.overlay = Some(Overlay::Surface(Box::new(
                    self.effective_help_surface(keymap, start),
                )));
            }
            "near.workspace.focus-peer" => self.focus_peer(),
            "near.workspace.swap-peers" => {
                mem::swap(&mut self.left, &mut self.right);
                mem::swap(&mut self.left_panel_type, &mut self.right_panel_type);
                if let Some((panel, _)) = &mut self.quick_view_replaced {
                    *panel = opposite_panel(*panel);
                }
                mem::swap(
                    &mut self.left_saved_selection,
                    &mut self.right_saved_selection,
                );
                "Swapped peer panels".clone_into(&mut self.status);
                self.refresh_quick_view();
            }
            "near.workspace.resize-panels" => {
                let columns = invocation
                    .arguments
                    .get("columns")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|value| isize::try_from(value).ok())
                    .unwrap_or(0);
                let rows = invocation
                    .arguments
                    .get("rows")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|value| isize::try_from(value).ok())
                    .unwrap_or(0);
                let (width, height) = self.viewport.get();
                self.panel_layout.resize_columns(width, columns, 8);
                self.panel_layout
                    .resize_rows(height.saturating_sub(3), rows, 3);
                "Panel layout resized".clone_into(&mut self.status);
            }
            "near.workspace.reset-panel-layout" => {
                self.panel_layout.reset();
                "Panel layout reset".clone_into(&mut self.status);
            }
            "near.collection.move" => {
                let rows = invocation
                    .arguments
                    .get("rows")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|value| isize::try_from(value).ok())
                    .unwrap_or(0);
                self.focused_panel_mut().move_cursor(rows);
                self.refresh_quick_view();
            }
            "near.collection.first" => {
                self.focused_panel_mut().first();
                self.refresh_quick_view();
            }
            "near.collection.last" => {
                self.focused_panel_mut().last();
                self.refresh_quick_view();
            }
            "near.collection.page" => {
                let pages = invocation
                    .arguments
                    .get("pages")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|value| isize::try_from(value).ok())
                    .unwrap_or(0);
                self.focused_panel_mut().page_cursor(pages);
                self.refresh_quick_view();
            }
            "near.collection.scroll-horizontal" => {
                let columns = invocation
                    .arguments
                    .get("columns")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|value| isize::try_from(value).ok())
                    .unwrap_or(0);
                self.focused_panel_mut().scroll_horizontal(columns);
            }
            "near.collection.horizontal-start" => self.focused_panel_mut().horizontal_start(),
            "near.collection.horizontal-end" => self.focused_panel_mut().horizontal_end(),
            "near.collection.toggle-selection" => {
                if self.focused_panel().current().is_some_and(is_parent_entry) {
                    "The parent entry cannot be selected".clone_into(&mut self.status);
                } else {
                    self.focused_panel_mut().toggle_selection();
                }
            }
            "near.collection.toggle-selection-move" => {
                let rows = invocation
                    .arguments
                    .get("rows")
                    .and_then(near_core::CommandValue::as_i64)
                    .and_then(|value| isize::try_from(value).ok())
                    .unwrap_or(0);
                self.focused_panel_mut().toggle_selection_and_move(rows);
                self.refresh_quick_view();
            }
            "near.selection.select-mask" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::selection_mask_dialog(
                    true,
                ))));
            }
            "near.selection.unselect-mask" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::selection_mask_dialog(
                    false,
                ))));
            }
            "near.selection.mask-confirmed" => {
                let include = invocation
                    .arguments
                    .get("include")
                    .and_then(near_core::CommandValue::as_str)
                    .unwrap_or("*");
                let exclude = invocation
                    .arguments
                    .get("exclude")
                    .and_then(near_core::CommandValue::as_str)
                    .unwrap_or_default();
                let selected = invocation
                    .arguments
                    .get("selected")
                    .and_then(|value| match value {
                        near_core::CommandValue::Boolean(value) => Some(*value),
                        _ => None,
                    })
                    .unwrap_or(true);
                self.overlay = None;
                let changed = self
                    .focused_panel_mut()
                    .select_by_masks(include, exclude, selected);
                self.status = format!(
                    "{} {changed} item(s) by mask",
                    if selected { "Selected" } else { "Unselected" }
                );
            }
            "near.selection.same-extension" => {
                let changed = self.focused_panel_mut().select_same_extension();
                self.status = format!("Selected {changed} item(s) with the current extension");
            }
            "near.selection.same-name" => {
                let changed = self.focused_panel_mut().select_same_name();
                self.status = format!("Selected {changed} item(s) with the current name");
            }
            "near.selection.invert" => {
                let changed = self.focused_panel_mut().invert_selection();
                self.status = format!("Inverted selection for {changed} item(s)");
            }
            "near.selection.save" => self.save_panel_selection(),
            "near.selection.restore" => self.restore_panel_selection(),
            "near.selection.compare-folders" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::folder_comparison_dialog())));
            }
            "near.selection.compare-folders-confirmed" => {
                self.compare_panel_folders(invocation);
            }
            "near.panel.view-mode.menu" => {
                self.overlay = Some(Overlay::Menu(self.panel_mode_menu()));
            }
            "near.panel.view-mode.set" => self.set_panel_view_mode(invocation),
            "near.collection.sort.unsorted" => self.set_sort_mode(SortMode::Unsorted),
            "near.collection.sort.name" => self.set_sort_mode(SortMode::Name),
            "near.collection.sort.extension" => self.set_sort_mode(SortMode::Extension),
            "near.collection.sort.modified" => self.set_sort_mode(SortMode::Modified),
            "near.collection.sort.size" => self.set_sort_mode(SortMode::Size),
            "near.collection.sort.created" => self.set_sort_mode(SortMode::Created),
            "near.collection.sort.accessed" => self.set_sort_mode(SortMode::Accessed),
            "near.collection.sort.kind" => self.set_sort_mode(SortMode::Kind),
            "near.collection.sort.owner" => self.set_sort_mode(SortMode::Owner),
            "near.collection.sort.permissions" => self.set_sort_mode(SortMode::Permissions),
            "near.collection.sort.toggle-reverse" => {
                self.focused_panel_mut().toggle_reverse_sort();
                self.report_sort_state();
            }
            "near.collection.sort.toggle-numeric" => {
                self.focused_panel_mut().toggle_numeric_sort();
                self.report_sort_state();
            }
            "near.collection.sort.toggle-selected-first" => {
                self.focused_panel_mut().toggle_selected_first();
                self.report_sort_state();
            }
            "near.collection.sort.toggle-directories-first" => {
                self.focused_panel_mut().toggle_directories_first();
                self.report_sort_state();
            }
            "near.collection.sort.toggle-groups" => {
                self.focused_panel_mut().toggle_sort_groups();
                self.report_sort_state();
            }
            "near.highlighting.report" => {
                self.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
                    "near-fm.highlighting-report",
                    "Highlighting and sort groups",
                    self.focused_panel().highlighting_report(),
                ))));
            }
            "near.resource.open" => self.open_current(),
            "near.resource.view" => self.open_view_by_policy(),
            "near.resource.edit" => self.open_editor_by_policy(),
            "near.resource.edit-external" => self.request_external_tool(ExternalAction::Edit),
            "near.resource.execute-external" => {
                self.request_external_tool(ExternalAction::Execute);
            }
            "near.resource.associations" => self.show_association_menu(),
            "near.resource.association-run" => self.run_association(invocation),
            "near.resource.description" => self.show_description_dialog(),
            "near.resource.description-confirmed" => self.update_descriptions(invocation),
            "near.folder-description.view" => self.open_folder_description(false),
            "near.folder-description.edit" => self.open_folder_description(true),
            "near.user-menu.global" => self.show_user_menu(UserMenuScope::Global),
            "near.user-menu.local" => self.show_user_menu(UserMenuScope::Local),
            "near.user-menu.run" => self.run_user_menu(invocation),
            "near.editor.close-confirmed" => self.close_active_editor(),
            "near.editor.save-as" => self.show_editor_save_as_dialog(),
            "near.editor.save-as-confirmed" => self.confirm_editor_save_as(invocation),
            "near.editor.external-change" => self.show_editor_external_change_menu(),
            "near.editor.external-reload" => self.reload_external_editor(),
            "near.editor.external-compare" => self.compare_external_editor(),
            "near.editor.external-keep-local" => self.keep_local_editor(),
            "near.editor.lossy-save-warning" => self.show_lossy_save_warning(),
            "near.editor.lossy-save-confirmed" => self.confirm_lossy_editor_save(),
            "near.screen.list" => self.show_screen_list(),
            "near.screen.panels" => {
                self.persist_active_editor_position();
                self.active_editor = None;
                self.terminal_presentation.hide();
                self.suspended_overlay = None;
                self.overlay = None;
                "Panels screen".clone_into(&mut self.status);
            }
            "near.screen.editor" => self.activate_editor_screen(invocation),
            "near.screen.terminal" => self.activate_terminal_tab(invocation),
            "near.screen.next" => self.cycle_editor_screen(1),
            "near.screen.previous" => self.cycle_editor_screen(-1),
            "near.command-line.history-previous" => {
                if !self.command_line.previous() {
                    "Command history is empty".clone_into(&mut self.status);
                }
            }
            "near.command-line.history-next" => {
                self.command_line.next();
            }
            "near.command-line.history-show" => self.show_command_history(),
            "near.history.menu" => self.show_resource_history_menu(),
            "near.history.viewed-show" => {
                self.show_resource_history(ResourceHistoryKind::Viewed);
            }
            "near.history.edited-show" => {
                self.show_resource_history(ResourceHistoryKind::Edited);
            }
            "near.resource-history.open-selected" => self.open_resource_history_entry(invocation),
            "near.resource-history.toggle-lock-selected" => {
                self.toggle_resource_history_lock(invocation);
            }
            "near.resource-history.clear" => self.clear_resource_history(invocation),
            "near.command-line.history-use" => {
                if let Some(command) = invocation
                    .arguments
                    .get("command")
                    .and_then(near_core::CommandValue::as_str)
                {
                    self.replace_command_text(command);
                    self.overlay = None;
                    "Command restored from history".clone_into(&mut self.status);
                }
            }
            "near.command-line.history-toggle-lock" => {
                if let Some(command) = invocation
                    .arguments
                    .get("command")
                    .and_then(near_core::CommandValue::as_str)
                    && let Some(locked) = self.command_line.toggle_lock(command)
                {
                    if let Some(Overlay::CommandHistory(surface)) = &mut self.overlay {
                        surface.set_locked(command, locked);
                    }
                    self.persist_command_history();
                    self.status = format!(
                        "History entry {}",
                        if locked { "locked" } else { "unlocked" }
                    );
                }
            }
            "near.command-line.history-clear-unlocked" => {
                let removed = self.command_line.clear_unlocked_history();
                self.persist_command_history();
                self.show_command_history();
                self.status = format!("Cleared {removed} unlocked command history entries");
            }
            "near.command-line.insert-current" => self.insert_current_name(false),
            "near.command-line.insert-peer" => self.insert_current_name(true),
            "near.command-line.insert-selected" => self.insert_selected_names(),
            "near.command-line.insert-focused-path" => self.insert_panel_path(false),
            "near.command-line.insert-peer-path" => self.insert_panel_path(true),
            "near.command-line.insert-current-path" => self.insert_current_path(false),
            "near.command-line.insert-peer-current-path" => self.insert_current_path(true),
            "near.location.history-show" => self.show_folder_history(),
            "near.location.history-open" => self.open_folder_history_entry(invocation),
            "near.location.history-toggle-lock" => {
                self.toggle_folder_history_lock(invocation);
            }
            "near.location.history-clear" => {
                let before = self.folder_navigation.history.len();
                self.folder_navigation.history.retain(|entry| entry.locked);
                let removed = before.saturating_sub(self.folder_navigation.history.len());
                self.persist_folder_navigation();
                self.show_folder_history();
                self.status = format!("Cleared {removed} folder history entries");
            }
            "near.location.shortcut-assign" => self.assign_folder_shortcut(invocation),
            "near.location.shortcut-open" => self.open_folder_shortcut(invocation),
            "near.panel.toggle-quick-view" => self.toggle_quick_view(),
            "near.panel.quick-view-control" => self.toggle_quick_view_control(),
            "near.panel.toggle-tree" => self.toggle_focused_panel_type(PanelType::Tree),
            "near.panel.toggle-information" => {
                self.toggle_focused_panel_type(PanelType::Information);
            }
            "near.resource.copy-to-peer" => {
                if !self.add_references_to_peer_temporary_panel(
                    CollectionTargetScope::SelectionOrCurrent,
                ) {
                    self.plan_peer_operation(false, CollectionTargetScope::SelectionOrCurrent);
                }
            }
            "near.resource.copy-current-to-peer" => {
                if !self.add_references_to_peer_temporary_panel(CollectionTargetScope::CurrentOnly)
                {
                    self.plan_peer_operation(false, CollectionTargetScope::CurrentOnly);
                }
            }
            "near.resource.move-to-peer" => {
                self.plan_peer_operation(true, CollectionTargetScope::SelectionOrCurrent);
            }
            "near.resource.move-current-to-peer" => {
                self.plan_peer_operation(true, CollectionTargetScope::CurrentOnly);
            }
            "near.resource.rename" => {
                if let Some(dialog) = self.rename_dialog() {
                    self.overlay = Some(Overlay::Surface(Box::new(dialog)));
                }
            }
            "near.resource.rename-confirmed" => self.confirm_rename(invocation),
            "near.resource.link" => {
                if let Some(dialog) = self.link_dialog() {
                    self.overlay = Some(Overlay::Surface(Box::new(dialog)));
                }
            }
            "near.resource.link-confirmed" => self.confirm_link(invocation),
            "near.resource.attributes" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::attributes_dialog())));
            }
            "near.resource.attributes-confirmed" => self.confirm_attributes(invocation),
            "near.operation.apply-command" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::apply_command_dialog())));
            }
            "near.operation.apply-command-confirmed" => self.start_apply_command(invocation),
            "near.search.start"
            | "near.search.confirmed"
            | "near.search.cancel"
            | "near.search.reveal"
            | "near.search.keep-panel"
            | "near.search.panels"
            | "near.search.open-panel" => self.dispatch_search_command(invocation),
            "near.temp-panel.open" => self.open_temporary_panel(invocation),
            "near.temp-panel.list" => self.show_temporary_panels(invocation),
            "near.temp-panel.remove" => self.remove_from_temporary_panel(),
            "near.temp-panel.clear" => self.clear_temporary_panels(false),
            "near.temp-panel.clear-all" => self.clear_temporary_panels(true),
            "near.temp-panel.reveal" => self.reveal_temporary_panel_resource(),
            "near.temp-panel.import" => self.show_temporary_panel_import_dialog(),
            "near.temp-panel.import-confirmed" => self.import_temporary_panel(invocation),
            "near.temp-panel.export" => self.show_temporary_panel_export_dialog(),
            "near.temp-panel.export-confirmed" => self.export_temporary_panel(invocation),
            "near.temp-panel.safe-toggle" => self.toggle_temporary_panel_safe_mode(),
            "near.temp-panel.refresh" => {
                self.refresh_temporary_panel(self.focused);
            }
            "near.temp-panel.menu-select" => self.activate_temporary_panel_menu_item(invocation),
            "near.handler.diagnostics" => self.show_handler_diagnostics(),
            "near.command-prefixes.show" => self.show_command_prefixes(),
            "near.filters.show" => self.show_filter_menu(),
            "near.filters.toggle" => self.toggle_filter(invocation),
            "near.filters.clear" => self.clear_filters(),
            "near.macro.record-toggle" => self.toggle_macro_recording(),
            "near.macro.play-last" => self.play_last_macro(),
            "near.macro.show-last" => self.show_last_macro(),
            "near.macro.manage" => self.show_macro_manager(),
            "near.macro.actions" => self.show_macro_actions(invocation),
            "near.macro.play" => self.play_macro(invocation),
            "near.macro.edit" => self.show_macro_edit_dialog(invocation),
            "near.macro.edit-confirmed" => self.confirm_macro_edit(invocation),
            "near.macro.bind" => self.show_macro_bind_dialog(invocation),
            "near.macro.bind-confirmed" => self.confirm_macro_bind(invocation),
            "near.macro.delete" => self.show_macro_delete_dialog(invocation),
            "near.macro.delete-confirmed" => self.confirm_macro_delete(invocation),
            "near.macro.diagnose" => self.diagnose_macro(invocation),
            "near.fs.create-directory" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::create_directory_dialog())));
            }
            "near.fs.create-directory.confirmed" => {
                let name = invocation
                    .arguments
                    .get("name")
                    .and_then(near_core::CommandValue::as_str)
                    .unwrap_or("unnamed");
                self.overlay = None;
                self.plan_operation(OperationIntent::CreateDirectory {
                    parent: self.focused_panel().location().clone(),
                    name: name.to_owned(),
                });
            }
            "near.archive.create" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::create_archive_dialog())));
            }
            "near.archive.create-confirmed" => self.create_archive(invocation),
            "near.resource.trash" => {
                let sources = self
                    .focused_panel()
                    .target_resources(CollectionTargetScope::SelectionOrCurrent);
                if sources.is_empty() {
                    "The parent entry is navigation-only".clone_into(&mut self.status);
                } else if self.preflight_mutation(&sources, MutationKind::Trash) {
                    self.plan_operation(OperationIntent::Trash { sources });
                }
            }
            "near.resource.trash-current" => {
                let sources = self
                    .focused_panel()
                    .target_resources(CollectionTargetScope::CurrentOnly);
                if sources.is_empty() {
                    "The parent entry is navigation-only".clone_into(&mut self.status);
                } else if self.preflight_mutation(&sources, MutationKind::Trash) {
                    self.plan_operation(OperationIntent::Trash { sources });
                }
            }
            "near.resource.restore-last-trash" => self.restore_last_trash(),
            "near.resource.delete" => {
                let sources = self.canonical_targets();
                if sources.is_empty() {
                    "The parent entry is navigation-only".clone_into(&mut self.status);
                } else if self.preflight_mutation(&sources, MutationKind::Delete) {
                    self.plan_operation(OperationIntent::Delete {
                        sources,
                        recursive: true,
                    });
                }
            }
            "near.resource.wipe" => {
                let sources = self.canonical_targets();
                if sources.is_empty() {
                    "The parent entry is navigation-only".clone_into(&mut self.status);
                } else if self.preflight_mutation(&sources, MutationKind::Wipe) {
                    self.overlay = Some(Overlay::Surface(Box::new(Self::wipe_dialog())));
                }
            }
            "near.resource.wipe-confirmed" => self.confirm_wipe(invocation),
            _ if self.extension_settings_open.contains_key(&invocation.id) => {
                self.show_extension_settings(&invocation.id);
            }
            _ if self.extension_settings_save.contains_key(&invocation.id) => {
                self.save_extension_settings(invocation);
            }
            _ if self.extension_commands.contains_key(&invocation.id) => {
                self.dispatch_extension_command(invocation);
            }
            _ => self.dispatch_host_command(invocation, keymap),
        }
    }

    fn dispatch_extension_command(&mut self, invocation: &CommandInvocation) {
        let Some(extension_id) = self.extension_commands.get(&invocation.id).cloned() else {
            return;
        };
        let Some(extension) = self.extensions.get(&extension_id).cloned() else {
            self.status = format!("Extension {extension_id} is unavailable");
            return;
        };
        let parent = self.diagnostics.active;
        let correlation =
            self.diagnostics
                .journal
                .begin(DiagnosticDomain::Plugin, &extension_id, parent);
        match extension.invoke(invocation, &self.action_context()) {
            Ok(report) => {
                self.diagnostics.journal.record(
                    correlation,
                    parent,
                    DiagnosticDomain::Plugin,
                    DiagnosticPhase::Completed,
                    &extension_id,
                    BTreeMap::new(),
                );
                match report.effect {
                    ExtensionEffect::Message(message) => self.status = message,
                    ExtensionEffect::Navigate(location) => {
                        if let Some(provider) = self.providers.for_location(&location) {
                            self.navigate_collection(&provider, &location);
                        } else {
                            self.status = format!(
                                "Extension {extension_id} requested unavailable location {}",
                                location.as_str()
                            );
                        }
                    }
                    ExtensionEffect::Open(resources) => {
                        self.open_extension_results(&extension_id, resources);
                    }
                    ExtensionEffect::Task(task) => {
                        self.status = format!("Extension {extension_id} started task {task}");
                    }
                    _ => {
                        self.status =
                            format!("Extension {extension_id} returned an unsupported effect");
                    }
                }
                if !report.diagnostics.is_empty() {
                    self.configuration_diagnostics.push_str("\n\nExtension ");
                    self.configuration_diagnostics.push_str(&extension_id);
                    self.configuration_diagnostics.push_str(":\n");
                    self.configuration_diagnostics
                        .push_str(&report.diagnostics.join("\n"));
                }
            }
            Err(error) => {
                self.diagnostics.journal.record(
                    correlation,
                    parent,
                    DiagnosticDomain::Plugin,
                    DiagnosticPhase::Failed,
                    &extension_id,
                    BTreeMap::from([("error".to_owned(), error.clone())]),
                );
                self.status = format!("Extension {extension_id} failed: {error}");
            }
        }
    }

    fn record_macro_invocation(&mut self, invocation: &CommandInvocation) {
        if self.macro_recorder.is_recording()
            && !self.macro_replaying
            && !invocation.id.as_str().starts_with("near.macro.")
        {
            self.macro_recorder.record(invocation.clone());
        }
    }

    fn dispatch_search_command(&mut self, invocation: &CommandInvocation) {
        match invocation.id.as_str() {
            "near.search.start" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::search_dialog())));
            }
            "near.search.confirmed" => {
                let mode = invocation
                    .arguments
                    .get("mode")
                    .and_then(near_core::CommandValue::as_str)
                    .unwrap_or("replace");
                let Some(mode) = SearchMode::parse(mode) else {
                    "Search mode must be replace, append, or refine".clone_into(&mut self.status);
                    return;
                };
                let predicate = match search_predicate(invocation) {
                    Ok(predicate) => predicate,
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                };
                let options = match search_options(invocation) {
                    Ok(options) => options,
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                };
                self.overlay = None;
                self.start_search(predicate, mode, options);
            }
            "near.search.cancel" => self.cancel_search(self.focused),
            "near.search.reveal" => self.reveal_search_result(),
            "near.search.keep-panel" => self.keep_search_panel(),
            "near.search.panels" => self.show_saved_search_panels(),
            "near.search.open-panel" => self.open_saved_search_panel(invocation),
            _ => {}
        }
    }

    fn dispatch_host_command(&mut self, invocation: &CommandInvocation, keymap: Option<&Keymap>) {
        match invocation.id.as_str() {
            "near.menu.main" => self.open_main_menu_category(match self.focused {
                FocusedPanel::Left => MainMenuCategory::Left,
                FocusedPanel::Right => MainMenuCategory::Right,
            }),
            "near.menu.left" => self.open_main_menu_category(MainMenuCategory::Left),
            "near.menu.files" => self.open_main_menu_category(MainMenuCategory::Files),
            "near.menu.commands" => self.open_main_menu_category(MainMenuCategory::Commands),
            "near.menu.options" => self.open_main_menu_category(MainMenuCategory::Options),
            "near.menu.right" => self.open_main_menu_category(MainMenuCategory::Right),
            "near.menu.previous-category" => self.switch_main_menu_category(-1),
            "near.menu.next-category" => self.switch_main_menu_category(1),
            "near.menu.switch-panel" => self.switch_main_menu_panel(),
            "near.selection.menu" => {
                self.overlay = Some(Overlay::Menu(Self::selection_menu()));
            }
            "near.collection.sort.menu" => {
                self.overlay = Some(Overlay::Menu(self.sort_menu()));
            }
            "near.command-palette.open" => {
                self.overlay = Some(Overlay::CommandPalette {
                    selected: 0,
                    entries: self.command_palette_entries(keymap),
                    search: SelectionSearch::default(),
                });
            }
            "near.menu.up" => self.move_menu(-1),
            "near.menu.down" => self.move_menu(1),
            "near.menu.first" => self.navigate_menu("near.menu.first"),
            "near.menu.last" => self.navigate_menu("near.menu.last"),
            "near.menu.page-up" => self.navigate_menu("near.menu.page-up"),
            "near.menu.page-down" => self.navigate_menu("near.menu.page-down"),
            "near.menu.activate" => self.activate_menu(),
            "near.panel.refresh" => self.refresh_collections(),
            "near.overlay.accept" => {
                self.overlay = None;
                "Confirmed".clone_into(&mut self.status);
            }
            "near.overlay.cancel" => {
                if self.quick_view_interactive {
                    self.quick_view_interactive = false;
                    "Returned to file panel navigation".clone_into(&mut self.status);
                } else {
                    self.persist_overlay_viewer_state();
                    self.overlay = self.overlay_history.pop();
                    if self.overlay.is_some() {
                        "Returned to the previous menu".clone_into(&mut self.status);
                    }
                }
            }
            "near.demo.tasks" => {
                self.overlay = Some(Overlay::Surface(Box::new(self.task_surface())));
            }
            "near.demo.terminal" => {
                self.overlay = Some(Overlay::Surface(Box::new(Self::demo_terminal())));
            }
            "near.terminal.open" => self.toggle_terminal_screen(),
            "near.terminal.menu" => self.show_terminal_workspace_menu(),
            "near.terminal.new" => self.create_terminal_tab(),
            "near.terminal.next" => self.cycle_terminal_tab(1),
            "near.terminal.previous" => self.cycle_terminal_tab(-1),
            "near.terminal.select" => self.select_terminal_tab(invocation),
            "near.terminal.place-left" => self.place_terminal_in_pane(PaneSlot::First),
            "near.terminal.place-right" => self.place_terminal_in_pane(PaneSlot::Second),
            "near.terminal.hide" => self.hide_terminal_workspace(),
            "near.terminal.close" => self.close_terminal_screen(),
            "near.terminal.close-confirmed" => self.confirm_close_terminal_screen(),
            "near.extensions.show" => self.show_extensions(),
            "near.task.cancel" => {
                let task = invocation
                    .arguments
                    .get("task")
                    .and_then(near_core::CommandValue::as_str)
                    .and_then(|task| task.parse::<u64>().ok());
                if let (Some(task), Some(operation)) = (task, self.operation_task.as_ref())
                    && operation.id().0 == task
                {
                    operation.cancel();
                    if let Some(record) = self.task_records.get_mut(&task) {
                        "Cancellation requested".clone_into(&mut record.message);
                    }
                    "Operation cancellation requested".clone_into(&mut self.status);
                } else if let (Some(task), Some(apply_command)) =
                    (task, self.apply_command_task.as_ref())
                    && apply_command.id().0 == task
                {
                    apply_command.cancel();
                    if let Some(record) = self.task_records.get_mut(&task) {
                        "Cancellation requested".clone_into(&mut record.message);
                    }
                    "Apply-command cancellation requested".clone_into(&mut self.status);
                } else {
                    "Task is no longer cancellable".clone_into(&mut self.status);
                }
            }
            "near.task.retry" => "Task retry requested".clone_into(&mut self.status),
            "near.operation.confirmed" => self.execute_operation(invocation),
            "near.operation.retry-elevated" => self.start_elevated_retry(),
            "near.terminal.input" => {
                let text = invocation
                    .arguments
                    .get("text")
                    .and_then(near_core::CommandValue::as_str)
                    .unwrap_or_default();
                self.status = format!("Terminal input: {text:?}");
            }
            "near.settings.show" => {
                let category = invocation
                    .arguments
                    .get("category")
                    .and_then(CommandValue::as_str);
                self.active_settings_category = category.map(str::to_owned);
                self.overlay = Some(Overlay::Surface(Box::new(
                    self.effective_settings_surface_for(category),
                )));
            }
            "near.settings.edit-value" => self.show_settings_value_dialog(invocation),
            "near.settings.apply-candidate" => self.apply_settings_candidate(invocation),
            "near.settings.reload" => self.reload_settings(),
            "near.theme.show" => self.show_theme_menu(),
            "near.theme.preview" => self.preview_theme(invocation),
            "near.theme.roles" => self.show_theme_roles(),
            "near.theme.edit" => self.show_theme_role_dialog(invocation),
            "near.theme.edit-confirmed" => self.update_theme_role(invocation),
            "near.theme.commit" => self.commit_theme(),
            "near.theme.rollback" => self.rollback_theme(),
            "near.devices.show" => self.show_removable_devices(),
            "near.device.disconnect" => self.disconnect_removable_device(),
            "near.about.show" => {
                self.overlay = Some(Overlay::Message {
                    title: "About Near".to_owned(),
                    body: "Universal keyboard-first TUI platform — interaction laboratory."
                        .to_owned(),
                });
            }
            "near.location.parent" => {
                let location = self.focused_panel().location().clone();
                let Some(provider) = self.providers.for_location(&location) else {
                    self.status = format!("No provider for {}", location.as_str());
                    return;
                };
                let Some(parent) = provider.parent(&location) else {
                    self.status = format!("Already at root: {}", location.as_str());
                    return;
                };
                let parent_provider = self.providers.for_location(&parent).unwrap_or(provider);
                self.navigate_collection(&parent_provider, &parent);
            }
            "near.provider.choose" => {
                let target = provider_target(invocation, self.focused);
                self.overlay = Some(Overlay::Menu(self.location_menu(target)));
            }
            "near.provider.navigate" => self.navigate_provider_location(invocation),
            "near.provider.disconnect" => self.disconnect_provider(),
            "near.provider.retry" => self.retry_provider(),
            "near.app.quit" => {
                let dirty = self
                    .editors
                    .iter()
                    .filter(|editor| editor.is_dirty())
                    .map(EditorSurface::title)
                    .collect::<Vec<_>>();
                if !dirty.is_empty() {
                    self.overlay = Some(Overlay::Message {
                        title: "Unsaved Editors".to_owned(),
                        body: format!(
                            "Save or explicitly close these documents before quitting:\n\n{}",
                            dirty.join("\n")
                        ),
                    });
                    return;
                }
                self.persist_active_editor_position();
                self.persist_overlay_viewer_state();
                self.should_quit = true;
            }
            "near.app.force-quit" => {
                self.persist_active_editor_position();
                self.persist_overlay_viewer_state();
                self.should_quit = true;
            }
            _ => self.status = format!("Command not implemented in M0: {}", invocation.id),
        }
    }

    fn show_extensions(&mut self) {
        if self.extensions.is_empty() {
            self.overlay = Some(Overlay::Message {
                title: "Extensions".to_owned(),
                body: "No isolated extensions are loaded.".to_owned(),
            });
            return;
        }
        let mut items = Vec::new();
        for (extension_id, extension) in &self.extensions {
            match extension.menu_items() {
                Ok(contributions) if !contributions.is_empty() => {
                    items.extend(self.extension_menu_items(extension_id, contributions));
                }
                Ok(_) => {
                    let contributions = self
                        .extension_commands
                        .iter()
                        .filter(|(_, owner)| *owner == extension_id)
                        .filter_map(|(command_id, _)| {
                            self.registry
                                .get(command_id)
                                .map(|command| ExtensionMenuItem {
                                    label: command.descriptor().title.clone(),
                                    description: command.descriptor().description.clone(),
                                    command: command_id.clone(),
                                })
                        })
                        .collect();
                    items.extend(self.extension_menu_items(extension_id, contributions));
                }
                Err(error) => items.push(MenuItem {
                    label: extension_id.clone(),
                    description: format!("menu unavailable: {error}"),
                    command: CommandInvocation {
                        id: CommandId::from("near.extensions.show"),
                        arguments: BTreeMap::new(),
                    },
                    enabled: false,
                }),
            }
            if let Some((command, _)) = self
                .extension_settings_open
                .iter()
                .find(|(_, owner)| *owner == extension_id)
            {
                items.push(self.menu_item(
                    &format!("Configure {extension_id}"),
                    "Extension settings",
                    command.as_str(),
                ));
            }
        }
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.extensions",
            "Extensions",
            items,
        )));
    }

    fn show_theme_menu(&mut self) {
        if self.working_theme.is_none() {
            "No editable runtime theme is configured".clone_into(&mut self.status);
            return;
        }
        let active = self.working_theme.as_ref().map(SemanticTheme::name);
        let mut items = self
            .theme_presets
            .keys()
            .map(|name| MenuItem {
                label: format!(
                    "{} {name}",
                    if active == Some(name.as_str()) {
                        "√"
                    } else {
                        " "
                    }
                ),
                description: "Preview preset immediately".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.theme.preview"),
                    arguments: BTreeMap::from([(
                        "name".to_owned(),
                        CommandValue::String(name.clone()),
                    )]),
                },
                enabled: true,
            })
            .collect::<Vec<_>>();
        items.extend([
            self.menu_item(
                "Edit semantic roles",
                "Foreground and background at the detected terminal depth",
                "near.theme.roles",
            ),
            self.menu_item(
                "Commit preview",
                "Make the working theme the rollback baseline",
                "near.theme.commit",
            ),
            self.menu_item(
                "Roll back preview",
                "Restore the last committed theme atomically",
                "near.theme.rollback",
            ),
        ]);
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.themes",
            "Colors and Themes",
            items,
        )));
    }

    fn preview_theme(&mut self, invocation: &CommandInvocation) {
        let Some(name) = invocation
            .arguments
            .get("name")
            .and_then(CommandValue::as_str)
        else {
            "Theme preset name is missing".clone_into(&mut self.status);
            return;
        };
        let Some(theme) = self.theme_presets.get(name).cloned() else {
            self.status = format!("Theme preset {name} is unavailable");
            return;
        };
        self.working_theme = Some(theme);
        self.overlay = None;
        self.status = format!("Previewing theme {name}; commit or roll back from Options");
    }

    fn show_theme_roles(&mut self) {
        let Some(theme) = self.working_theme.as_ref() else {
            "No editable runtime theme is configured".clone_into(&mut self.status);
            return;
        };
        let items = theme
            .role_names()
            .map(|role| {
                let (foreground, background) = theme.role_colors(role).unwrap_or_default();
                MenuItem {
                    label: role.to_owned(),
                    description: format!(
                        "fg {}  bg {}",
                        editable_color(foreground),
                        editable_color(background)
                    ),
                    command: CommandInvocation {
                        id: CommandId::from("near.theme.edit"),
                        arguments: BTreeMap::from([(
                            "role".to_owned(),
                            CommandValue::String(role.to_owned()),
                        )]),
                    },
                    enabled: true,
                }
            })
            .collect();
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.theme-roles",
            format!("Semantic Roles — {:?}", theme.terminal_depth()),
            items,
        )));
    }

    fn show_theme_role_dialog(&mut self, invocation: &CommandInvocation) {
        let Some(role) = invocation
            .arguments
            .get("role")
            .and_then(CommandValue::as_str)
        else {
            "Theme role is missing".clone_into(&mut self.status);
            return;
        };
        let Some(theme) = self.working_theme.as_ref() else {
            "No editable runtime theme is configured".clone_into(&mut self.status);
            return;
        };
        let Some((foreground, background)) = theme.role_colors(role) else {
            self.status = format!("Theme role {role} is unavailable");
            return;
        };
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            "near-fm.theme-role",
            format!("Role {role} — blank inherits; default, ansi:N, or #RRGGBB"),
            vec![
                DialogField {
                    id: "role".to_owned(),
                    label: "Role".to_owned(),
                    value: role.to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "foreground".to_owned(),
                    label: "Foreground".to_owned(),
                    value: foreground.map(format_semantic_color).unwrap_or_default(),
                    required: false,
                    secret: false,
                },
                DialogField {
                    id: "background".to_owned(),
                    label: "Background".to_owned(),
                    value: background.map(format_semantic_color).unwrap_or_default(),
                    required: false,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: CommandId::from("near.theme.edit-confirmed"),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn update_theme_role(&mut self, invocation: &CommandInvocation) {
        let Some(role) = invocation
            .arguments
            .get("role")
            .and_then(CommandValue::as_str)
        else {
            "Theme role is missing".clone_into(&mut self.status);
            return;
        };
        let parse = |name: &str| -> Result<_, String> {
            invocation
                .arguments
                .get(name)
                .and_then(CommandValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(parse_semantic_color)
                .transpose()
                .map_err(|error| error.to_string())
        };
        let (foreground, background) = match (parse("foreground"), parse("background")) {
            (Ok(foreground), Ok(background)) => (foreground, background),
            (Err(error), _) | (_, Err(error)) => {
                self.status = format!("Theme color is invalid: {error}");
                return;
            }
        };
        let Some(theme) = self.working_theme.as_mut() else {
            "No editable runtime theme is configured".clone_into(&mut self.status);
            return;
        };
        if let Err(error) = theme.set_role_colors(role, foreground, background) {
            self.status = error.to_string();
            return;
        }
        self.overlay = None;
        self.status = format!("Previewing edited role {role}; commit or roll back from Options");
    }

    fn commit_theme(&mut self) {
        let Some(theme) = self.working_theme.clone() else {
            "No theme preview is active".clone_into(&mut self.status);
            return;
        };
        let name = theme.name().to_owned();
        self.committed_theme = Some(theme);
        self.overlay = None;
        self.status = format!("Committed theme {name} for this session");
    }

    fn rollback_theme(&mut self) {
        let Some(theme) = self.committed_theme.clone() else {
            "No committed theme is available".clone_into(&mut self.status);
            return;
        };
        let name = theme.name().to_owned();
        self.working_theme = Some(theme);
        self.overlay = None;
        self.status = format!("Rolled back to theme {name}");
    }

    fn show_removable_devices(&mut self) {
        let provider_id = ProviderId::from("near.removable-devices");
        let Some(provider) = self.providers.get(&provider_id) else {
            "Removable-device provider is unavailable".clone_into(&mut self.status);
            return;
        };
        self.navigate_collection(&provider, &Location::new("device://attached"));
        "Listing removable devices".clone_into(&mut self.status);
    }

    fn disconnect_removable_device(&mut self) {
        let Some(entry) = self
            .focused_panel()
            .current()
            .filter(|entry| !is_parent_entry(entry))
            .cloned()
        else {
            "No removable device is selected".clone_into(&mut self.status);
            return;
        };
        if !self
            .providers
            .get(&entry.resource.provider)
            .is_some_and(|provider| {
                provider
                    .capabilities(&entry.resource)
                    .contains(&CapabilityId::from("device.disconnect"))
            })
        {
            "The selected device does not support safe disconnection".clone_into(&mut self.status);
            return;
        }
        let Some(device_id) = entry
            .metadata
            .extensions
            .get("near.device.id")
            .and_then(|value| match value {
                MetadataValue::String(value) => Some(value.clone()),
                _ => None,
            })
        else {
            "The selected device has no stable platform identifier".clone_into(&mut self.status);
            return;
        };
        let Some(service) = self.removable_devices.as_ref() else {
            "Removable-device service is unavailable".clone_into(&mut self.status);
            return;
        };
        let parent = self.diagnostics.active;
        let correlation =
            self.diagnostics
                .journal
                .begin(DiagnosticDomain::Provider, "device.disconnect", parent);
        match service.disconnect(&device_id) {
            Ok(report) => {
                self.diagnostics.journal.record(
                    correlation,
                    parent,
                    DiagnosticDomain::Provider,
                    DiagnosticPhase::Completed,
                    "device.disconnect",
                    BTreeMap::from([
                        ("device".to_owned(), report.device.clone()),
                        ("action".to_owned(), report.action.clone()),
                        ("audit".to_owned(), report.audit.clone()),
                    ]),
                );
                self.configuration_diagnostics
                    .push_str("\n\nDevice disconnect:\n");
                self.configuration_diagnostics.push_str(&report.audit);
                self.status = format!("Safely disconnected device {}", report.device);
                self.refresh_collections();
            }
            Err(error) => {
                self.diagnostics.journal.record(
                    correlation,
                    parent,
                    DiagnosticDomain::Provider,
                    DiagnosticPhase::Failed,
                    "device.disconnect",
                    BTreeMap::from([
                        ("device".to_owned(), device_id),
                        ("error".to_owned(), error.clone()),
                    ]),
                );
                self.status = format!("Device disconnect failed: {error}");
            }
        }
    }

    fn extension_menu_items(
        &self,
        extension_id: &str,
        contributions: Vec<ExtensionMenuItem>,
    ) -> Vec<MenuItem> {
        contributions
            .into_iter()
            .map(|item| {
                let invocation = CommandInvocation {
                    id: item.command,
                    arguments: BTreeMap::new(),
                };
                match self.registry.check(&invocation, &self.action_context()) {
                    Ok(_) => MenuItem {
                        label: item.label,
                        description: format!("{extension_id}: {}", item.description),
                        command: invocation,
                        enabled: true,
                    },
                    Err(error) => MenuItem {
                        label: item.label,
                        description: format!("{extension_id}: unavailable — {error}"),
                        command: invocation,
                        enabled: false,
                    },
                }
            })
            .collect()
    }

    fn show_extension_settings(&mut self, command: &CommandId) {
        let Some(extension_id) = self.extension_settings_open.get(command).cloned() else {
            return;
        };
        let Some(extension) = self.extensions.get(&extension_id) else {
            self.status = format!("Extension {extension_id} is unavailable");
            return;
        };
        let settings = match extension.settings() {
            Ok(settings) => settings,
            Err(error) => {
                self.status = format!("Extension {extension_id} settings unavailable: {error}");
                return;
            }
        };
        let Some((save_command, _)) = self
            .extension_settings_save
            .iter()
            .find(|(_, owner)| *owner == &extension_id)
        else {
            self.status = format!("Extension {extension_id} has no editable settings");
            return;
        };
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            format!("near-fm.extension.{extension_id}.settings"),
            format!("Configure {extension_id}"),
            settings.into_iter().map(extension_setting_field).collect(),
            CommandInvocation {
                id: save_command.clone(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn save_extension_settings(&mut self, invocation: &CommandInvocation) {
        let Some(extension_id) = self.extension_settings_save.get(&invocation.id).cloned() else {
            return;
        };
        let Some(extension) = self.extensions.get(&extension_id) else {
            self.status = format!("Extension {extension_id} is unavailable");
            return;
        };
        let settings = invocation
            .arguments
            .iter()
            .filter_map(|(id, value)| value.as_str().map(|value| (id.clone(), value.to_owned())))
            .collect();
        match extension.update_settings(&settings) {
            Ok(()) => {
                self.overlay = None;
                self.status = format!("Saved settings for {extension_id}");
            }
            Err(error) => {
                self.status = format!("Extension {extension_id} rejected settings: {error}");
            }
        }
    }

    /// Renders a deterministic headless snapshot for structural tests.
    ///
    /// # Panics
    ///
    /// Panics only if Ratatui's infallible test backend unexpectedly reports an error.
    pub fn snapshot(
        &self,
        theme: &SemanticTheme,
        keymap: &Keymap,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        self.semantic_snapshot(theme, keymap, width, height)
            .text_lines()
    }

    /// Renders content and semantic role IDs through the same headless render pass.
    ///
    /// # Panics
    ///
    /// Panics only if Ratatui's infallible test backend unexpectedly reports an error.
    pub fn semantic_snapshot(
        &self,
        theme: &SemanticTheme,
        keymap: &Keymap,
        width: u16,
        height: u16,
    ) -> SemanticSnapshot {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test backend is infallible");
        let mut roles = RoleBuffer::new(width, height, "workspace.background");
        let effective_theme = self.effective_theme(theme);
        terminal
            .draw(|frame| self.render(frame, &effective_theme, keymap, &mut roles))
            .expect("test backend is infallible");
        SemanticSnapshot::from_buffer(terminal.backend().buffer(), &roles)
    }

    #[allow(clippy::too_many_lines)]
    fn register_commands(&mut self) {
        let commands = [
            ("near.help.context", "Context help", SafetyClass::ReadOnly),
            ("near.help.contents", "Help contents", SafetyClass::ReadOnly),
            (
                "near.help.extensions",
                "Extension help",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-palette.open",
                "Commands",
                SafetyClass::ReadOnly,
            ),
            (
                "near.workspace.focus-peer",
                "Switch panel",
                SafetyClass::ReadOnly,
            ),
            (
                "near.workspace.swap-peers",
                "Swap panels",
                SafetyClass::ReadOnly,
            ),
            (
                "near.workspace.resize-panels",
                "Resize panels",
                SafetyClass::ReadOnly,
            ),
            (
                "near.workspace.reset-panel-layout",
                "Reset panel layout",
                SafetyClass::ReadOnly,
            ),
            ("near.collection.move", "Move cursor", SafetyClass::ReadOnly),
            (
                "near.collection.page",
                "Move one page",
                SafetyClass::ReadOnly,
            ),
            ("near.collection.first", "First item", SafetyClass::ReadOnly),
            ("near.collection.last", "Last item", SafetyClass::ReadOnly),
            (
                "near.collection.scroll-horizontal",
                "Scroll panel text",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.horizontal-start",
                "Align panel text left",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.horizontal-end",
                "Align panel text right",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.toggle-selection",
                "Select",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.toggle-selection-move",
                "Select and move",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.menu",
                "Selection commands",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.select-mask",
                "Select by mask",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.unselect-mask",
                "Unselect by mask",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.mask-confirmed",
                "Apply selection mask",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.same-extension",
                "Select same extension",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.same-name",
                "Select same name",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.invert",
                "Invert selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.save",
                "Save selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.restore",
                "Restore selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.compare-folders",
                "Compare panel folders",
                SafetyClass::ReadOnly,
            ),
            (
                "near.selection.compare-folders-confirmed",
                "Apply folder comparison",
                SafetyClass::ReadOnly,
            ),
            (
                "near.panel.view-mode.menu",
                "Panel view modes",
                SafetyClass::ReadOnly,
            ),
            (
                "near.panel.view-mode.set",
                "Set panel view mode",
                SafetyClass::ReadOnly,
            ),
            (
                "near.operation.apply-command",
                "Apply command",
                SafetyClass::Confirmable,
            ),
            (
                "near.operation.apply-command-confirmed",
                "Execute applied command",
                SafetyClass::Confirmable,
            ),
            (
                "near.collection.sort.menu",
                "Sort modes",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.unsorted",
                "Sort: unsorted",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.name",
                "Sort: name",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.extension",
                "Sort: extension",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.modified",
                "Sort: modified time",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.size",
                "Sort: size",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.created",
                "Sort: creation time",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.accessed",
                "Sort: access time",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.kind",
                "Sort: metadata kind",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.owner",
                "Sort: owner",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.permissions",
                "Sort: permissions",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.toggle-reverse",
                "Toggle reverse sort",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.toggle-numeric",
                "Toggle numeric sort",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.toggle-selected-first",
                "Toggle selected first",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.toggle-directories-first",
                "Toggle directories first",
                SafetyClass::ReadOnly,
            ),
            (
                "near.collection.sort.toggle-groups",
                "Toggle highlighting sort groups",
                SafetyClass::ReadOnly,
            ),
            (
                "near.highlighting.report",
                "Inspect highlighting and sort groups",
                SafetyClass::ReadOnly,
            ),
            ("near.resource.open", "Open", SafetyClass::ReadOnly),
            ("near.resource.view", "View", SafetyClass::ReadOnly),
            ("near.resource.edit", "Edit", SafetyClass::Confirmable),
            (
                "near.resource.edit-external",
                "Edit with external tool",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.execute-external",
                "Execute with external association",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.associations",
                "File associations",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource.association-run",
                "Run selected file association",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.description",
                "Edit file description",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.description-confirmed",
                "Save file description",
                SafetyClass::Confirmable,
            ),
            (
                "near.folder-description.view",
                "View folder description",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-description.edit",
                "Edit folder description",
                SafetyClass::Confirmable,
            ),
            (
                "near.command-prefixes.show",
                "Command prefixes",
                SafetyClass::ReadOnly,
            ),
            ("near.filters.show", "Panel filters", SafetyClass::ReadOnly),
            (
                "near.filters.toggle",
                "Toggle panel filter",
                SafetyClass::ReadOnly,
            ),
            (
                "near.filters.clear",
                "Clear panel filters",
                SafetyClass::ReadOnly,
            ),
            (
                "near.user-menu.global",
                "Global user menu",
                SafetyClass::ReadOnly,
            ),
            (
                "near.user-menu.local",
                "Local user menu",
                SafetyClass::ReadOnly,
            ),
            (
                "near.user-menu.run",
                "Run user-menu entry",
                SafetyClass::Confirmable,
            ),
            ("near.editor.up", "Editor up", SafetyClass::ReadOnly),
            ("near.editor.down", "Editor down", SafetyClass::ReadOnly),
            ("near.editor.left", "Editor left", SafetyClass::ReadOnly),
            ("near.editor.right", "Editor right", SafetyClass::ReadOnly),
            ("near.editor.select-up", "Select up", SafetyClass::ReadOnly),
            (
                "near.editor.select-down",
                "Select down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.select-left",
                "Select left",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.select-right",
                "Select right",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.column-select-up",
                "Column select up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.column-select-down",
                "Column select down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.column-select-left",
                "Column select left",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.column-select-right",
                "Column select right",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.page-up",
                "Editor page up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.page-down",
                "Editor page down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.home",
                "Editor line start",
                SafetyClass::ReadOnly,
            ),
            ("near.editor.end", "Editor line end", SafetyClass::ReadOnly),
            (
                "near.editor.newline",
                "Editor newline",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.insert-tab",
                "Editor indent",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.delete",
                "Editor delete",
                SafetyClass::Confirmable,
            ),
            ("near.editor.undo", "Editor undo", SafetyClass::Confirmable),
            ("near.editor.redo", "Editor redo", SafetyClass::Confirmable),
            (
                "near.editor.selection-toggle",
                "Editor selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.selection-clear",
                "Clear editor selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.toggle-persistent-blocks",
                "Toggle persistent editor blocks",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.select-all",
                "Editor select all",
                SafetyClass::ReadOnly,
            ),
            ("near.editor.copy", "Editor copy", SafetyClass::ReadOnly),
            ("near.editor.cut", "Editor cut", SafetyClass::Confirmable),
            (
                "near.editor.paste",
                "Editor paste",
                SafetyClass::Confirmable,
            ),
            ("near.editor.save", "Editor save", SafetyClass::Confirmable),
            (
                "near.editor.save-as",
                "Editor Save As",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.save-as-confirmed",
                "Confirm editor Save As",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.external-change",
                "Resolve external editor change",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.external-reload",
                "Reload external editor version",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.external-compare",
                "Compare external editor version",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.external-keep-local",
                "Overwrite external editor version",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.lossy-save-warning",
                "Confirm lossy editor save",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.lossy-save-confirmed",
                "Execute lossy editor save",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.search-start",
                "Editor search",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.search-confirm",
                "Confirm editor search",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.search-next",
                "Next editor match",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.replace-start",
                "Editor replace",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.replace-all",
                "Replace all editor matches",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.find-all",
                "Find all editor matches",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.toggle-regex",
                "Toggle editor regex",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.toggle-preserve-style",
                "Toggle replacement style preservation",
                SafetyClass::ReadOnly,
            ),
            (
                "near.editor.close",
                "Close editor",
                SafetyClass::Confirmable,
            ),
            (
                "near.editor.close-confirmed",
                "Discard and close editor",
                SafetyClass::Confirmable,
            ),
            ("near.screen.list", "Screen list", SafetyClass::ReadOnly),
            ("near.screen.panels", "Panels screen", SafetyClass::ReadOnly),
            ("near.screen.editor", "Editor screen", SafetyClass::ReadOnly),
            ("near.screen.terminal", "User screen", SafetyClass::ReadOnly),
            ("near.screen.next", "Next screen", SafetyClass::ReadOnly),
            (
                "near.screen.previous",
                "Previous screen",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.history-previous",
                "Previous command",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.history-next",
                "Next command",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.history-show",
                "Command history",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.history-use",
                "Use history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.history-toggle-lock",
                "Lock history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.up",
                "History up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.down",
                "History down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.first",
                "First history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.last",
                "Last history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.page-up",
                "Previous history page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.page-down",
                "Next history page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.use",
                "Choose history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.toggle-lock",
                "Toggle history lock",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-history.clear-unlocked",
                "Clear unlocked command history",
                SafetyClass::Confirmable,
            ),
            (
                "near.command-line.history-clear-unlocked",
                "Clear unlocked command history entries",
                SafetyClass::Confirmable,
            ),
            (
                "near.history.viewed-show",
                "Viewed resource history",
                SafetyClass::ReadOnly,
            ),
            (
                "near.history.menu",
                "View and edit histories",
                SafetyClass::ReadOnly,
            ),
            (
                "near.history.edited-show",
                "Edited resource history",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.up",
                "Resource history up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.down",
                "Resource history down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.first",
                "First resource history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.last",
                "Last resource history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.page-up",
                "Previous resource history page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.page-down",
                "Next resource history page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.open",
                "Open resource history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.toggle-lock",
                "Toggle resource history lock",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.clear-unlocked",
                "Clear unlocked resource history",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource-history.open-selected",
                "Open selected resource history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.toggle-lock-selected",
                "Toggle selected resource history lock",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource-history.clear",
                "Clear resource history",
                SafetyClass::Confirmable,
            ),
            (
                "near.command-line.insert-current",
                "Insert current name",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.insert-peer",
                "Insert peer name",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.insert-selected",
                "Insert selected names",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.insert-focused-path",
                "Insert focused panel path",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.insert-peer-path",
                "Insert peer panel path",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.insert-current-path",
                "Insert current resource path",
                SafetyClass::ReadOnly,
            ),
            (
                "near.command-line.insert-peer-current-path",
                "Insert peer resource path",
                SafetyClass::ReadOnly,
            ),
            (
                "near.location.history-show",
                "Folder history",
                SafetyClass::ReadOnly,
            ),
            (
                "near.location.history-clear",
                "Clear folder history",
                SafetyClass::Confirmable,
            ),
            (
                "near.location.history-toggle-lock",
                "Toggle folder history lock",
                SafetyClass::ReadOnly,
            ),
            (
                "near.location.history-open",
                "Open folder history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.location.shortcut-assign",
                "Assign folder shortcut",
                SafetyClass::ReadOnly,
            ),
            (
                "near.location.shortcut-open",
                "Open folder shortcut",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.up",
                "Folder history up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.down",
                "Folder history down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.first",
                "First folder history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.last",
                "Last folder history entry",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.page-up",
                "Previous folder history page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.page-down",
                "Next folder history page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.activate",
                "Open folder history selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.folder-history.clear",
                "Clear folder history",
                SafetyClass::Confirmable,
            ),
            (
                "near.folder-history.toggle-lock",
                "Toggle folder history lock",
                SafetyClass::ReadOnly,
            ),
            (
                "near.resource.copy-to-peer",
                "Copy",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.copy-current-to-peer",
                "Copy current item",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.move-to-peer",
                "Move",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.move-current-to-peer",
                "Move current item",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.rename",
                "Rename in place",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.rename-confirmed",
                "Confirm rename",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.link",
                "Create link",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.link-confirmed",
                "Confirm link creation",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.attributes",
                "Attributes and timestamps",
                SafetyClass::Confirmable,
            ),
            (
                "near.resource.attributes-confirmed",
                "Confirm attribute update",
                SafetyClass::Confirmable,
            ),
            ("near.search.start", "Find files", SafetyClass::ReadOnly),
            (
                "near.temp-panel.open",
                "Open temporary panel",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.list",
                "Temporary panels",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.remove",
                "Remove temporary-panel references",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.clear",
                "Clear temporary panel",
                SafetyClass::Confirmable,
            ),
            (
                "near.temp-panel.clear-all",
                "Clear all temporary panels",
                SafetyClass::Confirmable,
            ),
            (
                "near.temp-panel.reveal",
                "Reveal temporary-panel resource",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.import",
                "Import temporary-panel list",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.import-confirmed",
                "Confirm temporary-panel list import",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.export",
                "Export temporary-panel list",
                SafetyClass::Confirmable,
            ),
            (
                "near.temp-panel.export-confirmed",
                "Confirm temporary-panel list export",
                SafetyClass::Confirmable,
            ),
            (
                "near.temp-panel.safe-toggle",
                "Toggle temporary-panel safe mode",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.refresh",
                "Refresh temporary-panel references",
                SafetyClass::ReadOnly,
            ),
            (
                "near.temp-panel.menu-select",
                "Activate temporary-panel list menu item",
                SafetyClass::ReadOnly,
            ),
            (
                "near.search.confirmed",
                "Run recursive search",
                SafetyClass::ReadOnly,
            ),
            ("near.search.cancel", "Cancel search", SafetyClass::ReadOnly),
            (
                "near.search.reveal",
                "Reveal search result",
                SafetyClass::ReadOnly,
            ),
            (
                "near.search.keep-panel",
                "Keep search result panel",
                SafetyClass::ReadOnly,
            ),
            (
                "near.search.panels",
                "Saved search panels",
                SafetyClass::ReadOnly,
            ),
            (
                "near.search.open-panel",
                "Open saved search panel",
                SafetyClass::ReadOnly,
            ),
            (
                "near.handler.diagnostics",
                "Explain external handler",
                SafetyClass::ReadOnly,
            ),
            (
                "near.macro.record-toggle",
                "Record semantic macro",
                SafetyClass::ReadOnly,
            ),
            (
                "near.macro.play-last",
                "Replay last semantic macro",
                SafetyClass::ReadOnly,
            ),
            (
                "near.macro.show-last",
                "Inspect last semantic macro",
                SafetyClass::ReadOnly,
            ),
            (
                "near.macro.manage",
                "Manage semantic macros",
                SafetyClass::ReadOnly,
            ),
            ("near.macro.actions", "Macro actions", SafetyClass::ReadOnly),
            ("near.macro.play", "Replay macro", SafetyClass::ReadOnly),
            ("near.macro.edit", "Edit macro", SafetyClass::ReadOnly),
            (
                "near.macro.edit-confirmed",
                "Confirm macro edit",
                SafetyClass::Confirmable,
            ),
            ("near.macro.bind", "Bind macro", SafetyClass::ReadOnly),
            (
                "near.macro.bind-confirmed",
                "Confirm macro binding",
                SafetyClass::Confirmable,
            ),
            (
                "near.macro.delete",
                "Delete macro",
                SafetyClass::Confirmable,
            ),
            (
                "near.macro.delete-confirmed",
                "Confirm macro deletion",
                SafetyClass::Confirmable,
            ),
            (
                "near.macro.diagnose",
                "Diagnose macro",
                SafetyClass::ReadOnly,
            ),
            (
                "near.fs.create-directory",
                "New folder",
                SafetyClass::Confirmable,
            ),
            (
                "near.fs.create-directory.confirmed",
                "Confirm new folder",
                SafetyClass::Confirmable,
            ),
            (
                "near.archive.create",
                "Create ZIP archive",
                SafetyClass::Confirmable,
            ),
            (
                "near.archive.create-confirmed",
                "Confirm ZIP archive creation",
                SafetyClass::Confirmable,
            ),
            (
                "near.operation.execute",
                "Execute operation plan",
                SafetyClass::Confirmable,
            ),
            (
                "near.operation.confirmed",
                "Confirm operation plan",
                SafetyClass::Confirmable,
            ),
            (
                "near.operation.retry-elevated",
                "Retry operation with elevation",
                SafetyClass::Privileged,
            ),
            (
                "near.resource.restore-last-trash",
                "Restore last Trash operation",
                SafetyClass::Confirmable,
            ),
            (
                "near.operation.conflict.skip",
                "Skip operation conflicts",
                SafetyClass::ReadOnly,
            ),
            (
                "near.operation.conflict.replace",
                "Replace operation conflicts",
                SafetyClass::Confirmable,
            ),
            (
                "near.operation.conflict.rename",
                "Rename operation conflicts",
                SafetyClass::ReadOnly,
            ),
            ("near.resource.trash", "Trash", SafetyClass::Destructive),
            (
                "near.resource.trash-current",
                "Trash current item",
                SafetyClass::Destructive,
            ),
            (
                "near.resource.delete",
                "Delete permanently",
                SafetyClass::Destructive,
            ),
            (
                "near.resource.wipe",
                "Overwrite and delete files",
                SafetyClass::Destructive,
            ),
            (
                "near.resource.wipe-confirmed",
                "Confirm overwrite and delete",
                SafetyClass::Destructive,
            ),
            (
                "near.panel.toggle-quick-view",
                "Toggle quick view",
                SafetyClass::ReadOnly,
            ),
            (
                "near.panel.quick-view-control",
                "Toggle quick-view controls",
                SafetyClass::ReadOnly,
            ),
            (
                "near.panel.toggle-tree",
                "Toggle tree panel",
                SafetyClass::ReadOnly,
            ),
            (
                "near.panel.toggle-information",
                "Toggle information panel",
                SafetyClass::ReadOnly,
            ),
            ("near.menu.main", "Menu", SafetyClass::ReadOnly),
            ("near.menu.left", "Left panel menu", SafetyClass::ReadOnly),
            ("near.menu.files", "Files menu", SafetyClass::ReadOnly),
            ("near.menu.commands", "Commands menu", SafetyClass::ReadOnly),
            ("near.menu.options", "Options menu", SafetyClass::ReadOnly),
            ("near.menu.right", "Right panel menu", SafetyClass::ReadOnly),
            (
                "near.menu.previous-category",
                "Previous main menu",
                SafetyClass::ReadOnly,
            ),
            (
                "near.menu.next-category",
                "Next main menu",
                SafetyClass::ReadOnly,
            ),
            (
                "near.menu.switch-panel",
                "Switch panel menu",
                SafetyClass::ReadOnly,
            ),
            ("near.menu.up", "Menu up", SafetyClass::ReadOnly),
            ("near.menu.down", "Menu down", SafetyClass::ReadOnly),
            ("near.menu.first", "First menu item", SafetyClass::ReadOnly),
            ("near.menu.last", "Last menu item", SafetyClass::ReadOnly),
            (
                "near.menu.page-up",
                "Previous menu page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.menu.page-down",
                "Next menu page",
                SafetyClass::ReadOnly,
            ),
            ("near.menu.activate", "Activate menu", SafetyClass::ReadOnly),
            ("near.overlay.accept", "Confirm", SafetyClass::Confirmable),
            ("near.overlay.cancel", "Close", SafetyClass::ReadOnly),
            ("near.location.parent", "Parent", SafetyClass::ReadOnly),
            (
                "near.panel.refresh",
                "Refresh panels",
                SafetyClass::ReadOnly,
            ),
            (
                "near.provider.choose",
                "Choose provider",
                SafetyClass::ReadOnly,
            ),
            (
                "near.provider.navigate",
                "Navigate provider location",
                SafetyClass::ReadOnly,
            ),
            (
                "near.provider.disconnect",
                "Disconnect current provider",
                SafetyClass::Confirmable,
            ),
            (
                "near.provider.retry",
                "Reconnect current provider",
                SafetyClass::ReadOnly,
            ),
            ("near.demo.tasks", "Task surface", SafetyClass::ReadOnly),
            (
                "near.demo.terminal",
                "Terminal surface",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.open",
                "Open embedded terminal",
                SafetyClass::Confirmable,
            ),
            ("near.terminal.menu", "Terminal tabs", SafetyClass::ReadOnly),
            (
                "near.terminal.new",
                "New terminal tab",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.next",
                "Next terminal tab",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.previous",
                "Previous terminal tab",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.select",
                "Select terminal tab",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.place-left",
                "Place terminal in left pane",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.place-right",
                "Place terminal in right pane",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.hide",
                "Hide terminal pane",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.close",
                "Close user screen",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.close-confirmed",
                "Terminate shell and close user screen",
                SafetyClass::Confirmable,
            ),
            ("near.extensions.show", "Extensions", SafetyClass::ReadOnly),
            ("near.settings.show", "Settings", SafetyClass::ReadOnly),
            (
                "near.settings.up",
                "Previous setting",
                SafetyClass::ReadOnly,
            ),
            ("near.settings.down", "Next setting", SafetyClass::ReadOnly),
            (
                "near.settings.first",
                "First setting",
                SafetyClass::ReadOnly,
            ),
            ("near.settings.last", "Last setting", SafetyClass::ReadOnly),
            (
                "near.settings.page-up",
                "Previous settings page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.settings.page-down",
                "Next settings page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.settings.toggle",
                "Toggle setting",
                SafetyClass::ReadOnly,
            ),
            (
                "near.settings.reset",
                "Reset setting",
                SafetyClass::ReadOnly,
            ),
            ("near.settings.edit", "Edit setting", SafetyClass::ReadOnly),
            (
                "near.settings.edit-value",
                "Open typed setting value editor",
                SafetyClass::ReadOnly,
            ),
            (
                "near.settings.apply-candidate",
                "Apply typed settings candidate",
                SafetyClass::Reversible,
            ),
            (
                "near.settings.reload",
                "Reload settings",
                SafetyClass::ReadOnly,
            ),
            (
                "near.settings.toggle-advanced",
                "Show or hide advanced settings",
                SafetyClass::ReadOnly,
            ),
            (
                "near.theme.show",
                "Colors and themes",
                SafetyClass::ReadOnly,
            ),
            (
                "near.theme.preview",
                "Preview theme preset",
                SafetyClass::ReadOnly,
            ),
            (
                "near.theme.roles",
                "Semantic color roles",
                SafetyClass::ReadOnly,
            ),
            (
                "near.theme.edit",
                "Edit semantic role",
                SafetyClass::ReadOnly,
            ),
            (
                "near.theme.edit-confirmed",
                "Preview semantic role colors",
                SafetyClass::Reversible,
            ),
            (
                "near.theme.commit",
                "Commit theme preview",
                SafetyClass::Reversible,
            ),
            (
                "near.theme.rollback",
                "Roll back theme preview",
                SafetyClass::Reversible,
            ),
            ("near.about.show", "About Near", SafetyClass::ReadOnly),
            (
                "near.devices.show",
                "Removable devices",
                SafetyClass::ReadOnly,
            ),
            (
                "near.device.disconnect",
                "Safely disconnect device",
                SafetyClass::Confirmable,
            ),
            ("near.viewer.up", "Viewer up", SafetyClass::ReadOnly),
            ("near.viewer.down", "Viewer down", SafetyClass::ReadOnly),
            ("near.viewer.left", "Viewer left", SafetyClass::ReadOnly),
            ("near.viewer.right", "Viewer right", SafetyClass::ReadOnly),
            (
                "near.viewer.page-up",
                "Viewer page up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.page-down",
                "Viewer page down",
                SafetyClass::ReadOnly,
            ),
            ("near.viewer.home", "Viewer start", SafetyClass::ReadOnly),
            ("near.viewer.end", "Viewer end", SafetyClass::ReadOnly),
            (
                "near.viewer.toggle-wrap",
                "Toggle wrap",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.toggle-hex",
                "Toggle hex",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.cycle-encoding",
                "Cycle viewer encoding",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.search-start",
                "Start viewer search",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.search-confirm",
                "Confirm viewer search",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.search-next",
                "Find next in viewer",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.search-previous",
                "Find previous in viewer",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.goto-start",
                "Go to viewer position",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.goto-confirm",
                "Confirm viewer position",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.history-back",
                "Previous viewer position",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.history-forward",
                "Next viewer position",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.bookmark-set",
                "Set viewer bookmark",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.bookmark-jump",
                "Jump to viewer bookmark",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.select-up",
                "Extend viewer selection up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.select-down",
                "Extend viewer selection down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.select-left",
                "Extend viewer selection left",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.select-right",
                "Extend viewer selection right",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.column-select-up",
                "Extend viewer column selection up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.column-select-down",
                "Extend viewer column selection down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.column-select-left",
                "Extend viewer column selection left",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.column-select-right",
                "Extend viewer column selection right",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.selection-clear",
                "Clear viewer selection",
                SafetyClass::ReadOnly,
            ),
            (
                "near.viewer.copy",
                "Copy viewer selection",
                SafetyClass::ReadOnly,
            ),
            ("near.help.up", "Help up", SafetyClass::ReadOnly),
            ("near.help.down", "Help down", SafetyClass::ReadOnly),
            ("near.help.home", "Help home", SafetyClass::ReadOnly),
            ("near.help.end", "Help end", SafetyClass::ReadOnly),
            (
                "near.help.page-up",
                "Previous help page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.help.page-down",
                "Next help page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.help.next-link",
                "Next help link",
                SafetyClass::ReadOnly,
            ),
            (
                "near.help.previous-link",
                "Previous help link",
                SafetyClass::ReadOnly,
            ),
            (
                "near.help.activate",
                "Open help link",
                SafetyClass::ReadOnly,
            ),
            (
                "near.help.back",
                "Previous help topic",
                SafetyClass::ReadOnly,
            ),
            ("near.help.search", "Search help", SafetyClass::ReadOnly),
            (
                "near.help.search-next",
                "Next help search result",
                SafetyClass::ReadOnly,
            ),
            ("near.tree.up", "Tree up", SafetyClass::ReadOnly),
            ("near.tree.down", "Tree down", SafetyClass::ReadOnly),
            (
                "near.tree.toggle",
                "Toggle tree node",
                SafetyClass::ReadOnly,
            ),
            ("near.inspector.up", "Inspector up", SafetyClass::ReadOnly),
            (
                "near.inspector.down",
                "Inspector down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.inspector.home",
                "Inspector start",
                SafetyClass::ReadOnly,
            ),
            ("near.inspector.end", "Inspector end", SafetyClass::ReadOnly),
            (
                "near.inspector.page-up",
                "Previous inspector page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.inspector.page-down",
                "Next inspector page",
                SafetyClass::ReadOnly,
            ),
            ("near.tasks.up", "Task up", SafetyClass::ReadOnly),
            ("near.tasks.down", "Task down", SafetyClass::ReadOnly),
            ("near.tasks.first", "First task", SafetyClass::ReadOnly),
            ("near.tasks.last", "Last task", SafetyClass::ReadOnly),
            (
                "near.tasks.page-up",
                "Previous task page",
                SafetyClass::ReadOnly,
            ),
            (
                "near.tasks.page-down",
                "Next task page",
                SafetyClass::ReadOnly,
            ),
            ("near.tasks.cancel", "Cancel task", SafetyClass::Confirmable),
            ("near.tasks.retry", "Retry task", SafetyClass::Confirmable),
            (
                "near.task.cancel",
                "Request task cancellation",
                SafetyClass::Confirmable,
            ),
            (
                "near.task.retry",
                "Request task retry",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.scroll-up",
                "Terminal scroll up",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.scroll-down",
                "Terminal scroll down",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.copy-mode",
                "Terminal copy mode",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.normal-mode",
                "Terminal normal mode",
                SafetyClass::ReadOnly,
            ),
            (
                "near.terminal.send-key",
                "Send terminal key",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.interrupt",
                "Interrupt terminal process",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.eof",
                "Send terminal end of file",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.clear",
                "Clear terminal",
                SafetyClass::Confirmable,
            ),
            (
                "near.terminal.input",
                "Terminal input",
                SafetyClass::Confirmable,
            ),
            (
                "near.dialog.next",
                "Next dialog field",
                SafetyClass::ReadOnly,
            ),
            (
                "near.dialog.previous",
                "Previous dialog field",
                SafetyClass::ReadOnly,
            ),
            (
                "near.dialog.accept",
                "Accept dialog",
                SafetyClass::Confirmable,
            ),
            ("near.app.quit", "Quit", SafetyClass::Confirmable),
            (
                "near.app.force-quit",
                "Emergency quit",
                SafetyClass::Destructive,
            ),
        ];
        for (id, title, safety) in commands {
            self.registry
                .register(StaticCommand(CommandDescriptor {
                    id: CommandId::from(id),
                    title: title.to_owned(),
                    description: title.to_owned(),
                    category: vec!["Near".to_owned()],
                    safety,
                    arguments: command_arguments(id),
                }))
                .expect("built-in command IDs are unique");
        }
    }

    fn focused_panel(&self) -> &CollectionSurface {
        match self.focused {
            FocusedPanel::Left => &self.left,
            FocusedPanel::Right => &self.right,
        }
    }

    fn focused_panel_mut(&mut self) -> &mut CollectionSurface {
        self.panel_mut(self.focused)
    }

    fn panel_type(&self, panel: FocusedPanel) -> PanelType {
        match panel {
            FocusedPanel::Left => self.left_panel_type,
            FocusedPanel::Right => self.right_panel_type,
        }
    }

    fn set_panel_type(&mut self, panel: FocusedPanel, panel_type: PanelType) {
        match panel {
            FocusedPanel::Left => self.left_panel_type = panel_type,
            FocusedPanel::Right => self.right_panel_type = panel_type,
        }
    }

    fn panel(&self, panel: FocusedPanel) -> &CollectionSurface {
        match panel {
            FocusedPanel::Left => &self.left,
            FocusedPanel::Right => &self.right,
        }
    }

    fn toggle_focused_panel_type(&mut self, panel_type: PanelType) {
        let next = if self.panel_type(self.focused) == panel_type {
            PanelType::File
        } else {
            panel_type
        };
        self.set_panel_type(self.focused, next);
        self.status = format!(
            "{} panel type: {}",
            self.focus_name(),
            panel_type_label(next)
        );
    }

    fn set_sort_mode(&mut self, mode: SortMode) {
        self.focused_panel_mut().set_sort_mode(mode);
        self.report_sort_state();
        self.refresh_quick_view();
    }

    fn report_sort_state(&mut self) {
        self.status = format!(
            "{} panel sort: {}",
            self.focus_name(),
            self.focused_panel().sort_state().indicator()
        );
        self.refresh_quick_view();
    }

    fn set_panel_view_mode(&mut self, invocation: &CommandInvocation) {
        let Some(id) = invocation
            .arguments
            .get("id")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Panel view mode ID is required".clone_into(&mut self.status);
            return;
        };
        let Some(mode) = self.settings.panel_modes.mode(id).cloned() else {
            self.status = format!("Unknown panel view mode: {id}");
            return;
        };
        let label = mode.label.clone();
        self.focused_panel_mut().set_view_mode(mode);
        self.overlay = None;
        self.status = format!("{} panel view mode: {label}", self.focus_name());
    }

    fn save_panel_selection(&mut self) {
        let resources = self.focused_panel().selected_resources();
        let count = resources.len();
        match self.focused {
            FocusedPanel::Left => self.left_saved_selection = resources,
            FocusedPanel::Right => self.right_saved_selection = resources,
        }
        self.status = format!("Saved {count} selected item(s)");
    }

    fn restore_panel_selection(&mut self) {
        let resources = match self.focused {
            FocusedPanel::Left => self.left_saved_selection.clone(),
            FocusedPanel::Right => self.right_saved_selection.clone(),
        };
        let count = self.focused_panel_mut().restore_selection(&resources);
        self.status = format!("Restored {count} selected item(s)");
    }

    fn compare_panel_folders(&mut self, invocation: &CommandInvocation) {
        let policy = match folder_comparison_policy(invocation) {
            Ok(policy) => policy,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let result = compare_folders(&self.left, &self.right, policy);
        self.left.restore_selection(&result.left);
        self.right.restore_selection(&result.right);
        self.overlay = None;
        self.status = format!(
            "Compared folders: {} unique left, {} unique right, {} differing, {} equal; {} selected",
            result.unique_left,
            result.unique_right,
            result.differing_pairs,
            result.equal_pairs,
            result.selected_count()
        );
        self.refresh_quick_view();
    }

    fn panel_mut(&mut self, panel: FocusedPanel) -> &mut CollectionSurface {
        match panel {
            FocusedPanel::Left => &mut self.left,
            FocusedPanel::Right => &mut self.right,
        }
    }

    fn listing_state(&self, panel: FocusedPanel) -> Option<&ListingState> {
        match panel {
            FocusedPanel::Left => self.left_listing.as_ref(),
            FocusedPanel::Right => self.right_listing.as_ref(),
        }
    }

    fn listing_state_mut(&mut self, panel: FocusedPanel) -> Option<&mut ListingState> {
        match panel {
            FocusedPanel::Left => self.left_listing.as_mut(),
            FocusedPanel::Right => self.right_listing.as_mut(),
        }
    }

    fn replace_listing_state(&mut self, panel: FocusedPanel, state: ListingState) {
        let slot = match panel {
            FocusedPanel::Left => &mut self.left_listing,
            FocusedPanel::Right => &mut self.right_listing,
        };
        if let Some(active) = slot {
            active.cancel();
        }
        *slot = Some(state);
    }

    fn clear_listing_state(&mut self, panel: FocusedPanel) {
        let slot = match panel {
            FocusedPanel::Left => &mut self.left_listing,
            FocusedPanel::Right => &mut self.right_listing,
        };
        if let Some(mut active) = slot.take() {
            active.cancel();
        }
    }

    fn start_listing(
        &mut self,
        panel: FocusedPanel,
        provider: &Arc<dyn ResourceProvider>,
        location: &Location,
    ) {
        let retained =
            (self.panel(panel).location() == location).then(|| self.panel(panel).state_snapshot());
        let filter_active = self
            .active_filters
            .get(&panel)
            .is_some_and(|filters| !filters.is_empty());
        self.panel_mut(panel).set_filter_active(filter_active);
        self.record_folder_location(
            provider.id(),
            location.clone(),
            provider.location_label(location),
        );
        self.generation = ListingGeneration(self.generation.0.saturating_add(1));
        let generation = self.generation;
        self.replace_listing_state(
            panel,
            ListingState {
                generation,
                location: location.clone(),
                tasks: Vec::new(),
                loaded: 0,
                retained,
            },
        );
        self.status = format!("Loading {}…", location.as_str());
        self.schedule_listing_page(panel, generation, location, provider, None);
    }

    fn schedule_listing_page(
        &mut self,
        panel: FocusedPanel,
        generation: ListingGeneration,
        location: &Location,
        provider: &Arc<dyn ResourceProvider>,
        continuation: Option<String>,
    ) {
        let task_provider = Arc::clone(provider);
        let task_location = location.clone();
        let active_filters = self.active_filters.get(&panel).cloned().unwrap_or_default();
        let filter_catalog = self.filter_catalog.clone();
        match self.tasks.spawn(move |cancellation| {
            let mut page = block_on(task_provider.list(
                &task_location,
                ListRequest {
                    generation,
                    continuation,
                    page_size: 256,
                    cancellation: cancellation.clone(),
                },
            ));
            if let Ok(page) = &mut page
                && !active_filters.is_empty()
            {
                for entry in &mut page.entries {
                    if cancellation.is_cancelled() {
                        break;
                    }
                    match block_on(task_provider.stat(&entry.resource)) {
                        Ok(metadata) => entry.metadata = metadata,
                        Err(error) => {
                            entry
                                .metadata
                                .field_errors
                                .insert("filter-metadata".to_owned(), error.to_string());
                        }
                    }
                }
                page.entries
                    .retain(|entry| filter_catalog.matches(&active_filters, &entry.metadata));
            }
            WorkspaceTaskResult::ListingPage {
                panel,
                generation,
                location: task_location,
                provider: task_provider,
                page,
            }
        }) {
            Ok(task) => {
                self.track_task(&task, "provider-list");
                if let Some(state) = self.listing_state_mut(panel)
                    && state.generation == generation
                    && state.location == *location
                {
                    state.tasks.push(task);
                } else {
                    task.cancel();
                }
            }
            Err(error) => self.status = format!("Cannot queue listing: {error}"),
        }
    }

    fn schedule_metadata_hydration(
        &mut self,
        panel: FocusedPanel,
        generation: ListingGeneration,
        location: Location,
        provider: Arc<dyn ResourceProvider>,
        resources: Vec<ResourceRef>,
    ) {
        if resources.is_empty() {
            return;
        }
        match self.tasks.spawn(move |cancellation| {
            let mut results = Vec::with_capacity(resources.len());
            for resource in resources {
                if cancellation.is_cancelled() {
                    break;
                }
                let result = block_on(provider.stat(&resource)).map_err(|error| error.to_string());
                results.push((resource, result));
            }
            WorkspaceTaskResult::MetadataHydration {
                panel,
                generation,
                location,
                results,
            }
        }) {
            Ok(task) => {
                self.track_task(&task, "provider-metadata");
                if let Some(state) = self.listing_state_mut(panel)
                    && state.generation == generation
                {
                    state.tasks.push(task);
                } else {
                    task.cancel();
                }
            }
            Err(error) => self.status = format!("Cannot queue metadata hydration: {error}"),
        }
    }

    fn focus_name(&self) -> &'static str {
        ["left", "right"][self.focused as usize]
    }

    fn open_current(&mut self) {
        let Some(item) = self.focused_panel().current().cloned() else {
            return;
        };
        if let Some(MetadataValue::String(text)) = item
            .metadata
            .extensions
            .get("near.temporary-panel.arbitrary-text")
        {
            self.replace_command_text(text);
            self.status = "Copied Temporary Panel text to the command line".to_owned();
            return;
        }
        match self.providers.mount(&item.resource) {
            Ok(Some((provider, location))) => {
                self.navigate_collection(&provider, &location);
                return;
            }
            Ok(None) => {}
            Err(error) => {
                self.status = format!("Cannot open container: {error}");
                return;
            }
        }
        if matches!(
            item.metadata.kind,
            ResourceKind::Directory | ResourceKind::Package
        ) {
            let provider = if is_parent_entry(&item) {
                self.providers
                    .for_location(&item.resource.location)
                    .or_else(|| self.providers.get(&item.resource.provider))
            } else {
                self.providers.get(&item.resource.provider)
            };
            let Some(provider) = provider else {
                self.status = format!("No provider registered for {}", item.resource.provider);
                return;
            };
            self.navigate_collection(&provider, &item.resource.location);
        } else {
            self.request_external_tool(ExternalAction::Open);
        }
    }

    fn open_view_by_policy(&mut self) {
        match self.settings.viewer.open_policy {
            ResourceOpenPolicy::Internal => {
                let Some(item) = self.focused_panel().current().cloned() else {
                    "No current resource".clone_into(&mut self.status);
                    return;
                };
                if is_parent_entry(&item) {
                    "The parent entry is navigation-only".clone_into(&mut self.status);
                    return;
                }
                if matches!(
                    item.metadata.kind,
                    ResourceKind::Directory | ResourceKind::Package
                ) {
                    "The internal viewer requires a non-container resource"
                        .clone_into(&mut self.status);
                    return;
                }
                self.open_viewer(item);
            }
            ResourceOpenPolicy::External => self.request_external_tool(ExternalAction::View),
            ResourceOpenPolicy::Association => self.show_association_menu(),
        }
    }

    fn request_external_tool(&mut self, action: ExternalAction) {
        let Some(item) = self.focused_panel().current() else {
            "No current resource".clone_into(&mut self.status);
            return;
        };
        if is_parent_entry(item) {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return;
        }
        let resource = item.resource.clone();
        let Some(resolver) = &self.external_tools else {
            "No external tool resolver is configured".clone_into(&mut self.status);
            return;
        };
        match resolver.resolve_explained(action, &resource) {
            Ok(resolution) => self.queue_external_resolution(resolution),
            Err(error) => self.status = error,
        }
    }

    fn show_association_menu(&mut self) {
        let Some(resource) = self
            .focused_panel()
            .current()
            .filter(|entry| !is_parent_entry(entry))
            .map(|entry| entry.resource.clone())
        else {
            "No current resource".clone_into(&mut self.status);
            return;
        };
        let Some(resolver) = &self.external_tools else {
            "No external handler resolver is configured".clone_into(&mut self.status);
            return;
        };
        let mut items = Vec::new();
        for action in [
            ExternalAction::View,
            ExternalAction::Edit,
            ExternalAction::Execute,
        ] {
            if let Ok(alternatives) = resolver.alternatives(action, &resource) {
                items.extend(alternatives.into_iter().map(|resolution| {
                    let mode = external_mode_label(resolution.invocation.mode);
                    MenuItem {
                        label: format!(
                            "{} — {}",
                            external_action_label(action),
                            resolution.handler_id
                        ),
                        description: format!("{mode}; {}", resolution.explanation),
                        command: CommandInvocation {
                            id: CommandId::from("near.resource.association-run"),
                            arguments: BTreeMap::from([
                                (
                                    "action".to_owned(),
                                    near_core::CommandValue::String(
                                        external_action_name(action).to_owned(),
                                    ),
                                ),
                                (
                                    "handler".to_owned(),
                                    near_core::CommandValue::String(resolution.handler_id),
                                ),
                            ]),
                        },
                        enabled: true,
                    }
                }));
            }
        }
        if items.is_empty() {
            "No view, edit, or execute association matches the current resource"
                .clone_into(&mut self.status);
            return;
        }
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.file-associations",
            "File Associations",
            items,
        )));
    }

    fn run_association(&mut self, invocation: &CommandInvocation) {
        let Some(action) = invocation
            .arguments
            .get("action")
            .and_then(near_core::CommandValue::as_str)
            .and_then(parse_external_action)
        else {
            "Association action is invalid".clone_into(&mut self.status);
            return;
        };
        let Some(handler) = invocation
            .arguments
            .get("handler")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Association handler is missing".clone_into(&mut self.status);
            return;
        };
        let Some(resource) = self
            .focused_panel()
            .current()
            .filter(|entry| !is_parent_entry(entry))
            .map(|entry| entry.resource.clone())
        else {
            "No current resource".clone_into(&mut self.status);
            return;
        };
        let Some(resolver) = &self.external_tools else {
            "No external handler resolver is configured".clone_into(&mut self.status);
            return;
        };
        match resolver.resolve_named(action, &resource, handler) {
            Ok(resolution) => {
                self.overlay = None;
                self.queue_external_resolution(resolution);
            }
            Err(error) => self.status = error,
        }
    }

    fn show_description_dialog(&mut self) {
        let targets = self.canonical_target_entries();
        if targets.is_empty() {
            "No resource is available to describe".clone_into(&mut self.status);
            return;
        }
        let initial = if targets.len() == 1 {
            resource_description(&targets[0].metadata).unwrap_or_default()
        } else {
            String::new()
        };
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            "near-fm.resource-description",
            format!("Description — {} item(s)", targets.len()),
            vec![DialogField {
                id: "description".to_owned(),
                label: "Description (blank removes)".to_owned(),
                value: initial,
                required: false,
                secret: false,
            }],
            CommandInvocation {
                id: CommandId::from("near.resource.description-confirmed"),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn update_descriptions(&mut self, invocation: &CommandInvocation) {
        let description = invocation
            .arguments
            .get("description")
            .and_then(near_core::CommandValue::as_str)
            .map(str::trim)
            .filter(|description| !description.is_empty())
            .map(str::to_owned);
        let targets = self.canonical_targets();
        if targets.is_empty() {
            "No resource is available to describe".clone_into(&mut self.status);
            return;
        }
        let mut updates = Vec::with_capacity(targets.len());
        for resource in targets {
            let Some(provider) = self.providers.get(&resource.provider) else {
                self.status = format!("No provider registered for {}", resource.provider);
                return;
            };
            updates.push((provider, resource));
        }
        let count = updates.len();
        self.overlay = None;
        match self.tasks.spawn(move |cancellation| {
            let result = updates.into_iter().try_for_each(|(provider, resource)| {
                if cancellation.is_cancelled() {
                    return Err("description update cancelled".to_owned());
                }
                block_on(provider.set_description(&resource, description.clone()))
                    .map_err(|error| error.to_string())
            });
            WorkspaceTaskResult::DescriptionUpdated { count, result }
        }) {
            Ok(task) => {
                self.track_task(&task, "description-update");
                self.status = format!("Updating descriptions for {count} resource(s)…");
            }
            Err(error) => self.status = format!("Cannot queue description update: {error}"),
        }
    }

    fn open_folder_description(&mut self, edit: bool) {
        let location = self.focused_panel().location().clone();
        let Some(provider) = self.providers.for_location(&location) else {
            self.status = format!("No provider for {}", location.as_str());
            return;
        };
        let resource = match block_on(provider.folder_description(&location, edit)) {
            Ok(Some(resource)) => resource,
            Ok(None) => {
                "No configured folder-description file exists".clone_into(&mut self.status);
                return;
            }
            Err(error) => {
                self.status = format!("Folder description unavailable: {error}");
                return;
            }
        };
        let metadata = match block_on(provider.stat(&resource)) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.status = format!("Cannot inspect folder description: {error}");
                return;
            }
        };
        let item = CollectionEntry {
            resource,
            details: "folder description".to_owned(),
            metadata,
            selected: false,
        };
        if edit {
            self.open_editor_resource(item);
        } else {
            self.open_viewer(item);
        }
    }

    fn user_menu_context(&self) -> Option<UserMenuContext> {
        let focused_panel = self.focused_panel();
        let peer_panel = self.command_panel(true);
        let focused_entry = focused_panel
            .current()
            .filter(|entry| !is_parent_entry(entry))?;
        let focused = UserMenuResource {
            resource: focused_entry.resource.clone(),
            metadata: focused_entry.metadata.clone(),
        };
        let peer = peer_panel
            .current()
            .filter(|entry| !is_parent_entry(entry))
            .map(|entry| UserMenuResource {
                resource: entry.resource.clone(),
                metadata: entry.metadata.clone(),
            });
        let mut selected = focused_panel
            .entries()
            .iter()
            .filter(|entry| entry.selected && !is_parent_entry(entry))
            .map(|entry| UserMenuResource {
                resource: entry.resource.clone(),
                metadata: entry.metadata.clone(),
            })
            .collect::<Vec<_>>();
        if selected.is_empty() {
            selected.push(focused.clone());
        }
        Some(UserMenuContext {
            focused,
            focused_location: focused_panel.location().as_str().to_owned(),
            peer,
            peer_location: Some(peer_panel.location().as_str().to_owned()),
            selected,
        })
    }

    fn show_user_menu(&mut self, scope: UserMenuScope) {
        let Some(context) = self.user_menu_context() else {
            "No current resource for the user menu".clone_into(&mut self.status);
            return;
        };
        let items = self
            .user_menus
            .entries(scope)
            .iter()
            .map(|entry| {
                let mode = match entry.invocation {
                    UserMenuInvocationTemplate::Argv { .. } => "structured argv",
                    UserMenuInvocationTemplate::Shell { .. } => "EXPLICIT SHELL",
                };
                MenuItem {
                    label: format!("{}  {}", entry.key, entry.label),
                    description: format!("{mode}; {}", entry.description),
                    command: CommandInvocation {
                        id: CommandId::from("near.user-menu.run"),
                        arguments: BTreeMap::from([
                            (
                                "scope".to_owned(),
                                near_core::CommandValue::String(scope.as_str().to_owned()),
                            ),
                            (
                                "entry".to_owned(),
                                near_core::CommandValue::String(entry.id.clone()),
                            ),
                        ]),
                    },
                    enabled: entry.predicate.matches_metadata(&context.focused.metadata),
                }
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            self.status = format!("No {} user-menu entries are configured", scope.as_str());
            return;
        }
        let title = match scope {
            UserMenuScope::Global => "Global User Menu",
            UserMenuScope::Local => "Local User Menu",
        };
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            format!("near-fm.user-menu.{}", scope.as_str()),
            title,
            items,
        )));
    }

    fn run_user_menu(&mut self, invocation: &CommandInvocation) {
        let scope = match invocation
            .arguments
            .get("scope")
            .and_then(near_core::CommandValue::as_str)
        {
            Some("global") => UserMenuScope::Global,
            Some("local") => UserMenuScope::Local,
            _ => {
                "User-menu scope is invalid".clone_into(&mut self.status);
                return;
            }
        };
        let Some(entry) = invocation
            .arguments
            .get("entry")
            .and_then(near_core::CommandValue::as_str)
        else {
            "User-menu entry is missing".clone_into(&mut self.status);
            return;
        };
        let Some(context) = self.user_menu_context() else {
            "No current resource for the user menu".clone_into(&mut self.status);
            return;
        };
        match self.user_menus.resolve(scope, entry, &context) {
            Ok(resolution) => {
                self.overlay = None;
                self.queue_external_resolution(resolution);
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn queue_external_resolution(&mut self, resolution: near_core::ExternalResolution) {
        let mode = external_mode_label(resolution.invocation.mode);
        self.status = format!(
            "Suspending via {} ({mode}) — {}",
            resolution.handler_id, resolution.explanation
        );
        self.pending_external = Some(resolution.invocation);
    }

    fn insert_current_name(&mut self, peer: bool) {
        let panel = self.command_panel(peer);
        let Some(item) = panel.current() else {
            "No current resource to insert".clone_into(&mut self.status);
            return;
        };
        if is_parent_entry(item) {
            "The parent entry has no insertable name".clone_into(&mut self.status);
            return;
        }
        let argument = self.quote_command_text(&item.metadata.name);
        self.insert_command_text(&argument);
    }

    fn insert_selected_names(&mut self) {
        let names = self
            .focused_panel()
            .entries()
            .iter()
            .filter(|entry| entry.selected && !is_parent_entry(entry))
            .map(|entry| self.quote_command_text(&entry.metadata.name))
            .collect::<Vec<_>>();
        if names.is_empty() {
            "No selected resource names to insert".clone_into(&mut self.status);
            return;
        }
        self.insert_command_text(&names.join(" "));
    }

    fn insert_panel_path(&mut self, peer: bool) {
        let Some(resolver) = &self.command_line_arguments else {
            "No command-line path resolver is configured".clone_into(&mut self.status);
            return;
        };
        match resolver.location_argument(self.command_panel(peer).location()) {
            Ok(argument) => self.insert_command_text(&argument),
            Err(error) => self.status = error,
        }
    }

    fn insert_current_path(&mut self, peer: bool) {
        let Some(resolver) = &self.command_line_arguments else {
            "No command-line path resolver is configured".clone_into(&mut self.status);
            return;
        };
        let Some(item) = self.command_panel(peer).current() else {
            "No current resource path to insert".clone_into(&mut self.status);
            return;
        };
        if is_parent_entry(item) {
            "The parent entry has no insertable path".clone_into(&mut self.status);
            return;
        }
        match resolver.resource_argument(&item.resource) {
            Ok(argument) => self.insert_command_text(&argument),
            Err(error) => self.status = error,
        }
    }

    fn command_panel(&self, peer: bool) -> &CollectionSurface {
        if !peer {
            return self.focused_panel();
        }
        match self.focused {
            FocusedPanel::Left => &self.right,
            FocusedPanel::Right => &self.left,
        }
    }

    fn quote_command_text(&self, value: &str) -> String {
        self.command_line_arguments
            .as_ref()
            .map_or_else(|| shell_quote(value), |resolver| resolver.quote_text(value))
    }

    fn submit_command_line(&mut self) {
        let Some(command) = self.command_line.commit() else {
            return;
        };
        self.persist_command_history();
        if self.dispatch_command_prefix(&command) {
            return;
        }
        #[cfg(feature = "embedded-pty")]
        if self.embedded_pty_enabled {
            match self.ensure_embedded_terminal() {
                Ok(session) => match session.submit_line(&command) {
                    Ok(()) => {
                        self.activate_terminal_screen();
                        self.status = format!("Terminal: {command}");
                    }
                    Err(error) => self.status = format!("Terminal input failed: {error}"),
                },
                Err(error) => self.status = format!("Cannot start terminal: {error}"),
            }
            return;
        }
        if self.command_line_task.is_some() {
            "A command is already running".clone_into(&mut self.status);
            return;
        }
        let Some(executor) = self.command_line_executor.clone() else {
            "No command-line executor is configured".clone_into(&mut self.status);
            return;
        };
        let location = self.focused_panel().location().clone();
        let task_command = command.clone();
        match self.tasks.spawn(move |_| WorkspaceTaskResult::CommandLine {
            command: task_command.clone(),
            result: executor.execute(&location, &task_command),
        }) {
            Ok(task) => {
                self.track_task(&task, "command-line");
                self.status = format!("Running: {command}");
                self.command_line_task = Some(task);
            }
            Err(error) => self.status = format!("Cannot queue command: {error}"),
        }
    }

    fn dispatch_command_prefix(&mut self, command: &str) -> bool {
        let Some((prefix, arguments)) = command.split_once(':') else {
            return false;
        };
        let Some(registration) = self.command_prefixes.get(prefix).cloned() else {
            return false;
        };
        match registration.owner {
            CommandPrefixOwner::TemporaryPanel => self.dispatch_temporary_panel_prefix(arguments),
            CommandPrefixOwner::Provider(provider_id) => {
                let Some(provider) = self.providers.get(&provider_id) else {
                    self.status =
                        format!("Command prefix {prefix}: provider {provider_id} is unavailable");
                    return true;
                };
                let current = self.focused_panel().location().clone();
                match provider.resolve_command_prefix(prefix, arguments, Some(&current)) {
                    Ok(location) => {
                        self.navigate_collection(&provider, &location);
                        self.status = format!(
                            "Command prefix {prefix}: navigated with provider {provider_id}"
                        );
                    }
                    Err(error) => self.status = format!("Command prefix {prefix}: {error}"),
                }
            }
            CommandPrefixOwner::Extension {
                extension,
                command,
                argument,
            } => {
                self.status = format!("Command prefix {prefix}: invoking extension {extension}");
                self.dispatch_with_keymap(
                    &CommandInvocation {
                        id: command,
                        arguments: BTreeMap::from([(
                            argument,
                            near_core::CommandValue::String(arguments.to_owned()),
                        )]),
                    },
                    None,
                );
            }
        }
        true
    }

    fn show_command_prefixes(&mut self) {
        let body = if self.command_prefixes.is_empty() {
            "No command prefixes are registered".to_owned()
        } else {
            self.command_prefixes
                .iter()
                .map(|(name, registration)| {
                    let owner = match &registration.owner {
                        CommandPrefixOwner::TemporaryPanel => "Near FM Temporary Panel".to_owned(),
                        CommandPrefixOwner::Provider(provider) => {
                            format!("provider {provider}")
                        }
                        CommandPrefixOwner::Extension { extension, .. } => {
                            format!("extension {extension}")
                        }
                    };
                    format!("{name}:\n  {owner}\n  {}", registration.description)
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        self.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
            "near-fm.command-prefixes",
            "Command Prefixes",
            body,
        ))));
    }

    fn show_filter_menu(&mut self) {
        let active = self
            .active_filters
            .get(&self.focused)
            .cloned()
            .unwrap_or_default();
        let mut items = self
            .filter_catalog
            .filters()
            .iter()
            .map(|filter| {
                let group = filter
                    .mask_group
                    .as_ref()
                    .map_or_else(String::new, |group| format!("; mask group: {group}"));
                MenuItem {
                    label: format!(
                        "{} {} {}",
                        if active.contains(&filter.id) {
                            "√"
                        } else {
                            " "
                        },
                        filter.mode.marker(),
                        filter.label
                    ),
                    description: format!("{} filter{group}", filter.mode.marker()),
                    command: CommandInvocation {
                        id: CommandId::from("near.filters.toggle"),
                        arguments: BTreeMap::from([(
                            "filter".to_owned(),
                            near_core::CommandValue::String(filter.id.clone()),
                        )]),
                    },
                    enabled: true,
                }
            })
            .collect::<Vec<_>>();
        items.push(MenuItem {
            label: "Backspace  Clear current panel filters".to_owned(),
            description: "Disable every saved filter on the focused panel".to_owned(),
            command: CommandInvocation {
                id: CommandId::from("near.filters.clear"),
                arguments: BTreeMap::new(),
            },
            enabled: !active.is_empty(),
        });
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.filters",
            format!("{} Panel Filters", capitalize(self.focus_name())),
            items,
        )));
    }

    fn toggle_filter(&mut self, invocation: &CommandInvocation) {
        let Some(id) = invocation
            .arguments
            .get("filter")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Filter ID is missing".clone_into(&mut self.status);
            return;
        };
        if !self.filter_catalog.contains(id) {
            self.status = format!("Unknown saved filter: {id}");
            return;
        }
        let active = self.active_filters.entry(self.focused).or_default();
        let enabled = if let Some(index) = active.iter().position(|active| active == id) {
            active.remove(index);
            false
        } else {
            active.push(id.to_owned());
            true
        };
        self.overlay = None;
        self.status = format!(
            "{} filter {id} on the {} panel",
            if enabled { "Enabled" } else { "Disabled" },
            self.focus_name().to_lowercase()
        );
        self.refresh_collections();
    }

    fn clear_filters(&mut self) {
        let count = self
            .active_filters
            .remove(&self.focused)
            .map_or(0, |filters| filters.len());
        self.overlay = None;
        self.status = format!("Cleared {count} filter(s) from the focused panel");
        self.refresh_collections();
    }

    fn start_apply_command(&mut self, invocation: &CommandInvocation) {
        if self.apply_command_task.is_some() {
            "An applied command is already running".clone_into(&mut self.status);
            return;
        }
        let Some(executor) = self.command_line_executor.clone() else {
            "No command-line executor is configured".clone_into(&mut self.status);
            return;
        };
        let Some(resolver) = self.command_line_arguments.clone() else {
            "No structured command argument resolver is configured".clone_into(&mut self.status);
            return;
        };
        let template_source = invocation
            .arguments
            .get("template")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or_default();
        let template = match ApplyCommandTemplate::parse(template_source) {
            Ok(template) => template,
            Err(error) => {
                self.status = format!("Invalid apply-command template: {error}");
                return;
            }
        };
        let mode = match invocation
            .arguments
            .get("mode")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or("sequential")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "sequential" | "each" => ApplyCommandMode::Sequential,
            "batch" | "once" => ApplyCommandMode::Batch,
            _ => {
                "Apply-command mode must be 'sequential' or 'batch'".clone_into(&mut self.status);
                return;
            }
        };
        if mode == ApplyCommandMode::Batch && !template.has_resources_placeholder() {
            "Batch mode requires a {resources} placeholder".clone_into(&mut self.status);
            return;
        }
        let continue_on_error = match parse_yes_no(
            invocation
                .arguments
                .get("continue_on_error")
                .and_then(near_core::CommandValue::as_str)
                .unwrap_or("yes"),
            "Continue errors",
        ) {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let entries = self.canonical_target_entries();
        if entries.is_empty() {
            "No selectable resource for Apply command".clone_into(&mut self.status);
            return;
        }
        let location = self.focused_panel().location().clone();
        let panel_argument = match resolver.location_argument(&location) {
            Ok(argument) => argument,
            Err(error) => {
                self.status = format!("Cannot resolve panel path for Apply command: {error}");
                return;
            }
        };
        let mut targets = Vec::with_capacity(entries.len());
        let mut resources = Vec::with_capacity(entries.len());
        for entry in entries {
            let resource_argument = match resolver.resource_argument(&entry.resource) {
                Ok(argument) => argument,
                Err(error) => {
                    self.status = format!(
                        "Cannot resolve Apply-command source {}: {error}",
                        entry.metadata.name
                    );
                    return;
                }
            };
            resources.push(entry.resource);
            targets.push(ApplyCommandTarget {
                label: entry.metadata.name.clone(),
                resource_argument,
                name_argument: resolver.quote_text(&entry.metadata.name),
            });
        }
        let planned = match mode {
            ApplyCommandMode::Sequential => targets
                .iter()
                .zip(resources.iter())
                .map(|(target, resource)| {
                    (
                        vec![resource.clone()],
                        vec![target.label.clone()],
                        template.expand_sequential(target, &panel_argument),
                    )
                })
                .collect::<Vec<_>>(),
            ApplyCommandMode::Batch => {
                let command = match template.expand_batch(&targets, &panel_argument) {
                    Ok(command) => command,
                    Err(error) => {
                        self.status = format!("Invalid batch template: {error}");
                        return;
                    }
                };
                vec![(
                    resources,
                    targets.into_iter().map(|target| target.label).collect(),
                    command,
                )]
            }
        };
        let total = planned.len();
        let task_location = location.clone();
        match self.tasks.spawn(move |cancellation| {
            let mut executions = Vec::with_capacity(planned.len());
            let mut cancelled = false;
            for (sources, labels, command) in planned {
                if cancellation.is_cancelled() {
                    cancelled = true;
                    break;
                }
                let result = executor.execute(&task_location, &command);
                let failed = command_result_failed(&result);
                executions.push(ApplyCommandExecution {
                    sources,
                    labels,
                    command,
                    result,
                });
                if failed && !continue_on_error {
                    break;
                }
            }
            WorkspaceTaskResult::ApplyCommand(ApplyCommandSummary {
                planned: total,
                executions,
                cancelled,
            })
        }) {
            Ok(task) => {
                self.track_visible_task(
                    &task,
                    "apply-command",
                    TaskRecord::running(
                        &task,
                        "Apply command",
                        Some(u64::try_from(total).unwrap_or(u64::MAX)),
                        format!("Running {total} command invocation(s)"),
                    ),
                );
                self.apply_command_task = Some(task);
                self.overlay = None;
                self.status = format!("Applying command in {total} invocation(s)");
            }
            Err(error) => self.status = format!("Cannot queue Apply command: {error}"),
        }
    }

    fn finish_apply_command(&mut self, task: u64, summary: ApplyCommandSummary) {
        let failed = summary
            .executions
            .iter()
            .filter(|execution| command_result_failed(&execution.result))
            .count();
        let succeeded = summary.executions.len().saturating_sub(failed);
        let mut body = String::new();
        for execution in &summary.executions {
            let outcome = if command_result_failed(&execution.result) {
                "FAILED"
            } else {
                "OK"
            };
            body.push_str(&format!(
                "[{outcome}] {}\nSources:\n{}\n$ {}\n",
                execution.labels.join(", "),
                execution
                    .sources
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
                execution.command
            ));
            match &execution.result {
                Ok(output) => {
                    body.push_str(&format!("Exit: {:?}\n", output.exit_code));
                    if !output.stdout.is_empty() {
                        body.push_str("stdout:\n");
                        body.push_str(&output.stdout);
                        if !output.stdout.ends_with('\n') {
                            body.push('\n');
                        }
                    }
                    if !output.stderr.is_empty() {
                        body.push_str("stderr:\n");
                        body.push_str(&output.stderr);
                        if !output.stderr.ends_with('\n') {
                            body.push('\n');
                        }
                    }
                }
                Err(error) => body.push_str(&format!("Error: {error}\n")),
            }
            body.push('\n');
        }
        if summary.cancelled {
            body.push_str("Cancelled before all planned invocations completed.\n");
        } else if summary.executions.len() < summary.planned {
            body.push_str("Stopped after the first failed invocation.\n");
        }
        if let Some(record) = self.task_records.get_mut(&task) {
            record.completed = u64::try_from(summary.executions.len()).unwrap_or(u64::MAX);
            record.state = if summary.cancelled {
                TaskState::Cancelled
            } else if failed > 0 {
                TaskState::Failed
            } else {
                TaskState::Completed
            };
            record.message = format!(
                "{succeeded} succeeded, {failed} failed, {} not run",
                summary.planned.saturating_sub(summary.executions.len())
            );
        }
        self.refresh_collections();
        self.status = format!(
            "Apply command: {succeeded} succeeded, {failed} failed, {} not run",
            summary.planned.saturating_sub(summary.executions.len())
        );
        self.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
            "near-fm.apply-command-results",
            "Apply command results",
            body,
        ))));
    }

    fn finish_cancelled_apply_command(&mut self, task: u64) {
        if let Some(record) = self.task_records.get_mut(&task) {
            record.state = TaskState::Cancelled;
            "Cancelled before execution".clone_into(&mut record.message);
        }
        "Apply command cancelled before execution".clone_into(&mut self.status);
    }

    fn show_command_history(&mut self) {
        self.overlay = Some(Overlay::CommandHistory(CommandHistorySurface::new(
            self.command_line.entries().iter().rev().cloned(),
        )));
    }

    fn persist_command_history(&mut self) {
        let Some(store) = &self.command_history_store else {
            return;
        };
        if let Err(error) = store.save(self.command_line.entries()) {
            self.status = format!("Cannot save command history: {error}");
        }
    }

    fn show_resource_history(&mut self, kind: ResourceHistoryKind) {
        let entries = match kind {
            ResourceHistoryKind::Viewed => &self.resource_history.viewed,
            ResourceHistoryKind::Edited => &self.resource_history.edited,
        };
        self.overlay = Some(Overlay::ResourceHistory(ResourceHistorySurface::new(
            kind, entries,
        )));
    }

    fn show_resource_history_menu(&mut self) {
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.resource-history-menu",
            "View/Edit History",
            vec![
                MenuItem {
                    label: "Viewed files".to_owned(),
                    description: format!(
                        "{} persistent entries",
                        self.resource_history.viewed.len()
                    ),
                    command: CommandInvocation {
                        id: CommandId::from("near.history.viewed-show"),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                },
                MenuItem {
                    label: "Edited files".to_owned(),
                    description: format!(
                        "{} persistent entries",
                        self.resource_history.edited.len()
                    ),
                    command: CommandInvocation {
                        id: CommandId::from("near.history.edited-show"),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                },
            ],
        )));
    }

    fn record_resource_history(
        &mut self,
        kind: ResourceHistoryKind,
        resource: ResourceRef,
        label: String,
    ) {
        let entries = match kind {
            ResourceHistoryKind::Viewed => &mut self.resource_history.viewed,
            ResourceHistoryKind::Edited => &mut self.resource_history.edited,
        };
        let existing = entries
            .iter()
            .position(|entry| entry.resource == resource)
            .map(|index| entries.remove(index));
        let existed = existing.is_some();
        let mut entry =
            existing.unwrap_or_else(|| ResourceHistoryEntry::new(resource, label.clone()));
        entry.label = label;
        if existed {
            entry.use_count = entry.use_count.saturating_add(1);
        }
        entry.last_error = None;
        entries.push(entry);
        self.trim_resource_history(kind);
        self.persist_resource_history();
    }

    fn trim_resource_history(&mut self, kind: ResourceHistoryKind) {
        let entries = match kind {
            ResourceHistoryKind::Viewed => &mut self.resource_history.viewed,
            ResourceHistoryKind::Edited => &mut self.resource_history.edited,
        };
        while entries.iter().filter(|entry| !entry.locked).count()
            > self.resource_history.max_unlocked
        {
            let Some(index) = entries.iter().position(|entry| !entry.locked) else {
                break;
            };
            entries.remove(index);
        }
    }

    fn open_resource_history_entry(&mut self, invocation: &CommandInvocation) {
        let Some(kind) = resource_history_kind(invocation) else {
            "History entry has no valid kind".clone_into(&mut self.status);
            return;
        };
        let Some(provider) = invocation
            .arguments
            .get("provider")
            .and_then(near_core::CommandValue::as_str)
            .map(ProviderId::from)
        else {
            "History entry has no provider".clone_into(&mut self.status);
            return;
        };
        let Some(location) = invocation
            .arguments
            .get("location")
            .and_then(near_core::CommandValue::as_str)
            .map(Location::new)
        else {
            "History entry has no location".clone_into(&mut self.status);
            return;
        };
        let label = invocation
            .arguments
            .get("label")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or_else(|| location.as_str())
            .to_owned();
        if self.providers.get(&provider).is_none() {
            let error = format!("Provider {provider} is unavailable");
            self.mark_resource_history_error(kind, &provider, &location, Some(error.clone()));
            self.status = error;
            self.show_resource_history(kind);
            return;
        }
        self.mark_resource_history_error(kind, &provider, &location, None);
        let item = CollectionEntry {
            resource: ResourceRef { provider, location },
            metadata: ResourceMetadata {
                name: label,
                kind: ResourceKind::File,
                ..ResourceMetadata::default()
            },
            details: "history".to_owned(),
            selected: false,
        };
        match kind {
            ResourceHistoryKind::Viewed => self.open_viewer(item),
            ResourceHistoryKind::Edited => self.open_editor_resource(item),
        }
    }

    fn toggle_resource_history_lock(&mut self, invocation: &CommandInvocation) {
        let Some(kind) = resource_history_kind(invocation) else {
            return;
        };
        let Some(provider) = invocation
            .arguments
            .get("provider")
            .and_then(near_core::CommandValue::as_str)
        else {
            return;
        };
        let Some(location) = invocation
            .arguments
            .get("location")
            .and_then(near_core::CommandValue::as_str)
        else {
            return;
        };
        let entries = match kind {
            ResourceHistoryKind::Viewed => &mut self.resource_history.viewed,
            ResourceHistoryKind::Edited => &mut self.resource_history.edited,
        };
        if let Some(entry) = entries.iter_mut().find(|entry| {
            entry.resource.provider.as_str() == provider
                && entry.resource.location.as_str() == location
        }) {
            entry.locked = !entry.locked;
            self.status = format!(
                "{} history entry {}",
                kind.as_str(),
                if entry.locked { "locked" } else { "unlocked" }
            );
            self.persist_resource_history();
        }
        self.show_resource_history(kind);
    }

    fn clear_resource_history(&mut self, invocation: &CommandInvocation) {
        let Some(kind) = resource_history_kind(invocation) else {
            return;
        };
        let entries = match kind {
            ResourceHistoryKind::Viewed => &mut self.resource_history.viewed,
            ResourceHistoryKind::Edited => &mut self.resource_history.edited,
        };
        let before = entries.len();
        entries.retain(|entry| entry.locked);
        let removed = before.saturating_sub(entries.len());
        self.persist_resource_history();
        self.status = format!(
            "Cleared {removed} unlocked {} history entries",
            kind.as_str()
        );
        self.show_resource_history(kind);
    }

    fn mark_resource_history_error(
        &mut self,
        kind: ResourceHistoryKind,
        provider: &ProviderId,
        location: &Location,
        error: Option<String>,
    ) {
        let entries = match kind {
            ResourceHistoryKind::Viewed => &mut self.resource_history.viewed,
            ResourceHistoryKind::Edited => &mut self.resource_history.edited,
        };
        if let Some(entry) = entries.iter_mut().find(|entry| {
            &entry.resource.provider == provider && &entry.resource.location == location
        }) {
            entry.last_error = error;
            self.persist_resource_history();
        }
    }

    fn persist_resource_history(&mut self) {
        let Some(store) = &self.resource_history_store else {
            return;
        };
        if let Err(error) = store.save(&self.resource_history) {
            self.status = format!("Cannot save resource history: {error}");
        }
    }

    fn finish_command_line(&mut self, command: String, result: Result<CommandLineOutput, String>) {
        match result {
            Ok(output) => {
                let mut body = output.stdout;
                if !output.stderr.is_empty() {
                    if !body.is_empty() && !body.ends_with('\n') {
                        body.push('\n');
                    }
                    body.push_str(&output.stderr);
                }
                if body.is_empty() {
                    body = format!("Process exited with code {:?}", output.exit_code);
                }
                self.status = format!("Command exited with code {:?}: {command}", output.exit_code);
                self.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
                    "near-fm.command-output",
                    format!("$ {command}"),
                    body,
                ))));
                self.refresh_collections();
            }
            Err(error) => self.status = format!("Command failed: {error}"),
        }
    }

    fn finish_temporary_panel_command(
        &mut self,
        task: u64,
        panel: FocusedPanel,
        slot: u8,
        modes: (bool, bool),
        command: String,
        result: Result<CommandLineOutput, String>,
    ) {
        let (replace, allow_arbitrary) = modes;
        if let Some(record) = self.task_records.get_mut(&task) {
            record.finish(result.is_ok(), "Temporary-panel command finished");
        }
        match result {
            Ok(output) => {
                let result = self.ingest_temporary_panel_text(
                    panel,
                    slot,
                    &output.stdout,
                    replace,
                    allow_arbitrary,
                );
                let (added, rejected) = match result {
                    Ok(counts) => counts,
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                };
                let stderr = if output.stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!("; stderr: {}", output.stderr.trim())
                };
                self.status = format!(
                    "Temporary panel {slot}: command exited {:?}, added {added}, rejected {rejected}{stderr}: {command}",
                    output.exit_code
                );
            }
            Err(error) => {
                self.status = format!("Temporary panel {slot}: command failed: {error}: {command}");
            }
        }
    }

    fn show_handler_diagnostics(&mut self) {
        let Some(resource) = self
            .focused_panel()
            .current()
            .map(|item| item.resource.clone())
        else {
            "No current resource".clone_into(&mut self.status);
            return;
        };
        let Some(resolver) = &self.external_tools else {
            "No external handler resolver is configured".clone_into(&mut self.status);
            return;
        };
        let mut sections = Vec::new();
        for action in [
            ExternalAction::View,
            ExternalAction::Edit,
            ExternalAction::Execute,
        ] {
            let diagnostic = resolver
                .diagnose(action, &resource)
                .unwrap_or_else(|error| error);
            let alternatives = resolver
                .alternatives(action, &resource)
                .map(|alternatives| {
                    alternatives
                        .iter()
                        .enumerate()
                        .map(|(index, resolution)| {
                            format!(
                                "{}. {} [{}]",
                                index + 1,
                                resolution.handler_id,
                                external_mode_label(resolution.invocation.mode)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_else(|error| format!("No alternatives: {error}"));
            sections.push(format!(
                "{}\n{}\nAlternatives in configured order:\n{}",
                external_action_label(action),
                diagnostic,
                alternatives
            ));
        }
        self.overlay = Some(Overlay::Message {
            title: "Handler diagnostics".to_owned(),
            body: sections.join("\n\n"),
        });
    }

    fn toggle_macro_recording(&mut self) {
        if self.macro_recorder.is_recording() {
            match self.macro_recorder.finish() {
                Ok(recorded) => {
                    let count = recorded.steps.len();
                    self.last_macro = Some(recorded.clone());
                    self.macro_catalog.insert(recorded.id.clone(), recorded);
                    if self.persist_macros() {
                        self.status = format!("Semantic macro recorded: {count} commands");
                    }
                }
                Err(error) => self.status = error.to_string(),
            }
            return;
        }
        let condition = MacroCondition {
            required_contexts: self.active_contexts(),
            ..MacroCondition::default()
        };
        match self.macro_recorder.start(SemanticMacro {
            id: "near.macro.last-recording".to_owned(),
            title: "Last recorded macro".to_owned(),
            binding: None,
            trust: MacroTrust::Untrusted,
            when: condition,
            steps: Vec::new(),
        }) {
            Ok(()) => "Recording semantic commands…".clone_into(&mut self.status),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn play_last_macro(&mut self) {
        let Some(recorded) = self.last_macro.clone() else {
            "No semantic macro has been recorded".clone_into(&mut self.status);
            return;
        };
        self.replay_macro(&recorded);
    }

    fn replay_macro(&mut self, recorded: &SemanticMacro) {
        let engine = self.macro_engine.clone();
        self.macro_replaying = true;
        let result = {
            let mut host = WorkspaceMacroHost { workspace: self };
            engine.replay(recorded, &mut host)
        };
        self.macro_replaying = false;
        match result {
            Ok(report) => {
                self.status = format!(
                    "Replayed {} commands; {} conditionally skipped",
                    report.completed, report.skipped_conditions
                );
            }
            Err(error) => self.status = format!("Macro replay stopped: {error}"),
        }
    }

    fn show_last_macro(&mut self) {
        let Some(recorded) = self.last_macro.clone() else {
            "No semantic macro has been recorded".clone_into(&mut self.status);
            return;
        };
        let document = MacroDocument {
            schema_version: MACRO_SCHEMA_VERSION,
            macros: vec![recorded],
        };
        match toml::to_string_pretty(&document) {
            Ok(body) => {
                self.overlay = Some(Overlay::Message {
                    title: "Last semantic macro".to_owned(),
                    body,
                });
            }
            Err(error) => self.status = format!("Cannot inspect macro: {error}"),
        }
    }

    fn macro_binding_invocation(&self, stroke: &KeyStroke) -> Option<CommandInvocation> {
        let binding = format_key_stroke(stroke);
        self.macro_catalog.values().find_map(|semantic_macro| {
            semantic_macro
                .binding
                .as_ref()
                .is_some_and(|candidate| candidate == &binding)
                .then(|| CommandInvocation {
                    id: "near.macro.play".into(),
                    arguments: BTreeMap::from([(
                        "id".to_owned(),
                        CommandValue::String(semantic_macro.id.clone()),
                    )]),
                })
        })
    }

    fn show_macro_manager(&mut self) {
        if self.macro_catalog.is_empty() {
            self.overlay = Some(Overlay::Message {
                title: "Macros".to_owned(),
                body: "No macros are configured. Press Ctrl+. to record one.".to_owned(),
            });
            return;
        }
        let items = self
            .macro_catalog
            .values()
            .map(|semantic_macro| MenuItem {
                label: semantic_macro.title.clone(),
                description: format!(
                    "{} steps · {} · {}",
                    semantic_macro.steps.len(),
                    semantic_macro.binding.as_deref().unwrap_or("unbound"),
                    macro_condition_summary(&semantic_macro.when)
                ),
                command: macro_invocation("near.macro.actions", &semantic_macro.id),
                enabled: true,
            })
            .collect();
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.macros",
            "Macro Manager",
            items,
        )));
    }

    fn show_macro_actions(&mut self, invocation: &CommandInvocation) {
        let Some(id) = macro_id(invocation) else {
            "Macro action requires an ID".clone_into(&mut self.status);
            return;
        };
        let Some(semantic_macro) = self.macro_catalog.get(id) else {
            self.status = format!("Unknown macro {id}");
            return;
        };
        let title = semantic_macro.title.clone();
        let items = [
            (
                "&Play",
                "Replay through normal command validation",
                "near.macro.play",
            ),
            (
                "&Edit",
                "Edit title trust and execution conditions",
                "near.macro.edit",
            ),
            (
                "&Bind",
                "Assign or remove one canonical key binding",
                "near.macro.bind",
            ),
            (
                "&Diagnose",
                "Explain conditions steps availability and safety",
                "near.macro.diagnose",
            ),
            (
                "&Delete",
                "Remove this macro from the catalog",
                "near.macro.delete",
            ),
        ]
        .into_iter()
        .map(|(label, description, command)| MenuItem {
            label: label.to_owned(),
            description: description.to_owned(),
            command: macro_invocation(command, id),
            enabled: true,
        })
        .collect();
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            format!("near-fm.macro-actions.{id}"),
            format!("Macro: {title}"),
            items,
        )));
    }

    fn play_macro(&mut self, invocation: &CommandInvocation) {
        let Some(id) = macro_id(invocation) else {
            "Macro replay requires an ID".clone_into(&mut self.status);
            return;
        };
        let Some(semantic_macro) = self.macro_catalog.get(id).cloned() else {
            self.status = format!("Unknown macro {id}");
            return;
        };
        self.overlay = None;
        self.last_macro = Some(semantic_macro.clone());
        self.replay_macro(&semantic_macro);
    }

    fn show_macro_edit_dialog(&mut self, invocation: &CommandInvocation) {
        let Some(id) = macro_id(invocation) else {
            return;
        };
        let Some(semantic_macro) = self.macro_catalog.get(id) else {
            return;
        };
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            format!("near-fm.macro-edit.{id}"),
            format!("Edit Macro: {}", semantic_macro.title),
            vec![
                macro_dialog_field("id", "ID", id, true),
                macro_dialog_field("title", "Title", &semantic_macro.title, true),
                macro_dialog_field(
                    "trust",
                    "Trust: untrusted | trusted",
                    match semantic_macro.trust {
                        MacroTrust::Untrusted => "untrusted",
                        MacroTrust::Trusted => "trusted",
                    },
                    true,
                ),
                macro_dialog_field(
                    "contexts",
                    "Required contexts (comma separated)",
                    &semantic_macro
                        .when
                        .required_contexts
                        .iter()
                        .map(ContextId::as_str)
                        .collect::<Vec<_>>()
                        .join(","),
                    false,
                ),
                macro_dialog_field(
                    "capabilities",
                    "Required capabilities (comma separated)",
                    &semantic_macro
                        .when
                        .required_capabilities
                        .iter()
                        .map(CapabilityId::as_str)
                        .collect::<Vec<_>>()
                        .join(","),
                    false,
                ),
                macro_dialog_field(
                    "current",
                    "Current resource: any | present | absent",
                    presence_label(semantic_macro.when.current_resource),
                    true,
                ),
                macro_dialog_field(
                    "peer",
                    "Peer surface: any | present | absent",
                    presence_label(semantic_macro.when.peer_surface),
                    true,
                ),
            ],
            CommandInvocation {
                id: "near.macro.edit-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn confirm_macro_edit(&mut self, invocation: &CommandInvocation) {
        let id = argument_text(invocation, "id");
        let Some(existing) = self.macro_catalog.get(id).cloned() else {
            self.status = format!("Unknown macro {id}");
            return;
        };
        let title = argument_text(invocation, "title");
        if title.is_empty() {
            "Macro title cannot be empty".clone_into(&mut self.status);
            return;
        }
        let trust = match argument_text(invocation, "trust") {
            "untrusted" => MacroTrust::Untrusted,
            "trusted" => MacroTrust::Trusted,
            _ => {
                "Macro trust must be untrusted or trusted".clone_into(&mut self.status);
                return;
            }
        };
        let current_resource = match parse_presence(argument_text(invocation, "current")) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Current resource: {error}");
                return;
            }
        };
        let peer_surface = match parse_presence(argument_text(invocation, "peer")) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Peer surface: {error}");
                return;
            }
        };
        let required_contexts = argument_text(invocation, "contexts")
            .split(',')
            .map(str::trim)
            .filter(|context| !context.is_empty())
            .map(ContextId::from)
            .collect();
        let required_capabilities = argument_text(invocation, "capabilities")
            .split(',')
            .map(str::trim)
            .filter(|capability| !capability.is_empty())
            .map(CapabilityId::from)
            .collect();
        let updated = SemanticMacro {
            title: title.to_owned(),
            trust,
            when: MacroCondition {
                required_contexts,
                required_capabilities,
                current_resource,
                peer_surface,
            },
            ..existing
        };
        self.macro_catalog.insert(id.to_owned(), updated.clone());
        if self.last_macro.as_ref().is_some_and(|last| last.id == id) {
            self.last_macro = Some(updated);
        }
        self.overlay = None;
        if self.persist_macros() {
            self.status = format!("Updated macro {id}");
        }
    }

    fn show_macro_bind_dialog(&mut self, invocation: &CommandInvocation) {
        let Some(id) = macro_id(invocation) else {
            return;
        };
        let Some(semantic_macro) = self.macro_catalog.get(id) else {
            return;
        };
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            format!("near-fm.macro-bind.{id}"),
            format!("Bind Macro: {}", semantic_macro.title),
            vec![
                macro_dialog_field("id", "ID", id, true),
                macro_dialog_field(
                    "binding",
                    "Key (blank removes binding)",
                    semantic_macro.binding.as_deref().unwrap_or_default(),
                    false,
                ),
            ],
            CommandInvocation {
                id: "near.macro.bind-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn confirm_macro_bind(&mut self, invocation: &CommandInvocation) {
        let id = argument_text(invocation, "id");
        let raw = argument_text(invocation, "binding");
        let binding = if raw.is_empty() {
            None
        } else {
            let stroke = match parse_key_stroke(raw) {
                Ok(stroke) => stroke,
                Err(error) => {
                    self.status = format!("Invalid macro binding: {error}");
                    return;
                }
            };
            Some(format_key_stroke(&stroke))
        };
        if let Some(binding) = &binding
            && let Some(conflict) = self.macro_catalog.values().find(|semantic_macro| {
                semantic_macro.id != id && semantic_macro.binding.as_ref() == Some(binding)
            })
        {
            self.status = format!("Binding {binding} is already assigned to {}", conflict.id);
            return;
        }
        let Some(semantic_macro) = self.macro_catalog.get_mut(id) else {
            self.status = format!("Unknown macro {id}");
            return;
        };
        semantic_macro.binding.clone_from(&binding);
        if self.last_macro.as_ref().is_some_and(|last| last.id == id) {
            self.last_macro = Some(semantic_macro.clone());
        }
        self.overlay = None;
        if self.persist_macros() {
            self.status = binding.map_or_else(
                || format!("Removed binding from {id}"),
                |binding| format!("Bound {id} to {binding}"),
            );
        }
    }

    fn show_macro_delete_dialog(&mut self, invocation: &CommandInvocation) {
        let Some(id) = macro_id(invocation) else {
            return;
        };
        let Some(semantic_macro) = self.macro_catalog.get(id) else {
            return;
        };
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            format!("near-fm.macro-delete.{id}"),
            format!("Delete Macro: {}", semantic_macro.title),
            vec![
                macro_dialog_field("id", "ID", id, true),
                macro_dialog_field("confirm", "Type yes to delete", "no", true),
            ],
            CommandInvocation {
                id: "near.macro.delete-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn confirm_macro_delete(&mut self, invocation: &CommandInvocation) {
        let id = argument_text(invocation, "id");
        if !matches!(argument_text(invocation, "confirm"), "yes" | "true") {
            "Macro deletion requires yes".clone_into(&mut self.status);
            return;
        }
        if self.macro_catalog.remove(id).is_none() {
            self.status = format!("Unknown macro {id}");
            return;
        }
        if self.last_macro.as_ref().is_some_and(|last| last.id == id) {
            self.last_macro = self.macro_catalog.values().next_back().cloned();
        }
        self.overlay = None;
        if self.persist_macros() {
            self.status = format!("Deleted macro {id}");
        }
    }

    fn diagnose_macro(&mut self, invocation: &CommandInvocation) {
        let Some(id) = macro_id(invocation) else {
            return;
        };
        let Some(semantic_macro) = self.macro_catalog.get(id).cloned() else {
            return;
        };
        let engine = self.macro_engine.clone();
        let diagnostic = {
            let host = WorkspaceMacroHost { workspace: self };
            engine.diagnose(&semantic_macro, &host)
        };
        let mut lines = vec![
            format!("ID: {}", semantic_macro.id),
            format!("Title: {}", semantic_macro.title),
            format!(
                "Binding: {}",
                semantic_macro.binding.as_deref().unwrap_or("unbound")
            ),
            format!("Trust: {:?}", semantic_macro.trust),
            format!(
                "Condition: {}",
                macro_condition_summary(&semantic_macro.when)
            ),
            format!("Available now: {}", diagnostic.macro_available),
        ];
        for step in diagnostic.steps {
            let validation = step.error.map_or_else(
                || {
                    format!(
                        "{:?} · authorized={}",
                        step.safety.expect("validated macro step has safety"),
                        step.authorized
                    )
                },
                |error| format!("unavailable: {error}"),
            );
            lines.push(format!(
                "{}. {} · condition={} · {}",
                step.step + 1,
                step.command,
                step.condition_matches,
                validation
            ));
        }
        self.overlay = Some(Overlay::Message {
            title: format!("Macro Diagnostics: {}", semantic_macro.title),
            body: lines.join("\n"),
        });
    }

    fn persist_macros(&mut self) -> bool {
        let document = MacroDocument {
            schema_version: MACRO_SCHEMA_VERSION,
            macros: self.macro_catalog.values().cloned().collect(),
        };
        if let Err(error) = document.validate() {
            self.status = format!("Cannot save macro catalog: {error}");
            return false;
        }
        let Some(store) = &self.macro_store else {
            return true;
        };
        if let Err(error) = store.save(&document) {
            self.status = format!("Cannot save macro catalog: {error}");
            return false;
        }
        true
    }

    fn dispatch_temporary_panel_prefix(&mut self, arguments: &str) {
        let panel = self.focused;
        let command_location = if self.active_temporary_panel_slot(self.focused).is_some() {
            let peer = opposite_panel(self.focused);
            let peer_location = self.panel(peer).location();
            self.providers.for_location(peer_location).map_or_else(
                || self.focused_panel().location().clone(),
                |_| peer_location.clone(),
            )
        } else {
            self.focused_panel().location().clone()
        };
        let mut slot = self.active_temporary_panel_slot(self.focused).unwrap_or(0);
        let mut safe_mode = None;
        let mut allow_arbitrary = None;
        let mut menu_mode = false;
        let mut full_screen = None;
        let mut mode = "append";
        let mut remainder = arguments.trim();
        while let Some(option) = remainder
            .strip_prefix('+')
            .or_else(|| remainder.strip_prefix('-'))
        {
            let enabled = remainder.starts_with('+');
            let boundary = option
                .find(|character: char| {
                    character.is_whitespace()
                        || character == '+'
                        || character == '-'
                        || character == '"'
                        || character == '<'
                })
                .unwrap_or(option.len());
            let name = &option[..boundary];
            match name.to_ascii_lowercase().as_str() {
                "safe" => safe_mode = Some(enabled),
                "any" => allow_arbitrary = Some(enabled),
                "replace" => mode = if enabled { "replace" } else { "append" },
                "menu" => menu_mode = enabled,
                "full" => full_screen = Some(enabled),
                value if value.len() == 1 && value.as_bytes()[0].is_ascii_digit() => {
                    slot = value.as_bytes()[0] - b'0';
                }
                _ => {
                    self.status = format!("tmp: unknown option {name}");
                    return;
                }
            }
            remainder = option[boundary..].trim_start();
        }
        if menu_mode {
            let path = remainder
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(remainder);
            self.show_temporary_panel_list_menu(path);
            return;
        }
        self.open_temporary_panel(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([(
                "slot".to_owned(),
                CommandValue::Integer(i64::from(slot)),
            )]),
        });
        if let Some(safe_mode) = safe_mode {
            let temporary = self
                .temporary_panels
                .get_mut(&slot)
                .expect("opened temporary panel slot must exist");
            temporary.safe_mode = safe_mode;
            self.replace_temporary_panel_surface(self.focused, slot);
        }
        if let Some(allow_arbitrary) = allow_arbitrary {
            let temporary = self
                .temporary_panels
                .get_mut(&slot)
                .expect("opened temporary panel slot must exist");
            temporary.allow_arbitrary = allow_arbitrary;
        }
        if let Some(full_screen) = full_screen {
            let temporary = self
                .temporary_panels
                .get_mut(&slot)
                .expect("opened temporary panel slot must exist");
            temporary.full_screen = full_screen;
        }
        self.persist_temporary_panels();
        if remainder.is_empty() {
            let temporary = &self.temporary_panels[&slot];
            let modes = [
                temporary.safe_mode.then_some("safe"),
                temporary.allow_arbitrary.then_some("any"),
                temporary.full_screen.then_some("full-screen"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            self.status = format!(
                "Temporary panel {slot} opened{}",
                if modes.is_empty() {
                    String::new()
                } else {
                    format!(" in {} mode", modes.join(", "))
                }
            );
            return;
        }
        if let Some(command) = remainder.strip_prefix('<') {
            self.start_temporary_panel_command(
                panel,
                slot,
                command_location,
                command.trim().to_owned(),
                mode == "replace",
                self.temporary_panels[&slot].allow_arbitrary,
            );
            return;
        }
        let path = remainder
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(remainder);
        self.import_temporary_panel(&CommandInvocation {
            id: CommandId::from("near.temp-panel.import-confirmed"),
            arguments: BTreeMap::from([
                ("path".to_owned(), CommandValue::String(path.to_owned())),
                ("mode".to_owned(), CommandValue::String(mode.to_owned())),
                (
                    "any".to_owned(),
                    CommandValue::Boolean(self.temporary_panels[&slot].allow_arbitrary),
                ),
            ]),
        });
    }

    fn start_temporary_panel_command(
        &mut self,
        panel: FocusedPanel,
        slot: u8,
        location: Location,
        command: String,
        replace: bool,
        allow_arbitrary: bool,
    ) {
        if command.is_empty() {
            "tmp: command output capture requires a command after <".clone_into(&mut self.status);
            return;
        }
        let Some(executor) = self.command_line_executor.clone() else {
            "tmp: no command-line executor is configured".clone_into(&mut self.status);
            return;
        };
        let task_command = command.clone();
        match self.tasks.spawn(move |_| {
            let result = executor.execute(&location, &task_command);
            WorkspaceTaskResult::TemporaryPanelCommand {
                panel,
                slot,
                replace,
                allow_arbitrary,
                command: task_command,
                result,
            }
        }) {
            Ok(task) => {
                self.track_visible_task(
                    &task,
                    "temporary-panel-command",
                    TaskRecord::running(
                        &task,
                        "Temporary-panel command",
                        Some(1),
                        format!("Running {command}"),
                    ),
                );
                self.status = format!("Temporary panel {slot}: running <{command}");
            }
            Err(error) => self.status = format!("Cannot queue tmp command: {error}"),
        }
    }

    fn navigate_collection(&mut self, provider: &Arc<dyn ResourceProvider>, location: &Location) {
        self.cancel_search(self.focused);
        self.searches.remove(&self.focused);
        self.start_listing(self.focused, provider, location);
    }

    fn show_folder_history(&mut self) {
        self.overlay = Some(Overlay::FolderHistory(FolderHistorySurface::new(
            &self.folder_navigation,
        )));
    }

    fn open_folder_history_entry(&mut self, invocation: &CommandInvocation) {
        let Some(provider_id) = invocation
            .arguments
            .get("provider")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Folder history entry has no provider".clone_into(&mut self.status);
            return;
        };
        let Some(location) = invocation
            .arguments
            .get("location")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Folder history entry has no location".clone_into(&mut self.status);
            return;
        };
        let provider_id = ProviderId::from(provider_id);
        let location = Location::new(location);
        let Some(provider) = self.providers.get(&provider_id) else {
            let error = format!("Provider {provider_id} is unavailable");
            self.mark_folder_error(&provider_id, &location, Some(error.clone()));
            self.status = error;
            return;
        };
        self.overlay = None;
        self.navigate_collection(&provider, &location);
    }

    fn toggle_folder_history_lock(&mut self, invocation: &CommandInvocation) {
        let Some(provider) = invocation
            .arguments
            .get("provider")
            .and_then(near_core::CommandValue::as_str)
        else {
            return;
        };
        let Some(location) = invocation
            .arguments
            .get("location")
            .and_then(near_core::CommandValue::as_str)
        else {
            return;
        };
        if let Some(entry) = self.folder_navigation.history.iter_mut().find(|entry| {
            entry.provider.as_str() == provider && entry.location.as_str() == location
        }) {
            entry.locked = !entry.locked;
            self.status = format!(
                "Folder history entry {}",
                if entry.locked { "locked" } else { "unlocked" }
            );
            self.persist_folder_navigation();
        }
        self.show_folder_history();
    }

    fn assign_folder_shortcut(&mut self, invocation: &CommandInvocation) {
        let Some(slot) = command_slot(invocation) else {
            "Folder shortcut slot must be 0 through 9".clone_into(&mut self.status);
            return;
        };
        let location = self.focused_panel().location().clone();
        let Some(provider) = self.providers.for_location(&location) else {
            self.status = format!("No provider for {}", location.as_str());
            return;
        };
        let label = provider.location_label(&location);
        self.folder_navigation.shortcuts[slot] = Some(FolderLocationEntry::new(
            provider.id(),
            location.clone(),
            label.clone(),
        ));
        self.persist_folder_navigation();
        self.status = format!("Assigned folder shortcut {slot}: {label}");
    }

    fn open_folder_shortcut(&mut self, invocation: &CommandInvocation) {
        let Some(slot) = command_slot(invocation) else {
            "Folder shortcut slot must be 0 through 9".clone_into(&mut self.status);
            return;
        };
        let Some(entry) = self.folder_navigation.shortcuts[slot].clone() else {
            self.status = format!("Folder shortcut {slot} is not assigned");
            return;
        };
        self.open_folder_history_entry(&CommandInvocation {
            id: CommandId::from("near.location.history-open"),
            arguments: BTreeMap::from([
                (
                    "provider".to_owned(),
                    near_core::CommandValue::String(entry.provider.to_string()),
                ),
                (
                    "location".to_owned(),
                    near_core::CommandValue::String(entry.location.as_str().to_owned()),
                ),
            ]),
        });
    }

    fn record_folder_location(&mut self, provider: ProviderId, location: Location, label: String) {
        self.folder_navigation
            .history
            .retain(|entry| entry.provider != provider || entry.location != location);
        self.folder_navigation
            .history
            .push(FolderLocationEntry::new(provider, location, label));
        self.trim_folder_history();
        self.persist_folder_navigation();
    }

    fn trim_folder_history(&mut self) {
        while self
            .folder_navigation
            .history
            .iter()
            .filter(|entry| !entry.locked)
            .count()
            > self.folder_navigation.max_unlocked
        {
            let Some(index) = self
                .folder_navigation
                .history
                .iter()
                .position(|entry| !entry.locked)
            else {
                break;
            };
            self.folder_navigation.history.remove(index);
        }
    }

    fn mark_folder_error(
        &mut self,
        provider: &ProviderId,
        location: &Location,
        error: Option<String>,
    ) {
        for entry in &mut self.folder_navigation.history {
            if &entry.provider == provider && &entry.location == location {
                entry.last_error.clone_from(&error);
            }
        }
        for entry in self.folder_navigation.shortcuts.iter_mut().flatten() {
            if &entry.provider == provider && &entry.location == location {
                entry.last_error.clone_from(&error);
            }
        }
        if let Some(Overlay::FolderHistory(surface)) = &mut self.overlay {
            surface.update_error(provider.as_str(), location.as_str(), error.as_deref());
        }
        self.persist_folder_navigation();
    }

    fn persist_folder_navigation(&mut self) {
        let Some(store) = &self.folder_navigation_store else {
            return;
        };
        if let Err(error) = store.save(&self.folder_navigation) {
            self.status = format!("Cannot save folder navigation: {error}");
        }
    }

    fn navigate_provider_location(&mut self, invocation: &CommandInvocation) {
        let target = provider_target(invocation, self.focused);
        let Some(provider_id) = invocation
            .arguments
            .get("provider")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Provider ID is required".clone_into(&mut self.status);
            return;
        };
        let Some(location) = invocation
            .arguments
            .get("location")
            .and_then(near_core::CommandValue::as_str)
        else {
            "Provider location is required".clone_into(&mut self.status);
            return;
        };
        let provider_id = ProviderId::from(provider_id);
        let Some(provider) = self.providers.get(&provider_id) else {
            self.status = format!("Provider {provider_id} is unavailable");
            return;
        };
        if self.panel_type(target) == PanelType::QuickView {
            if let Some(task) = self.quick_view_task.take() {
                task.cancel();
            }
            self.quick_view_requests.cancel();
            self.quick_view = None;
            self.quick_view_replaced = None;
        }
        self.set_panel_type(target, PanelType::File);
        self.overlay = None;
        self.start_listing(target, &provider, &Location::new(location));
        self.status = format!(
            "{} panel: {} via {}",
            capitalize(panel_name(target)),
            location,
            provider_id
        );
    }

    fn disconnect_provider(&mut self) {
        let location = self.focused_panel().location().clone();
        let Some(provider) = self.providers.for_location(&location) else {
            self.status = format!("No provider for {}", location.as_str());
            return;
        };
        match provider.disconnect(&location) {
            Ok(true) => {
                self.status = format!(
                    "Disconnected {}; panel state retained for retry",
                    location.as_str()
                );
            }
            Ok(false) => {
                self.status = format!("{} has no active connection", provider.id());
            }
            Err(error) => self.status = format!("Cannot disconnect {}: {error}", provider.id()),
        }
    }

    fn retry_provider(&mut self) {
        let panel = self.focused;
        let location = self.focused_panel().location().clone();
        let Some(provider) = self.providers.for_location(&location) else {
            self.status = format!("No provider for {}", location.as_str());
            return;
        };
        match provider.reconnect(&location) {
            Ok(true) => {
                self.start_listing(panel, &provider, &location);
                self.status = format!(
                    "Reconnected {} and refreshed the retained panel",
                    provider.id()
                );
            }
            Ok(false) => self.status = format!("{} does not support reconnect", provider.id()),
            Err(error) => self.status = format!("Cannot reconnect {}: {error}", provider.id()),
        }
    }

    fn open_viewer(&mut self, item: CollectionEntry) {
        let Some(provider) = self.providers.get(&item.resource.provider) else {
            self.status = format!("No provider registered for {}", item.resource.provider);
            return;
        };
        let history_resource = item.resource.clone();
        let history_label = item.metadata.name.clone();
        match ViewerSurface::stream(
            "near-fm.viewer",
            item.metadata.name,
            provider,
            item.resource,
            CancellationToken::default(),
        ) {
            Ok(viewer) => {
                self.record_resource_history(
                    ResourceHistoryKind::Viewed,
                    history_resource,
                    history_label,
                );
                self.overlay = Some(Overlay::Surface(Box::new(self.configure_viewer(viewer))));
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn configure_viewer(&self, mut viewer: ViewerSurface) -> ViewerSurface {
        viewer = self.with_viewer_clipboard(viewer);
        if let Some(resource) = viewer.state().current
            && let Some(state) = self
                .viewer_states
                .get(&viewer_state_key(&resource.provider, &resource.location))
            && let Some(state) = self.settings.viewer.filter_state(state.clone())
        {
            viewer.restore_state(&state);
        }
        viewer
    }

    fn with_viewer_clipboard(&self, viewer: ViewerSurface) -> ViewerSurface {
        let viewer = viewer.with_settings(self.settings.viewer);
        if let Some(clipboard) = &self.clipboard {
            viewer.with_clipboard(Arc::clone(clipboard))
        } else {
            viewer
        }
    }

    fn record_viewer_state(&mut self, state: ViewerStateEntry) {
        self.viewer_states
            .insert(viewer_state_key(&state.provider, &state.location), state);
    }

    fn persist_overlay_viewer_state(&mut self) {
        let state = match &self.overlay {
            Some(Overlay::Surface(surface)) => surface.viewer_state(),
            _ => None,
        };
        if let Some(state) = state.and_then(|state| self.settings.viewer.filter_state(state)) {
            self.record_viewer_state(state);
            self.persist_viewer_states();
        }
    }

    fn persist_viewer_states(&mut self) {
        let Some(store) = &self.viewer_state_store else {
            return;
        };
        let entries = self.viewer_states.values().cloned().collect::<Vec<_>>();
        if let Err(error) = store.save(&entries) {
            self.status = format!("Cannot save viewer state: {error}");
        }
    }

    fn open_editor(&mut self) {
        let Some(item) = self.focused_panel().current().cloned() else {
            "No current resource".clone_into(&mut self.status);
            return;
        };
        if is_parent_entry(&item) || item.metadata.kind != ResourceKind::File {
            "The internal editor requires a file".clone_into(&mut self.status);
            return;
        }
        self.open_editor_resource(item);
    }

    fn open_editor_by_policy(&mut self) {
        match self.settings.editor.open_policy {
            ResourceOpenPolicy::Internal => self.open_editor(),
            ResourceOpenPolicy::External => self.request_external_tool(ExternalAction::Edit),
            ResourceOpenPolicy::Association => self.show_association_menu(),
        }
    }

    fn open_editor_resource(&mut self, item: CollectionEntry) {
        let Some(provider) = self.providers.get(&item.resource.provider) else {
            self.status = format!("No provider registered for {}", item.resource.provider);
            return;
        };
        if !provider
            .capabilities(&item.resource)
            .iter()
            .any(|capability| capability.as_str() == "resource.write")
        {
            self.status = format!("{} is read-only", item.metadata.name);
            return;
        }
        if let Some(index) = self
            .editors
            .iter()
            .position(|editor| editor.resource() == &item.resource)
        {
            self.active_editor = Some(index);
            self.overlay = None;
            self.record_resource_history(
                ResourceHistoryKind::Edited,
                item.resource,
                item.metadata.name,
            );
            self.status = format!("Editor: {}", self.editors[index].title());
            return;
        }
        let history_resource = item.resource.clone();
        let history_label = item.metadata.name.clone();
        match EditorSurface::open(
            "near-fm.editor",
            item.metadata.name,
            provider,
            item.resource,
            CancellationToken::default(),
        ) {
            Ok(editor) => {
                let mut editor = editor.with_settings(self.settings.editor);
                let key = editor_resource_key(editor.resource());
                if let Some(position) = self.editor_positions.get(&key) {
                    editor.restore_position(EditorPosition {
                        row: position.row,
                        column: position.column,
                        top: position.top,
                    });
                }
                self.editors.push(editor);
                self.active_editor = Some(self.editors.len() - 1);
                self.record_resource_history(
                    ResourceHistoryKind::Edited,
                    history_resource,
                    history_label,
                );
                self.overlay = None;
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn close_active_editor(&mut self) {
        let Some(index) = self.active_editor.take() else {
            return;
        };
        if index >= self.editors.len() {
            return;
        }
        let editor = self.editors.remove(index);
        self.record_editor_position(&editor);
        self.persist_editor_positions();
        self.overlay = None;
        self.status = format!("Closed editor: {}", editor.title());
    }

    fn show_editor_save_as_dialog(&mut self) {
        let Some(editor) = self.active_editor() else {
            "No active editor".clone_into(&mut self.status);
            return;
        };
        let format = editor.save_format();
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            "near-fm.editor-save-as",
            "Editor Save As",
            vec![
                DialogField {
                    id: "provider".to_owned(),
                    label: "Provider".to_owned(),
                    value: editor.resource().provider.as_str().to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "location".to_owned(),
                    label: "Location".to_owned(),
                    value: editor.resource().location.as_str().to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "encoding".to_owned(),
                    label: "Encoding".to_owned(),
                    value: format.encoding.label().to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "bom".to_owned(),
                    label: "BOM yes/no".to_owned(),
                    value: if format.bom { "yes" } else { "no" }.to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "eol".to_owned(),
                    label: "EOL".to_owned(),
                    value: format.line_ending.label().to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "replace".to_owned(),
                    label: "Create/replace yes".to_owned(),
                    value: "no".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "lossy".to_owned(),
                    label: "Allow lossy yes/no".to_owned(),
                    value: "no".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: "near.editor.save-as-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn confirm_editor_save_as(&mut self, invocation: &CommandInvocation) {
        let value = |name: &str| {
            invocation
                .arguments
                .get(name)
                .and_then(CommandValue::as_str)
                .unwrap_or_default()
        };
        let replace = match parse_yes_no(value("replace"), "Create/replace") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if !replace {
            "Save As requires explicit create/replace confirmation".clone_into(&mut self.status);
            return;
        }
        let provider_id = ProviderId::from(value("provider"));
        let Some(provider) = self.providers.get(&provider_id) else {
            self.status = format!("Provider {provider_id} is unavailable");
            return;
        };
        let Some(encoding) = EditorEncoding::parse(value("encoding")) else {
            "Encoding must be UTF-8, UTF-16LE, UTF-16BE, or Latin-1".clone_into(&mut self.status);
            return;
        };
        let Some(line_ending) = EditorLineEnding::parse(value("eol")) else {
            "EOL must be LF, CRLF, or CR".clone_into(&mut self.status);
            return;
        };
        let resource = ResourceRef {
            provider: provider_id,
            location: Location::new(value("location")),
        };
        let bom = match parse_yes_no(value("bom"), "BOM") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let confirm_lossy = match parse_yes_no(value("lossy"), "Allow lossy") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let format = EditorSaveFormat {
            encoding,
            bom,
            line_ending,
        };
        let outcome = self
            .active_editor_mut()
            .map(|editor| editor.save_as(provider, resource.clone(), format, confirm_lossy));
        match outcome {
            Some(EditorSaveOutcome::Saved) => {
                self.overlay = None;
                self.record_resource_history(
                    ResourceHistoryKind::Edited,
                    resource,
                    value("location").to_owned(),
                );
                "Saved editor document as a new provider resource".clone_into(&mut self.status);
            }
            Some(EditorSaveOutcome::LossyConfirmationRequired) => {
                "Save As would replace unrepresentable characters; set Allow lossy to yes"
                    .clone_into(&mut self.status);
            }
            Some(EditorSaveOutcome::ExternalChange) => self.show_editor_external_change_menu(),
            Some(EditorSaveOutcome::Failed(error)) => {
                self.status = format!("Save As failed: {error}");
            }
            None => "No active editor".clone_into(&mut self.status),
        }
    }

    fn show_editor_external_change_menu(&mut self) {
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.editor-external-change",
            "Resource Changed Externally",
            vec![
                MenuItem {
                    label: "&Reload external version".to_owned(),
                    description: "Discard local edits and reload the provider resource".to_owned(),
                    command: CommandInvocation {
                        id: "near.editor.external-reload".into(),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                },
                MenuItem {
                    label: "&Compare local and external".to_owned(),
                    description: "Open a read-only line comparison".to_owned(),
                    command: CommandInvocation {
                        id: "near.editor.external-compare".into(),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                },
                MenuItem {
                    label: "&Keep local and overwrite".to_owned(),
                    description: "Write local text without the stale version precondition"
                        .to_owned(),
                    command: CommandInvocation {
                        id: "near.editor.external-keep-local".into(),
                        arguments: BTreeMap::new(),
                    },
                    enabled: true,
                },
            ],
        )));
    }

    fn reload_external_editor(&mut self) {
        let result = self.active_editor_mut().map(EditorSurface::reload_external);
        match result {
            Some(Ok(())) => {
                self.overlay = None;
                "Reloaded the externally changed resource".clone_into(&mut self.status);
            }
            Some(Err(error)) => self.status = format!("Reload failed: {error}"),
            None => "No active editor".clone_into(&mut self.status),
        }
    }

    fn compare_external_editor(&mut self) {
        let comparison = self.active_editor().map(EditorSurface::external_comparison);
        match comparison {
            Some(Ok(comparison)) => {
                self.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
                    "near-fm.editor-external-comparison",
                    "External ↔ Local Comparison",
                    comparison,
                ))));
            }
            Some(Err(error)) => self.status = format!("Comparison failed: {error}"),
            None => "No active editor".clone_into(&mut self.status),
        }
    }

    fn keep_local_editor(&mut self) {
        let outcome = self
            .active_editor_mut()
            .map(EditorSurface::force_save_after_external_change);
        match outcome {
            Some(EditorSaveOutcome::Saved) => {
                self.overlay = None;
                "Overwrote the external version with local edits".clone_into(&mut self.status);
            }
            Some(EditorSaveOutcome::Failed(error)) => {
                self.status = format!("Keep-local save failed: {error}");
            }
            Some(EditorSaveOutcome::LossyConfirmationRequired) => self.show_lossy_save_warning(),
            Some(EditorSaveOutcome::ExternalChange) => self.show_editor_external_change_menu(),
            None => "No active editor".clone_into(&mut self.status),
        }
    }

    fn show_lossy_save_warning(&mut self) {
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.editor-lossy-save",
            "Lossy Save Warning",
            vec![MenuItem {
                label: "Save with &replacement characters".to_owned(),
                description: "Unrepresentable characters will be written as ?".to_owned(),
                command: CommandInvocation {
                    id: "near.editor.lossy-save-confirmed".into(),
                    arguments: BTreeMap::new(),
                },
                enabled: true,
            }],
        )));
    }

    fn confirm_lossy_editor_save(&mut self) {
        let outcome = self
            .active_editor_mut()
            .map(EditorSurface::confirm_lossy_save);
        match outcome {
            Some(EditorSaveOutcome::Saved) => {
                self.overlay = None;
                "Saved with explicit lossy conversion".clone_into(&mut self.status);
            }
            Some(EditorSaveOutcome::ExternalChange) => self.show_editor_external_change_menu(),
            Some(EditorSaveOutcome::Failed(error)) => {
                self.status = format!("Lossy save failed: {error}");
            }
            Some(EditorSaveOutcome::LossyConfirmationRequired) => {
                "Lossy confirmation was not applied".clone_into(&mut self.status);
            }
            None => "No active editor".clone_into(&mut self.status),
        }
    }

    fn persist_active_editor_position(&mut self) {
        let Some(index) = self.active_editor else {
            return;
        };
        let Some(editor) = self.editors.get(index) else {
            return;
        };
        let resource = editor.resource().clone();
        let position = editor.position();
        let entry = EditorPositionEntry {
            provider: resource.provider,
            location: resource.location,
            row: position.row,
            column: position.column,
            top: position.top,
        };
        self.editor_positions
            .insert(editor_position_key(&entry.provider, &entry.location), entry);
        self.persist_editor_positions();
    }

    fn record_editor_position(&mut self, editor: &EditorSurface) {
        let position = editor.position();
        let entry = EditorPositionEntry {
            provider: editor.resource().provider.clone(),
            location: editor.resource().location.clone(),
            row: position.row,
            column: position.column,
            top: position.top,
        };
        self.editor_positions
            .insert(editor_position_key(&entry.provider, &entry.location), entry);
    }

    fn persist_editor_positions(&mut self) {
        let Some(store) = &self.editor_position_store else {
            return;
        };
        let entries = self.editor_positions.values().cloned().collect::<Vec<_>>();
        if let Err(error) = store.save(&entries) {
            self.status = format!("Cannot save editor positions: {error}");
        }
    }

    fn toggle_quick_view(&mut self) {
        let target = opposite_panel(self.focused);
        if self.panel_type(target) == PanelType::QuickView {
            if let Some(task) = self.quick_view_task.take() {
                task.cancel();
            }
            self.quick_view_requests.cancel();
            self.quick_view = None;
            self.quick_view_interactive = false;
            let restored = self
                .quick_view_replaced
                .take()
                .filter(|(panel, _)| *panel == target)
                .map_or(PanelType::File, |(_, previous)| previous);
            self.set_panel_type(target, restored);
            "Quick view closed".clone_into(&mut self.status);
        } else {
            if let Some((panel, previous)) = self.quick_view_replaced.take() {
                self.set_panel_type(panel, previous);
            }
            let previous = self.panel_type(target);
            self.quick_view_replaced = Some((target, previous));
            self.set_panel_type(target, PanelType::QuickView);
            self.quick_view = Some(ViewerSurface::text(
                "near-fm.quick-view",
                "Quick view",
                "Loading…",
            ));
            self.refresh_quick_view();
        }
    }

    fn toggle_quick_view_control(&mut self) {
        if self.quick_view_panel().is_none() || self.quick_view.is_none() {
            "Open quick view with Ctrl+Q first".clone_into(&mut self.status);
            return;
        }
        self.quick_view_interactive = !self.quick_view_interactive;
        self.status = if self.quick_view_interactive {
            "Quick-view controls active — viewer keys and search apply to the preview"
        } else {
            "Returned to file panel navigation"
        }
        .to_owned();
    }

    fn refresh_quick_view(&mut self) {
        let Some(target) = self.quick_view_panel() else {
            return;
        };
        let source = opposite_panel(target);
        if let Some(task) = self.quick_view_task.take() {
            task.cancel();
        }
        let ticket = self.quick_view_requests.begin();
        let Some(item) = self.panel(source).current().cloned() else {
            self.quick_view = Some(ViewerSurface::text(
                "near-fm.quick-view",
                "Quick view",
                "No current resource",
            ));
            return;
        };
        if matches!(
            item.metadata.kind,
            ResourceKind::Directory | ResourceKind::Package
        ) {
            let Some(provider) = self.providers.get(&item.resource.provider) else {
                self.quick_view = Some(ViewerSurface::text(
                    "near-fm.quick-view",
                    item.metadata.name,
                    "No provider registered",
                ));
                return;
            };
            let title = item.metadata.name;
            let location = item.resource.location;
            let task_provider = Arc::clone(&provider);
            let task_location = location.clone();
            let task_ticket = ticket.clone();
            let generation = self.generation;
            match self.tasks.spawn(move |cancellation| {
                let page = block_on(task_provider.list(
                    &task_location,
                    ListRequest {
                        generation,
                        continuation: None,
                        page_size: 256,
                        cancellation,
                    },
                ));
                WorkspaceTaskResult::QuickViewDirectory {
                    ticket: task_ticket,
                    title,
                    location: task_location,
                    page,
                }
            }) {
                Ok(task) => {
                    self.track_task(&task, "provider-directory-preview");
                    self.quick_view_task = Some(task);
                    "Loading directory quick view…".clone_into(&mut self.status);
                }
                Err(error) => {
                    self.quick_view = Some(ViewerSurface::text(
                        "near-fm.quick-view",
                        "Quick view",
                        format!("Cannot queue directory quick view: {error}"),
                    ));
                }
            }
            return;
        }
        let Some(provider) = self.providers.get(&item.resource.provider) else {
            self.quick_view = Some(ViewerSurface::text(
                "near-fm.quick-view",
                item.metadata.name,
                "No provider registered",
            ));
            return;
        };
        let title = item.metadata.name;
        let resource = item.resource;
        let task_provider = Arc::clone(&provider);
        let task_resource = resource.clone();
        let task_ticket = ticket.clone();
        match self.tasks.spawn(move |cancellation| {
            let stream = block_on(task_provider.open(
                &task_resource,
                OpenRequest {
                    offset: 0,
                    length: 64 * 1024,
                    cancellation,
                },
            ));
            WorkspaceTaskResult::QuickView {
                ticket: task_ticket,
                title,
                provider: task_provider,
                resource: task_resource,
                stream,
            }
        }) {
            Ok(task) => {
                self.track_task(&task, "provider-open");
                self.quick_view_task = Some(task);
                "Loading quick view…".clone_into(&mut self.status);
            }
            Err(error) => {
                self.quick_view = Some(ViewerSurface::text(
                    "near-fm.quick-view",
                    "Quick view",
                    format!("Cannot queue quick view: {error}"),
                ));
            }
        }
    }

    fn quick_view_panel(&self) -> Option<FocusedPanel> {
        if self.left_panel_type == PanelType::QuickView {
            Some(FocusedPanel::Left)
        } else if self.right_panel_type == PanelType::QuickView {
            Some(FocusedPanel::Right)
        } else {
            None
        }
    }

    fn create_directory_dialog() -> DialogSurface {
        DialogSurface::new(
            "near-fm.create-directory",
            "New folder",
            vec![DialogField {
                id: "name".to_owned(),
                label: "Name".to_owned(),
                value: String::new(),
                required: true,
                secret: false,
            }],
            CommandInvocation {
                id: CommandId::from("near.fs.create-directory.confirmed"),
                arguments: BTreeMap::default(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::default(),
            },
        )
    }

    fn create_archive_dialog() -> DialogSurface {
        DialogSurface::new(
            "near-fm.create-archive",
            "Create ZIP archive",
            vec![DialogField {
                id: "name".to_owned(),
                label: "Archive name".to_owned(),
                value: "archive.zip".to_owned(),
                required: true,
                secret: false,
            }],
            CommandInvocation {
                id: CommandId::from("near.archive.create-confirmed"),
                arguments: BTreeMap::default(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::default(),
            },
        )
    }

    fn create_archive(&mut self, invocation: &CommandInvocation) {
        let name = invocation
            .arguments
            .get("name")
            .and_then(CommandValue::as_str)
            .unwrap_or("archive.zip");
        let sources = self.canonical_targets();
        if sources.is_empty() {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return;
        }
        let parent = self.focused_panel().location().clone();
        match self.providers.create_container(&parent, name) {
            Ok(Some((_provider, destination))) => {
                self.overlay = None;
                self.plan_operation(OperationIntent::CopyTo {
                    sources,
                    destination,
                });
            }
            Ok(None) => {
                self.status = format!("No archive provider can create {name}");
            }
            Err(error) => {
                self.status = format!("Cannot create archive: {error}");
            }
        }
    }

    fn rename_dialog(&mut self) -> Option<DialogSurface> {
        let entries = self.canonical_target_entries();
        if entries.is_empty() {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return None;
        }
        let template = if entries.len() == 1 {
            entries[0].metadata.name.clone()
        } else {
            "{stem}_{index}{dotext}".to_owned()
        };
        Some(DialogSurface::new(
            "near-fm.rename",
            if entries.len() == 1 {
                "Rename"
            } else {
                "Multi-Rename"
            },
            vec![
                DialogField {
                    id: "template".to_owned(),
                    label: "Template {name} {stem} {ext} {dotext} {index}".to_owned(),
                    value: template,
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "start".to_owned(),
                    label: "Starting index".to_owned(),
                    value: "1".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: "near.resource.rename-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        ))
    }

    fn confirm_rename(&mut self, invocation: &CommandInvocation) {
        let template = invocation
            .arguments
            .get("template")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or_default()
            .trim();
        let start = match invocation
            .arguments
            .get("start")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or("1")
            .trim()
            .parse::<u64>()
        {
            Ok(start) => start,
            Err(_) => {
                "Starting index must be a non-negative integer".clone_into(&mut self.status);
                return;
            }
        };
        let entries = self.canonical_target_entries();
        let mut seen = BTreeMap::<String, String>::new();
        let mut items = Vec::new();
        for (offset, entry) in entries.iter().enumerate() {
            let index = start.saturating_add(u64::try_from(offset).unwrap_or(u64::MAX));
            let target = expand_rename_template(template, &entry.metadata.name, index);
            if let Err(error) = validate_rename_target(&target) {
                self.status = format!("{}: {error}", entry.metadata.name);
                return;
            }
            if let Some(existing) = seen.insert(target.clone(), entry.metadata.name.clone()) {
                self.status = format!(
                    "Rename template maps both {existing} and {} to {target}",
                    entry.metadata.name
                );
                return;
            }
            if target != entry.metadata.name {
                items.push((entry.resource.clone(), target));
            }
        }
        if items.is_empty() {
            self.overlay = None;
            "No names changed".clone_into(&mut self.status);
            return;
        }
        self.overlay = None;
        self.plan_operation(OperationIntent::Rename { items });
    }

    fn link_dialog(&mut self) -> Option<DialogSurface> {
        let entries = self.canonical_target_entries();
        if entries.len() != 1 {
            "Link creation requires exactly one source resource".clone_into(&mut self.status);
            return None;
        }
        Some(DialogSurface::new(
            "near-fm.link",
            "Create Link",
            vec![
                DialogField {
                    id: "name".to_owned(),
                    label: "New link name".to_owned(),
                    value: format!("{}.link", entries[0].metadata.name),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "type".to_owned(),
                    label: "Type: hard | symbolic | junction".to_owned(),
                    value: "symbolic".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: "near.resource.link-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        ))
    }

    fn confirm_link(&mut self, invocation: &CommandInvocation) {
        let mut entries = self.canonical_target_entries();
        if entries.len() != 1 {
            "Link creation requires exactly one source resource".clone_into(&mut self.status);
            return;
        }
        let entry = entries.remove(0);
        let name = invocation
            .arguments
            .get("name")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or_default()
            .trim();
        if let Err(error) = validate_rename_target(name) {
            self.status = format!("Invalid link name: {error}");
            return;
        }
        let kind = match invocation
            .arguments
            .get("type")
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or("symbolic")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "hard" | "hard-link" | "hardlink" => LinkKind::Hard,
            "symbolic" | "symbolic-link" | "symlink" | "soft" => LinkKind::Symbolic,
            "junction" | "directory" | "directory-link" => LinkKind::Junction,
            _ => {
                "Link type must be hard, symbolic, or junction".clone_into(&mut self.status);
                return;
            }
        };
        self.overlay = None;
        self.plan_operation(OperationIntent::CreateLink {
            source: entry.resource,
            name: name.to_owned(),
            kind,
        });
    }

    fn attributes_dialog() -> DialogSurface {
        DialogSurface::new(
            "near-fm.attributes",
            "Attributes, Ownership, and Timestamps",
            vec![
                DialogField {
                    id: "readonly".to_owned(),
                    label: "Read only: keep | yes | no".to_owned(),
                    value: "keep".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "unix_mode".to_owned(),
                    label: "Unix mode octal (blank keeps)".to_owned(),
                    value: String::new(),
                    required: false,
                    secret: false,
                },
                DialogField {
                    id: "owner".to_owned(),
                    label: "Owner UID (blank keeps)".to_owned(),
                    value: String::new(),
                    required: false,
                    secret: false,
                },
                DialogField {
                    id: "group".to_owned(),
                    label: "Group GID (blank keeps)".to_owned(),
                    value: String::new(),
                    required: false,
                    secret: false,
                },
                DialogField {
                    id: "modified".to_owned(),
                    label: "Modified Unix ms or now (blank keeps)".to_owned(),
                    value: String::new(),
                    required: false,
                    secret: false,
                },
                DialogField {
                    id: "accessed".to_owned(),
                    label: "Accessed Unix ms or now (blank keeps)".to_owned(),
                    value: String::new(),
                    required: false,
                    secret: false,
                },
                DialogField {
                    id: "recursive".to_owned(),
                    label: "Apply recursively: yes | no".to_owned(),
                    value: "no".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: "near.resource.attributes-confirmed".into(),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: "near.overlay.cancel".into(),
                arguments: BTreeMap::new(),
            },
        )
    }

    fn confirm_attributes(&mut self, invocation: &CommandInvocation) {
        let readonly = match parse_keep_boolean(argument_text(invocation, "readonly")) {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let unix_mode = match parse_optional_octal(argument_text(invocation, "unix_mode")) {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let owner = match parse_optional_u32(argument_text(invocation, "owner"), "Owner UID") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let group = match parse_optional_u32(argument_text(invocation, "group"), "Group GID") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let modified = match parse_optional_timestamp(argument_text(invocation, "modified")) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Modified timestamp: {error}");
                return;
            }
        };
        let accessed = match parse_optional_timestamp(argument_text(invocation, "accessed")) {
            Ok(value) => value,
            Err(error) => {
                self.status = format!("Accessed timestamp: {error}");
                return;
            }
        };
        let recursive = match parse_yes_no(argument_text(invocation, "recursive"), "Recursive") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let update = AttributeUpdate {
            readonly,
            unix_mode,
            owner,
            group,
            modified_unix_ms: modified,
            accessed_unix_ms: accessed,
        };
        if update.is_empty() {
            "Choose at least one attribute to change".clone_into(&mut self.status);
            return;
        }
        let sources = self.canonical_targets();
        if sources.is_empty() {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return;
        }
        self.overlay = None;
        self.plan_operation(OperationIntent::SetAttributes {
            sources,
            update,
            recursive,
        });
    }

    fn search_dialog() -> DialogSurface {
        let fields = SEARCH_DIALOG_FIELDS
            .iter()
            .map(|(id, label, value, required)| DialogField {
                id: (*id).to_owned(),
                label: (*label).to_owned(),
                value: (*value).to_owned(),
                required: *required,
                secret: false,
            })
            .collect();
        DialogSurface::new(
            "near-fm.search",
            "Find files",
            fields,
            CommandInvocation {
                id: CommandId::from("near.search.confirmed"),
                arguments: BTreeMap::default(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::default(),
            },
        )
    }

    fn selection_mask_dialog(selected: bool) -> DialogSurface {
        DialogSurface::new(
            if selected {
                "near-fm.select-mask"
            } else {
                "near-fm.unselect-mask"
            },
            if selected {
                "Select by mask"
            } else {
                "Unselect by mask"
            },
            vec![
                DialogField {
                    id: "include".to_owned(),
                    label: "Include".to_owned(),
                    value: "*".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "exclude".to_owned(),
                    label: "Exclude".to_owned(),
                    value: String::new(),
                    required: false,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: CommandId::from("near.selection.mask-confirmed"),
                arguments: BTreeMap::from([(
                    "selected".to_owned(),
                    near_core::CommandValue::Boolean(selected),
                )]),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::default(),
            },
        )
    }

    fn folder_comparison_dialog() -> DialogSurface {
        DialogSurface::new(
            "near-fm.compare-folders",
            "Compare folders",
            vec![
                DialogField {
                    id: "compare_size".to_owned(),
                    label: "Compare size".to_owned(),
                    value: "yes".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "compare_modified".to_owned(),
                    label: "Compare time".to_owned(),
                    value: "yes".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "tolerance_seconds".to_owned(),
                    label: "Time tolerance".to_owned(),
                    value: "0".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "case_sensitive".to_owned(),
                    label: "Case sensitive".to_owned(),
                    value: "no".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "selection".to_owned(),
                    label: "Select".to_owned(),
                    value: "newer".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: CommandId::from("near.selection.compare-folders-confirmed"),
                arguments: BTreeMap::default(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::default(),
            },
        )
    }

    fn apply_command_dialog() -> DialogSurface {
        DialogSurface::new(
            "near-fm.apply-command",
            "Apply command — {resource} {resources} {name} {panel}",
            vec![
                DialogField {
                    id: "template".to_owned(),
                    label: "Template".to_owned(),
                    value: String::new(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "mode".to_owned(),
                    label: "Mode".to_owned(),
                    value: "sequential".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "continue_on_error".to_owned(),
                    label: "Continue errors".to_owned(),
                    value: "yes".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: CommandId::from("near.operation.apply-command-confirmed"),
                arguments: BTreeMap::default(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::default(),
            },
        )
    }

    fn wipe_dialog() -> DialogSurface {
        DialogSurface::new(
            "near-fm.wipe",
            "Wipe regular files — SSD/COW recovery is not guaranteed",
            vec![DialogField {
                id: "passes".to_owned(),
                label: "Passes 1-7".to_owned(),
                value: "1".to_owned(),
                required: true,
                secret: false,
            }],
            CommandInvocation {
                id: CommandId::from("near.resource.wipe-confirmed"),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        )
    }

    fn confirm_wipe(&mut self, invocation: &CommandInvocation) {
        let passes = invocation
            .arguments
            .get("passes")
            .and_then(near_core::CommandValue::as_str)
            .and_then(|passes| passes.trim().parse::<u8>().ok());
        let Some(passes) = passes.filter(|passes| (1..=7).contains(passes)) else {
            "Wipe passes must be between 1 and 7".clone_into(&mut self.status);
            return;
        };
        let sources = self.canonical_targets();
        if sources.is_empty() {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return;
        }
        if !self.preflight_mutation(&sources, MutationKind::Wipe) {
            return;
        }
        self.overlay = None;
        self.plan_operation(OperationIntent::Wipe { sources, passes });
    }

    fn start_search(
        &mut self,
        predicate: ResourcePredicate,
        mode: SearchMode,
        options: SearchOptions,
    ) {
        let panel = self.focused;
        if let Err(error) = predicate.validate() {
            self.status = error.to_string();
            return;
        }
        if mode == SearchMode::Refine
            && let Some(state) = self.searches.get(&panel)
        {
            if let Some(task) = &state.task {
                task.cancel();
            }
            let session = state.session;
            let hits = state.provider.snapshot();
            let providers = state.providers.clone();
            match self.tasks.spawn(move |cancellation| {
                let result =
                    SearchService.refine_scoped(&providers, &hits, &predicate, &cancellation);
                WorkspaceTaskResult::SearchRefined {
                    panel,
                    session,
                    result,
                }
            }) {
                Ok(task) => {
                    self.track_task(&task, "provider-search-refine");
                    if let Some(state) = self.searches.get_mut(&panel) {
                        state.task = Some(task);
                    }
                    "Refining current search results…".clone_into(&mut self.status);
                }
                Err(error) => self.status = format!("Cannot queue refinement: {error}"),
            }
            return;
        }

        let existing = self.searches.get(&panel).map(|state| {
            (
                state.session,
                Arc::clone(&state.provider),
                state.diagnostics.clone(),
                state.request.clone(),
            )
        });
        let roots = if mode == SearchMode::Append {
            existing
                .as_ref()
                .map(|(_, _, _, request)| request.roots.clone())
        } else {
            None
        }
        .map_or_else(|| self.search_roots(options.scope), Ok);
        let roots = match roots {
            Ok(roots) => roots,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let mut request = ScopedSearchRequest::new(roots, predicate);
        request.archives = options.archives;
        request.symlinks = options.symlinks;
        request.streams = options.streams;
        let (session, results_provider, diagnostics, append) = if mode == SearchMode::Append
            && let Some((session, provider, diagnostics, _)) = existing
        {
            (session, provider, diagnostics, true)
        } else {
            let session = self.next_search_session;
            self.next_search_session = self.next_search_session.saturating_add(1);
            (
                session,
                Arc::new(SearchResultsProvider::new(session.to_string())),
                Vec::new(),
                false,
            )
        };
        self.cancel_search(panel);
        self.clear_listing_state(panel);
        if !append {
            self.panel_mut(panel)
                .replace(results_provider.location().clone(), Vec::new());
        }
        let updates = self.search_update_sender.clone();
        let providers = self.providers.clone();
        let task_providers = providers.clone();
        let task_request = request.clone();
        match self.tasks.spawn(move |cancellation| {
            let result = SearchService.search_scoped(
                &task_providers,
                &task_request,
                &cancellation,
                |event| {
                    let _ = updates.send(SearchUpdate {
                        panel,
                        session,
                        event,
                    });
                },
            );
            WorkspaceTaskResult::SearchComplete {
                panel,
                session,
                result,
            }
        }) {
            Ok(task) => {
                self.track_task(&task, "provider-search");
                self.searches.insert(
                    panel,
                    SearchState {
                        session,
                        provider: results_provider,
                        providers,
                        request,
                        diagnostics,
                        task: Some(task),
                    },
                );
                if append {
                    "Appending to current search results…".clone_into(&mut self.status);
                } else {
                    "Recursive search started…".clone_into(&mut self.status);
                }
            }
            Err(error) => self.status = format!("Cannot queue search: {error}"),
        }
    }

    fn search_roots(&self, scope: SearchScope) -> Result<Vec<SearchRoot>, String> {
        let mut roots = BTreeMap::<(ProviderId, Location), SearchRoot>::new();
        let mut add_location = |provider: Arc<dyn ResourceProvider>, location: Location| {
            let root = SearchRoot::new(provider.id(), location);
            roots.insert((root.provider.clone(), root.location.clone()), root);
        };
        match scope {
            SearchScope::CurrentRoots => {
                let location = self.focused_panel().location().clone();
                if let Some(provider) = self.providers.for_location(&location) {
                    add_location(provider, location);
                }
            }
            SearchScope::SelectedRoots => {
                let mut selected = self
                    .focused_panel()
                    .entries()
                    .iter()
                    .filter(|entry| entry.selected && !is_parent_entry(entry))
                    .collect::<Vec<_>>();
                if selected.is_empty()
                    && let Some(current) = self.focused_panel().current()
                    && !is_parent_entry(current)
                {
                    selected.push(current);
                }
                for entry in selected {
                    if entry.metadata.kind == ResourceKind::Directory
                        && let Some(provider) = self.providers.get(&entry.resource.provider)
                    {
                        add_location(provider, entry.resource.location.clone());
                    }
                }
            }
            SearchScope::Providers => {
                for provider in self.providers.providers() {
                    for location in provider.locations() {
                        add_location(Arc::clone(&provider), location.location);
                    }
                }
            }
            SearchScope::Archives => {
                let mut resources = self.focused_panel().selected_resources();
                if resources.is_empty()
                    && let Some(current) = self.focused_panel().current()
                    && !is_parent_entry(current)
                {
                    resources.push(current.resource.clone());
                }
                for resource in resources {
                    match self.providers.mount(&resource) {
                        Ok(Some((provider, location))) => add_location(provider, location),
                        Ok(None) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
        }
        let roots = roots.into_values().collect::<Vec<_>>();
        if roots.is_empty() {
            Err(match scope {
                SearchScope::CurrentRoots => "No current panel roots are searchable",
                SearchScope::SelectedRoots => "Select at least one searchable directory",
                SearchScope::Providers => "No provider roots are available",
                SearchScope::Archives => "Select at least one mountable archive",
            }
            .to_owned())
        } else {
            Ok(roots)
        }
    }

    fn cancel_search(&mut self, panel: FocusedPanel) {
        let Some(state) = self.searches.get_mut(&panel) else {
            return;
        };
        let Some(task) = state.task.as_ref() else {
            return;
        };
        task.cancel();
        "Search cancellation requested".clone_into(&mut self.status);
    }

    fn open_extension_results(&mut self, extension: &str, resources: Vec<ResourceRef>) {
        if resources.is_empty() {
            self.status = format!("Extension {extension} returned no resources");
            return;
        }
        let session = self.next_search_session;
        self.next_search_session = self.next_search_session.saturating_add(1);
        let provider = Arc::new(SearchResultsProvider::new(format!("extension-{session}")));
        provider.replace(
            resources
                .into_iter()
                .map(|source| SearchHit {
                    metadata: self
                        .known_metadata(&source)
                        .unwrap_or_else(|| generated_placeholder_metadata(&source)),
                    details: format!("extension {extension}"),
                    source,
                })
                .collect(),
        );
        self.cancel_search(self.focused);
        self.clear_listing_state(self.focused);
        let entries = provider
            .snapshot()
            .into_iter()
            .map(|hit| {
                let entry = hit.resource_entry(&format!("extension-{session}"));
                CollectionEntry::new(entry.resource, entry.metadata, entry.details)
            })
            .collect();
        self.panel_mut(self.focused)
            .replace(provider.location().clone(), entries);
        self.extension_panels.insert(
            self.focused,
            ExtensionPanelState {
                session,
                extension: extension.to_owned(),
                provider,
            },
        );
        self.status = format!(
            "Opened {} results from extension {extension}",
            self.focused_panel().entries().len()
        );
    }

    fn known_metadata(&self, resource: &ResourceRef) -> Option<ResourceMetadata> {
        self.left
            .entries()
            .iter()
            .chain(self.right.entries())
            .find(|entry| &entry.resource == resource)
            .map(|entry| entry.metadata.clone())
    }

    fn refresh_generated_panel(&mut self, panel: FocusedPanel) -> bool {
        let active = self
            .extension_panels
            .get(&panel)
            .filter(|state| self.panel(panel).location() == state.provider.location())
            .map(|state| (state.session, Arc::clone(&state.provider)))
            .or_else(|| {
                self.searches
                    .get(&panel)
                    .filter(|state| self.panel(panel).location() == state.provider.location())
                    .map(|state| (state.session, Arc::clone(&state.provider)))
            });
        let Some((session, generated_provider)) = active else {
            return false;
        };
        let resources = generated_provider
            .snapshot()
            .into_iter()
            .map(|hit| {
                let provider = self.providers.get(&hit.source.provider);
                (hit.source, provider)
            })
            .collect::<Vec<_>>();
        match self.tasks.spawn(move |cancellation| {
            let results = resources
                .into_iter()
                .map(|(resource, provider)| {
                    let result = if cancellation.is_cancelled() {
                        Err("refresh cancelled".to_owned())
                    } else if let Some(provider) = provider {
                        block_on(provider.stat(&resource)).map_err(|error| error.to_string())
                    } else {
                        Err(format!("provider {} is unavailable", resource.provider))
                    };
                    (resource, result)
                })
                .collect();
            WorkspaceTaskResult::GeneratedPanelRefresh {
                panel,
                session,
                results,
            }
        }) {
            Ok(task) => {
                self.track_task(&task, "generated-panel-refresh");
                "Refreshing generated panel resources…".clone_into(&mut self.status);
            }
            Err(error) => self.status = format!("Cannot refresh generated panel: {error}"),
        }
        true
    }

    fn finish_extension_panel_refresh(
        &mut self,
        panel: FocusedPanel,
        session: u64,
        results: Vec<(ResourceRef, Result<ResourceMetadata, String>)>,
    ) {
        let provider = self
            .extension_panels
            .get(&panel)
            .filter(|state| state.session == session)
            .map(|state| Arc::clone(&state.provider))
            .or_else(|| {
                self.searches
                    .get(&panel)
                    .filter(|state| state.session == session)
                    .map(|state| Arc::clone(&state.provider))
            });
        let Some(provider) = provider else { return };
        let previous = provider.snapshot();
        let mut stale = 0_usize;
        let hits = previous
            .into_iter()
            .map(|mut hit| {
                match results.iter().find(|(resource, _)| *resource == hit.source) {
                    Some((_, Ok(metadata))) => hit.metadata = metadata.clone(),
                    Some((_, Err(error))) => {
                        stale = stale.saturating_add(1);
                        hit.metadata.extensions.insert(
                            "near.generated.stale".to_owned(),
                            MetadataValue::Boolean(true),
                        );
                        hit.metadata.extensions.insert(
                            "near.generated.stale-reason".to_owned(),
                            MetadataValue::String(error.clone()),
                        );
                        hit.details = format!("stale — {error}");
                    }
                    None => {}
                }
                hit
            })
            .collect::<Vec<_>>();
        provider.replace(hits.clone());
        if self.panel(panel).location() == provider.location() {
            self.panel_mut(panel).replace(
                provider.location().clone(),
                hits.into_iter()
                    .map(|hit| {
                        let entry = hit.resource_entry(&session.to_string());
                        CollectionEntry::new(entry.resource, entry.metadata, entry.details)
                    })
                    .collect(),
            );
        }
        self.status = format!(
            "Refreshed generated panel: {} current, {stale} stale retained",
            provider.len().saturating_sub(stale)
        );
    }

    fn reveal_search_result(&mut self) {
        let Some(item) = self.focused_panel().current().cloned() else {
            "No search result to reveal".clone_into(&mut self.status);
            return;
        };
        let Some(provider) = self.providers.get(&item.resource.provider) else {
            self.status = format!("No provider registered for {}", item.resource.provider);
            return;
        };
        let Some(parent) = provider.parent(&item.resource.location) else {
            "Resource has no revealable parent".clone_into(&mut self.status);
            return;
        };
        self.pending_reveal_targets
            .insert(self.focused, item.resource.clone());
        self.navigate_collection(&provider, &parent);
        self.status = format!("Revealing {}", item.metadata.name);
    }

    fn keep_search_panel(&mut self) {
        if let Some(state) = self
            .extension_panels
            .get(&self.focused)
            .filter(|state| self.focused_panel().location() == state.provider.location())
        {
            if self
                .saved_extension_panels
                .iter()
                .any(|panel| panel.session == state.session)
            {
                self.status = format!("Generated panel {} is already saved", state.session);
                return;
            }
            let label = format!(
                "Extension {} — {} results",
                state.extension,
                state.provider.len()
            );
            self.saved_extension_panels.push(SavedExtensionPanel {
                session: state.session,
                label,
                extension: state.extension.clone(),
                provider: Arc::clone(&state.provider),
            });
            self.status = format!("Saved generated panel {}", state.session);
            return;
        }
        let Some(state) = self.searches.get(&self.focused) else {
            "The focused panel is not a search result panel".clone_into(&mut self.status);
            return;
        };
        if self.focused_panel().location() != state.provider.location() {
            "The focused panel is not displaying its search results".clone_into(&mut self.status);
            return;
        }
        if self
            .saved_search_panels
            .iter()
            .any(|panel| panel.session == state.session)
        {
            self.status = format!("Search panel {} is already saved", state.session);
            return;
        }
        let label = format!(
            "Search {} — {} results — {}",
            state.session,
            state.provider.len(),
            search_root_label(&state.request)
        );
        self.saved_search_panels.push(SavedSearchPanel {
            session: state.session,
            label,
            provider: Arc::clone(&state.provider),
            providers: state.providers.clone(),
            request: state.request.clone(),
            diagnostics: state.diagnostics.clone(),
        });
        self.status = format!("Saved search panel {}", state.session);
    }

    fn show_saved_search_panels(&mut self) {
        if self.saved_search_panels.is_empty() && self.saved_extension_panels.is_empty() {
            self.overlay = Some(Overlay::Message {
                title: "Saved Generated Panels".to_owned(),
                body: "No search or extension result panels have been saved in this session."
                    .to_owned(),
            });
            return;
        }
        let mut items = self
            .saved_search_panels
            .iter()
            .map(|panel| MenuItem {
                label: panel.label.clone(),
                description: format!("{} current results", panel.provider.len()),
                command: CommandInvocation {
                    id: CommandId::from("near.search.open-panel"),
                    arguments: BTreeMap::from([(
                        "session".to_owned(),
                        CommandValue::Integer(i64::try_from(panel.session).unwrap_or(i64::MAX)),
                    )]),
                },
                enabled: true,
            })
            .collect::<Vec<_>>();
        items.extend(self.saved_extension_panels.iter().map(|panel| MenuItem {
            label: panel.label.clone(),
            description: format!("{} current extension results", panel.provider.len()),
            command: CommandInvocation {
                id: CommandId::from("near.search.open-panel"),
                arguments: BTreeMap::from([(
                    "session".to_owned(),
                    CommandValue::Integer(i64::try_from(panel.session).unwrap_or(i64::MAX)),
                )]),
            },
            enabled: true,
        }));
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.saved-search-panels",
            "Saved Generated Panels",
            items,
        )));
    }

    fn open_saved_search_panel(&mut self, invocation: &CommandInvocation) {
        let session = invocation
            .arguments
            .get("session")
            .and_then(|value| match value {
                CommandValue::Integer(value) => u64::try_from(*value).ok(),
                _ => None,
            });
        let Some(saved) = session.and_then(|session| {
            self.saved_search_panels
                .iter()
                .find(|panel| panel.session == session)
                .cloned()
        }) else {
            if let Some(saved) = session.and_then(|session| {
                self.saved_extension_panels
                    .iter()
                    .find(|panel| panel.session == session)
                    .cloned()
            }) {
                self.open_saved_extension_panel(saved);
            } else {
                "Saved generated panel is unavailable".clone_into(&mut self.status);
            }
            return;
        };
        self.cancel_search(self.focused);
        self.clear_listing_state(self.focused);
        let entries = saved
            .provider
            .snapshot()
            .into_iter()
            .map(|hit| {
                let entry = hit.resource_entry(&saved.session.to_string());
                CollectionEntry::new(entry.resource, entry.metadata, entry.details)
            })
            .collect();
        self.panel_mut(self.focused)
            .replace(saved.provider.location().clone(), entries);
        self.searches.insert(
            self.focused,
            SearchState {
                session: saved.session,
                provider: saved.provider,
                providers: saved.providers,
                request: saved.request,
                diagnostics: saved.diagnostics,
                task: None,
            },
        );
        self.overlay = None;
        self.status = format!("Opened saved search panel {}", saved.session);
    }

    fn open_saved_extension_panel(&mut self, saved: SavedExtensionPanel) {
        self.cancel_search(self.focused);
        self.clear_listing_state(self.focused);
        let entries = saved
            .provider
            .snapshot()
            .into_iter()
            .map(|hit| {
                let entry = hit.resource_entry(&format!("extension-{}", saved.session));
                CollectionEntry::new(entry.resource, entry.metadata, entry.details)
            })
            .collect();
        self.panel_mut(self.focused)
            .replace(saved.provider.location().clone(), entries);
        self.extension_panels.insert(
            self.focused,
            ExtensionPanelState {
                session: saved.session,
                extension: saved.extension,
                provider: saved.provider,
            },
        );
        self.overlay = None;
        self.status = format!("Opened saved generated panel {}", saved.session);
    }

    fn canonical_targets(&self) -> Vec<ResourceRef> {
        self.focused_panel()
            .target_resources(CollectionTargetScope::SelectionOrCurrent)
    }

    fn canonical_target_entries(&self) -> Vec<CollectionEntry> {
        self.focused_panel()
            .target_entries(CollectionTargetScope::SelectionOrCurrent)
            .into_iter()
            .cloned()
            .collect()
    }

    fn active_temporary_panel_slot(&self, panel: FocusedPanel) -> Option<u8> {
        let slot = *self.active_temporary_panels.get(&panel)?;
        self.temporary_panels
            .get(&slot)
            .filter(|temporary| self.panel(panel).location() == &temporary.location)
            .map(|temporary| temporary.slot)
    }

    fn temporary_panel_entries(temporary: &TemporaryPanel) -> Vec<CollectionEntry> {
        temporary
            .hits
            .iter()
            .map(|hit| {
                let mut metadata = hit.metadata.clone();
                metadata.extensions.insert(
                    "near.temporary-panel.slot".to_owned(),
                    MetadataValue::Integer(i64::from(temporary.slot)),
                );
                metadata.extensions.insert(
                    "near.temporary-panel.any".to_owned(),
                    MetadataValue::Boolean(temporary.allow_arbitrary),
                );
                metadata.extensions.insert(
                    "near.temporary-panel.safe".to_owned(),
                    MetadataValue::Boolean(temporary.safe_mode),
                );
                CollectionEntry::new(hit.source.clone(), metadata, hit.details.clone())
            })
            .collect()
    }

    fn temporary_panel_is_safe(&self, panel: FocusedPanel) -> bool {
        self.active_temporary_panel_slot(panel)
            .and_then(|slot| self.temporary_panels.get(&slot))
            .is_some_and(|temporary| temporary.safe_mode)
    }

    fn temporary_panel_is_full_screen(&self, panel: FocusedPanel) -> bool {
        self.active_temporary_panel_slot(panel)
            .and_then(|slot| self.temporary_panels.get(&slot))
            .is_some_and(|temporary| temporary.full_screen)
    }

    fn panel_viewport_height(&self, rows: u16) -> u16 {
        rows.saturating_sub(1)
            .saturating_sub(u16::from(self.settings.interface.show_status_line))
            .saturating_sub(u16::from(self.settings.interface.show_keybar))
    }

    fn replace_temporary_panel_surface(&mut self, panel: FocusedPanel, slot: u8) {
        let temporary = self
            .temporary_panels
            .get(&slot)
            .expect("temporary panel slot must exist")
            .clone();
        self.panel_mut(panel).replace(
            temporary.location.clone(),
            Self::temporary_panel_entries(&temporary),
        );
    }

    fn open_temporary_panel(&mut self, invocation: &CommandInvocation) {
        let slot = invocation
            .arguments
            .get("slot")
            .and_then(|value| match value {
                CommandValue::Integer(value) => u8::try_from(*value).ok(),
                _ => None,
            })
            .unwrap_or(0);
        if slot > 9 {
            self.status = format!("Temporary panel slot must be between 0 and 9, not {slot}");
            return;
        }
        let target = provider_target(invocation, self.focused);
        let temporary = self
            .temporary_panels
            .entry(slot)
            .or_insert_with(|| TemporaryPanel::new(slot))
            .clone();
        self.cancel_search(target);
        self.clear_listing_state(target);
        self.panel_mut(target).replace(
            temporary.location.clone(),
            Self::temporary_panel_entries(&temporary),
        );
        self.active_temporary_panels.insert(target, slot);
        self.last_temporary_panel_slot = slot;
        self.overlay = None;
        self.status = format!(
            "Temporary panel {slot}: {} reference(s), {} stale{}",
            temporary.hits.len(),
            temporary.stale_count(),
            if temporary.safe_mode {
                ", safe mode"
            } else {
                ""
            }
        );
        self.persist_temporary_panels();
    }

    fn show_temporary_panels(&mut self, invocation: &CommandInvocation) {
        let target = provider_target(invocation, self.focused);
        let mut items = (0_u8..=9)
            .map(|slot| {
                let count = self
                    .temporary_panels
                    .get(&slot)
                    .map_or(0, |temporary| temporary.hits.len());
                let mode = self
                    .temporary_panels
                    .get(&slot)
                    .map_or_else(String::new, |temporary| {
                        format!(
                            "{}{}",
                            if temporary.safe_mode { " R" } else { "" },
                            if temporary.full_screen { " F" } else { "" }
                        )
                    });
                let active = self
                    .active_temporary_panel_slot(target)
                    .filter(|active| *active == slot)
                    .map_or(" ", |_| "*");
                let preview = self
                    .temporary_panels
                    .get(&slot)
                    .map(|temporary| {
                        temporary
                            .hits
                            .iter()
                            .take(3)
                            .map(|hit| hit.metadata.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|preview| !preview.is_empty())
                    .unwrap_or_else(|| "empty".to_owned());
                MenuItem {
                    label: format!("&{slot}  [{active}] {count} reference(s){mode}"),
                    description: format!("Open on {}: {preview}", panel_name(target)),
                    command: CommandInvocation {
                        id: CommandId::from("near.temp-panel.open"),
                        arguments: BTreeMap::from([
                            ("slot".to_owned(), CommandValue::Integer(i64::from(slot))),
                            (
                                "target".to_owned(),
                                CommandValue::String(panel_name(target).to_owned()),
                            ),
                        ]),
                    },
                    enabled: true,
                }
            })
            .collect::<Vec<_>>();
        let active = self.active_temporary_panel_slot(target);
        let management_enabled = target == self.focused && active.is_some();
        let safe_mode = active
            .and_then(|slot| self.temporary_panels.get(&slot))
            .is_some_and(|temporary| temporary.safe_mode);
        items.extend([
            MenuItem {
                label: "&Import list…".to_owned(),
                description: "Append or replace references from a UTF-8 list".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.temp-panel.import"),
                    arguments: BTreeMap::new(),
                },
                enabled: management_enabled,
            },
            MenuItem {
                label: "&Export list…".to_owned(),
                description: "Write provider-qualified references to a UTF-8 list".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.temp-panel.export"),
                    arguments: BTreeMap::new(),
                },
                enabled: management_enabled,
            },
            MenuItem {
                label: format!("&Safe mode [{}]", if safe_mode { "x" } else { " " }),
                description: "Disable reference changes and source mutations".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.temp-panel.safe-toggle"),
                    arguments: BTreeMap::new(),
                },
                enabled: management_enabled,
            },
            MenuItem {
                label: "&Refresh references".to_owned(),
                description: "Refresh metadata and retain visibly stale references".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.temp-panel.refresh"),
                    arguments: BTreeMap::new(),
                },
                enabled: management_enabled,
            },
        ]);
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.temporary-panels",
            "Temporary Panels",
            items,
        )));
    }

    fn persist_temporary_panels(&mut self) {
        let Some(store) = &self.state_document_store else {
            return;
        };
        let document = TemporaryPanelDocument {
            schema_version: TEMPORARY_PANEL_STATE_SCHEMA,
            last_used_slot: self.last_temporary_panel_slot,
            panels: self
                .temporary_panels
                .values()
                .map(|panel| TemporaryPanelRecord {
                    slot: panel.slot,
                    hits: panel
                        .hits
                        .iter()
                        .map(|hit| TemporaryPanelHitRecord {
                            source: hit.source.clone(),
                            metadata: hit.metadata.clone(),
                            details: hit.details.clone(),
                        })
                        .collect(),
                    safe_mode: panel.safe_mode,
                    allow_arbitrary: panel.allow_arbitrary,
                    full_screen: panel.full_screen,
                })
                .collect(),
        };
        let result = toml::to_string_pretty(&document)
            .map_err(|error| error.to_string())
            .and_then(|source| store.persist(TEMPORARY_PANEL_STATE_DOCUMENT, &source));
        if let Err(error) = result {
            self.status = format!(
                "Cannot save Temporary Panels: {error}; references remain available in memory and source resources are unchanged"
            );
        }
    }

    fn clear_temporary_panels(&mut self, all: bool) {
        if all {
            if let Some(slot) = self
                .temporary_panels
                .values()
                .find(|panel| panel.safe_mode && !panel.hits.is_empty())
                .map(|panel| panel.slot)
            {
                self.status = format!(
                    "Temporary panel {slot} is in safe mode; clearing all references is disabled"
                );
                return;
            }
            let cleared = self
                .temporary_panels
                .values_mut()
                .map(|panel| {
                    let count = panel.hits.len();
                    panel.hits.clear();
                    count
                })
                .sum::<usize>();
            let active = self.active_temporary_panels.clone();
            for (panel, slot) in active {
                self.replace_temporary_panel_surface(panel, slot);
            }
            self.status = format!(
                "Cleared {cleared} reference(s) from all Temporary Panels; source resources unchanged"
            );
            self.persist_temporary_panels();
            return;
        }
        let Some(slot) = self.active_temporary_panel_slot(self.focused) else {
            "The focused panel is not a temporary panel".clone_into(&mut self.status);
            return;
        };
        if self.temporary_panel_is_safe(self.focused) {
            self.status =
                format!("Temporary panel {slot} is in safe mode; clearing references is disabled");
            return;
        }
        let cleared = self
            .temporary_panels
            .get_mut(&slot)
            .map(|panel| {
                let count = panel.hits.len();
                panel.hits.clear();
                count
            })
            .unwrap_or_default();
        self.replace_temporary_panel_surface(self.focused, slot);
        self.status = format!(
            "Cleared {cleared} reference(s) from Temporary Panel {slot}; source resources unchanged"
        );
        self.persist_temporary_panels();
    }

    fn add_references_to_peer_temporary_panel(&mut self, scope: CollectionTargetScope) -> bool {
        let peer = match self.focused {
            FocusedPanel::Left => FocusedPanel::Right,
            FocusedPanel::Right => FocusedPanel::Left,
        };
        let Some(slot) = self.active_temporary_panel_slot(peer) else {
            return false;
        };
        if self.temporary_panel_is_safe(peer) {
            self.status =
                format!("Temporary panel {slot} is in safe mode; adding references is disabled");
            return true;
        }
        let hits = self
            .focused_panel()
            .target_entries(scope)
            .into_iter()
            .map(|entry| SearchHit {
                source: entry.resource.clone(),
                metadata: entry.metadata.clone(),
                details: entry.details.clone(),
            })
            .collect::<Vec<_>>();
        if hits.is_empty() {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return true;
        }
        let temporary = self
            .temporary_panels
            .get_mut(&slot)
            .expect("active temporary panel slot must exist");
        let before = temporary.hits.len();
        for hit in hits {
            if temporary
                .hits
                .iter()
                .all(|existing| existing.source != hit.source)
            {
                temporary.hits.push(hit);
            }
        }
        let added = temporary.hits.len().saturating_sub(before);
        let temporary = temporary.clone();
        self.panel_mut(peer).replace(
            temporary.location.clone(),
            Self::temporary_panel_entries(&temporary),
        );
        self.status = format!(
            "Added {added} reference(s) to temporary panel {slot}; source resources unchanged"
        );
        self.persist_temporary_panels();
        true
    }

    fn remove_from_temporary_panel(&mut self) {
        let Some(slot) = self.active_temporary_panel_slot(self.focused) else {
            "The focused panel is not a temporary panel".clone_into(&mut self.status);
            return;
        };
        if self.temporary_panel_is_safe(self.focused) {
            self.status =
                format!("Temporary panel {slot} is in safe mode; removing references is disabled");
            return;
        }
        let resources = self
            .focused_panel()
            .target_resources(CollectionTargetScope::SelectionOrCurrent);
        if resources.is_empty() {
            "No temporary-panel references selected".clone_into(&mut self.status);
            return;
        }
        let temporary = self
            .temporary_panels
            .get_mut(&slot)
            .expect("active temporary panel slot must exist");
        let before = temporary.hits.len();
        temporary
            .hits
            .retain(|hit| !resources.contains(&hit.source));
        let removed = before.saturating_sub(temporary.hits.len());
        let temporary = temporary.clone();
        self.panel_mut(self.focused).replace(
            temporary.location.clone(),
            Self::temporary_panel_entries(&temporary),
        );
        self.status = format!(
            "Removed {removed} reference(s) from temporary panel {slot}; source resources unchanged"
        );
        self.persist_temporary_panels();
    }

    fn reveal_temporary_panel_resource(&mut self) {
        if self.active_temporary_panel_slot(self.focused).is_none() {
            "The focused panel is not a temporary panel".clone_into(&mut self.status);
            return;
        }
        self.reveal_search_result();
    }

    fn show_temporary_panel_import_dialog(&mut self) {
        if self.active_temporary_panel_slot(self.focused).is_none() {
            "Open a temporary panel before importing a list".clone_into(&mut self.status);
            return;
        }
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            "near-fm.temporary-panel-import",
            "Import UTF-8 resource list — mode is append or replace",
            vec![
                DialogField {
                    id: "path".to_owned(),
                    label: "List file".to_owned(),
                    value: String::new(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "any".to_owned(),
                    label: "Any lines (true/false)".to_owned(),
                    value: "false".to_owned(),
                    required: true,
                    secret: false,
                },
                DialogField {
                    id: "mode".to_owned(),
                    label: "Mode".to_owned(),
                    value: "append".to_owned(),
                    required: true,
                    secret: false,
                },
            ],
            CommandInvocation {
                id: CommandId::from("near.temp-panel.import-confirmed"),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn show_temporary_panel_export_dialog(&mut self) {
        if self.active_temporary_panel_slot(self.focused).is_none() {
            "Open a temporary panel before exporting a list".clone_into(&mut self.status);
            return;
        }
        self.overlay = Some(Overlay::Surface(Box::new(DialogSurface::new(
            "near-fm.temporary-panel-export",
            "Export UTF-8 provider-qualified resource list",
            vec![DialogField {
                id: "path".to_owned(),
                label: "List file".to_owned(),
                value: String::new(),
                required: true,
                secret: false,
            }],
            CommandInvocation {
                id: CommandId::from("near.temp-panel.export-confirmed"),
                arguments: BTreeMap::new(),
            },
            CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            },
        ))));
    }

    fn parse_temporary_panel_resource(&self, line: &str) -> Result<ResourceRef, String> {
        let line = line.trim();
        if line.is_empty() {
            return Err("empty line".to_owned());
        }
        for provider in self.providers.providers() {
            let provider_id = provider.id();
            let marker = format!("{provider_id}:");
            if let Some(location) = line.strip_prefix(&marker) {
                return Ok(ResourceRef {
                    provider: provider_id,
                    location: Location::new(location),
                });
            }
        }
        for provider in self.providers.providers() {
            if let Some(resource) = provider
                .parse_native_reference(line)
                .map_err(|error| error.to_string())?
            {
                return Ok(resource);
            }
        }
        Err("expected a provider-native or provider-qualified resource".to_owned())
    }

    fn show_temporary_panel_list_menu(&mut self, path: &str) {
        if path.is_empty() {
            "tmp:+menu requires a UTF-8 list file".clone_into(&mut self.status);
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text.trim_start_matches('\u{feff}').to_owned(),
            Err(error) => {
                self.status = format!("Cannot read Temporary Panel menu {path}: {error}");
                return;
            }
        };
        let mut items = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if line == "|-|" {
                items.push(MenuItem {
                    label: "────────".to_owned(),
                    description: "Separator".to_owned(),
                    command: CommandInvocation {
                        id: CommandId::from("near.temp-panel.menu-select"),
                        arguments: BTreeMap::new(),
                    },
                    enabled: false,
                });
                continue;
            }
            let (label, action) = line
                .strip_prefix('|')
                .and_then(|value| value.split_once('|'))
                .map_or((line, line), |(label, action)| (label, action));
            items.push(MenuItem {
                label: label.to_owned(),
                description: action.to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.temp-panel.menu-select"),
                    arguments: BTreeMap::from([(
                        "text".to_owned(),
                        CommandValue::String(action.to_owned()),
                    )]),
                },
                enabled: true,
            });
        }
        if items.is_empty() {
            self.status = format!("Temporary Panel menu {path} contains no items");
            return;
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Temporary Panel List")
            .to_owned();
        self.overlay = Some(Overlay::Menu(MenuSurface::new(
            "near-fm.temporary-panel-list-menu",
            title,
            items,
        )));
        self.status = format!("Temporary Panel menu: {path}");
    }

    fn activate_temporary_panel_menu_item(&mut self, invocation: &CommandInvocation) {
        let Some(text) = invocation
            .arguments
            .get("text")
            .and_then(CommandValue::as_str)
        else {
            "Temporary Panel menu item has no action".clone_into(&mut self.status);
            return;
        };
        self.overlay = None;
        if self.dispatch_command_prefix(text) {
            return;
        }
        if let Ok(resource) = self.parse_temporary_panel_resource(text)
            && let Some(provider) = self.providers.get(&resource.provider)
            && let Ok(metadata) = block_on(provider.stat(&resource))
        {
            if metadata.kind == ResourceKind::Directory {
                self.navigate_collection(&provider, &resource.location);
                self.status = format!("Temporary Panel menu opened {}", metadata.name);
                return;
            }
            if let Some(parent) = provider.parent(&resource.location) {
                self.pending_reveal_targets
                    .insert(self.focused, resource.clone());
                self.navigate_collection(&provider, &parent);
                self.status = format!("Temporary Panel menu revealing {}", metadata.name);
                return;
            }
        }
        self.replace_command_text(text);
        self.status = "Copied Temporary Panel menu action to the command line".to_owned();
    }

    fn import_temporary_panel(&mut self, invocation: &CommandInvocation) {
        let Some(slot) = self.active_temporary_panel_slot(self.focused) else {
            "The focused panel is not a temporary panel".clone_into(&mut self.status);
            return;
        };
        let Some(path) = invocation
            .arguments
            .get("path")
            .and_then(CommandValue::as_str)
        else {
            "Temporary-panel list path is missing".clone_into(&mut self.status);
            return;
        };
        let mode = invocation
            .arguments
            .get("mode")
            .and_then(CommandValue::as_str)
            .unwrap_or("append")
            .trim()
            .to_ascii_lowercase();
        if mode != "append" && mode != "replace" {
            "Temporary-panel import mode must be append or replace".clone_into(&mut self.status);
            return;
        }
        let allow_arbitrary = invocation.arguments.get("any").map_or_else(
            || self.temporary_panels[&slot].allow_arbitrary,
            |value| match value {
                CommandValue::Boolean(value) => *value,
                CommandValue::String(value) => value.eq_ignore_ascii_case("true"),
                _ => false,
            },
        );
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text.trim_start_matches('\u{feff}').to_owned(),
            Err(error) => {
                self.status = format!("Cannot read temporary-panel list {path}: {error}");
                return;
            }
        };
        let result = self.ingest_temporary_panel_text(
            self.focused,
            slot,
            &text,
            mode == "replace",
            allow_arbitrary,
        );
        let (added, rejected) = match result {
            Ok(counts) => counts,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        self.overlay = None;
        self.status = format!(
            "Temporary panel {slot}: imported {added} reference(s), rejected {rejected} line(s)"
        );
    }

    fn ingest_temporary_panel_text(
        &mut self,
        panel: FocusedPanel,
        slot: u8,
        text: &str,
        replace: bool,
        allow_arbitrary: bool,
    ) -> Result<(usize, usize), String> {
        if self
            .temporary_panels
            .get(&slot)
            .is_some_and(|temporary| temporary.safe_mode)
        {
            return Err(format!(
                "Temporary panel {slot} is in safe mode; imports and command capture cannot change references"
            ));
        }
        let mut imported = Vec::new();
        let mut rejected = 0_usize;
        let arbitrary_prefix = format!("temporary-text://{slot}/");
        let mut arbitrary_sequence = if replace {
            0
        } else {
            self.temporary_panels
                .get(&slot)
                .into_iter()
                .flat_map(|temporary| &temporary.hits)
                .filter_map(|hit| {
                    hit.source
                        .location
                        .as_str()
                        .strip_prefix(&arbitrary_prefix)?
                        .parse::<usize>()
                        .ok()
                })
                .max()
                .map_or(0, |sequence| sequence.saturating_add(1))
        };
        for line in text.lines() {
            let resource = match self.parse_temporary_panel_resource(line) {
                Ok(resource) => resource,
                Err(_) if line.trim().is_empty() => continue,
                Err(_) if allow_arbitrary => {
                    let text = line.to_owned();
                    let mut metadata = ResourceMetadata {
                        name: text.clone(),
                        kind: ResourceKind::Virtual,
                        ..ResourceMetadata::default()
                    };
                    metadata.extensions.insert(
                        "near.temporary-panel.arbitrary-text".to_owned(),
                        MetadataValue::String(text.clone()),
                    );
                    imported.push(SearchHit {
                        source: ResourceRef {
                            provider: ProviderId::from("near.temporary-text"),
                            location: Location::new(format!(
                                "{arbitrary_prefix}{arbitrary_sequence}"
                            )),
                        },
                        metadata,
                        details: "Arbitrary Temporary Panel line".to_owned(),
                    });
                    arbitrary_sequence = arbitrary_sequence.saturating_add(1);
                    continue;
                }
                Err(_) => {
                    rejected += 1;
                    continue;
                }
            };
            let Some(provider) = self.providers.get(&resource.provider) else {
                rejected += 1;
                continue;
            };
            match block_on(provider.stat(&resource)) {
                Ok(metadata) => imported.push(SearchHit {
                    source: resource,
                    details: "Imported temporary-panel reference".to_owned(),
                    metadata,
                }),
                Err(_) => rejected += 1,
            }
        }
        let temporary = self
            .temporary_panels
            .get_mut(&slot)
            .expect("temporary panel slot must exist");
        temporary.allow_arbitrary = allow_arbitrary;
        if replace {
            temporary.hits.clear();
        }
        let before = temporary.hits.len();
        for hit in imported {
            if temporary
                .hits
                .iter()
                .all(|existing| existing.source != hit.source)
            {
                temporary.hits.push(hit);
            }
        }
        let added = temporary.hits.len().saturating_sub(before);
        if self.active_temporary_panel_slot(panel) == Some(slot) {
            self.replace_temporary_panel_surface(panel, slot);
        }
        self.persist_temporary_panels();
        Ok((added, rejected))
    }

    fn export_temporary_panel(&mut self, invocation: &CommandInvocation) {
        let Some(slot) = self.active_temporary_panel_slot(self.focused) else {
            "The focused panel is not a temporary panel".clone_into(&mut self.status);
            return;
        };
        let Some(path) = invocation
            .arguments
            .get("path")
            .and_then(CommandValue::as_str)
        else {
            "Temporary-panel list path is missing".clone_into(&mut self.status);
            return;
        };
        let temporary = self
            .temporary_panels
            .get(&slot)
            .expect("active temporary panel slot must exist");
        let mut body = String::from("\u{feff}");
        for hit in &temporary.hits {
            if let Some(MetadataValue::String(text)) = hit
                .metadata
                .extensions
                .get("near.temporary-panel.arbitrary-text")
            {
                body.push_str(text);
            } else {
                body.push_str(&hit.source.to_string());
            }
            body.push('\n');
        }
        if let Err(error) = std::fs::write(path, body) {
            self.status = format!("Cannot write temporary-panel list {path}: {error}");
            return;
        }
        self.overlay = None;
        self.status = format!(
            "Temporary panel {slot}: exported {} reference(s) to {path}",
            temporary.hits.len()
        );
    }

    fn toggle_temporary_panel_safe_mode(&mut self) {
        let Some(slot) = self.active_temporary_panel_slot(self.focused) else {
            "The focused panel is not a temporary panel".clone_into(&mut self.status);
            return;
        };
        let temporary = self
            .temporary_panels
            .get_mut(&slot)
            .expect("active temporary panel slot must exist");
        temporary.safe_mode = !temporary.safe_mode;
        let safe_mode = temporary.safe_mode;
        self.replace_temporary_panel_surface(self.focused, slot);
        self.status = format!(
            "Temporary panel {slot}: safe mode {}",
            if safe_mode { "enabled" } else { "disabled" }
        );
        self.persist_temporary_panels();
    }

    fn refresh_temporary_panel(&mut self, panel: FocusedPanel) -> bool {
        let Some(slot) = self.active_temporary_panel_slot(panel) else {
            return false;
        };
        let hits = self
            .temporary_panels
            .get(&slot)
            .expect("active temporary panel slot must exist")
            .hits
            .clone();
        let mut refreshed = Vec::with_capacity(hits.len());
        let mut stale = 0_usize;
        for mut hit in hits {
            if hit
                .metadata
                .extensions
                .contains_key("near.temporary-panel.arbitrary-text")
            {
                refreshed.push(hit);
                continue;
            }
            let result = self
                .providers
                .get(&hit.source.provider)
                .ok_or_else(|| format!("provider {} is unavailable", hit.source.provider))
                .and_then(|provider| {
                    block_on(provider.stat(&hit.source)).map_err(|error| error.to_string())
                });
            match result {
                Ok(metadata) => {
                    hit.metadata = metadata;
                    hit.details = "Refreshed temporary-panel reference".to_owned();
                }
                Err(error) => {
                    stale += 1;
                    hit.metadata.extensions.insert(
                        "near.temporary-panel.stale".to_owned(),
                        MetadataValue::String(error.clone()),
                    );
                    hit.metadata
                        .field_errors
                        .insert("temporary-panel-refresh".to_owned(), error);
                    hit.details = "Stale temporary-panel reference".to_owned();
                }
            }
            refreshed.push(hit);
        }
        self.temporary_panels
            .get_mut(&slot)
            .expect("active temporary panel slot must exist")
            .hits = refreshed;
        self.replace_temporary_panel_surface(panel, slot);
        self.status = format!("Temporary panel {slot}: refreshed, {stale} stale reference(s)");
        self.persist_temporary_panels();
        true
    }

    fn plan_peer_operation(&mut self, move_resources: bool, scope: CollectionTargetScope) {
        let sources = self.focused_panel().target_resources(scope);
        if sources.is_empty() {
            "The parent entry is navigation-only".clone_into(&mut self.status);
            return;
        }
        let peer = match self.focused {
            FocusedPanel::Left => self.right.location().clone(),
            FocusedPanel::Right => self.left.location().clone(),
        };
        let intent = if move_resources {
            OperationIntent::MoveTo {
                sources,
                destination: peer,
            }
        } else {
            OperationIntent::CopyTo {
                sources,
                destination: peer,
            }
        };
        self.plan_operation(intent);
    }

    fn plan_operation(&mut self, intent: OperationIntent) {
        if self.temporary_panel_is_safe(self.focused) {
            let slot = self
                .active_temporary_panel_slot(self.focused)
                .expect("safe temporary panel must be active");
            self.status =
                format!("Temporary panel {slot} is in safe mode; resource mutation is disabled");
            return;
        }
        if self
            .focused_panel()
            .target_entries(CollectionTargetScope::SelectionOrCurrent)
            .iter()
            .any(|entry| {
                entry
                    .metadata
                    .extensions
                    .contains_key("near.temporary-panel.arbitrary-text")
            })
        {
            "Arbitrary Temporary Panel lines are command text, not provider resources"
                .clone_into(&mut self.status);
            return;
        }
        self.plan_unscoped_operation(intent);
    }

    fn restore_last_trash(&mut self) {
        if self.last_trash_restoration.is_empty() {
            "No completed Trash operation is available to restore".clone_into(&mut self.status);
            return;
        }
        self.plan_unscoped_operation(OperationIntent::Restore {
            items: self.last_trash_restoration.clone(),
        });
    }

    fn plan_unscoped_operation(&mut self, intent: OperationIntent) {
        let Some(service) = &self.operations else {
            "No operation service is configured".clone_into(&mut self.status);
            return;
        };
        let planned = service
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plan(intent, self.generation);
        match planned {
            Ok(plan) => {
                if self.settings.confirmations.requires_preview(&plan) {
                    self.overlay = Some(Overlay::Surface(Box::new(OperationPreviewSurface::new(
                        "near-fm.operation-preview",
                        plan,
                    ))));
                } else {
                    let conflict = default_conflict_action(&plan);
                    self.start_operation(plan.id().clone(), conflict, true, false);
                }
            }
            Err(error) => self.status = error,
        }
    }

    fn mutation_denial(
        &self,
        sources: &[ResourceRef],
        mutation: MutationKind,
    ) -> Option<(ResourceRef, MutationDenial)> {
        sources.iter().find_map(|source| {
            let Some(provider) = self.providers.get(&source.provider) else {
                return Some((
                    source.clone(),
                    MutationDenial {
                        reason: format!(
                            "cannot verify mutation safety because provider {} is unavailable",
                            source.provider
                        ),
                        alternative: None,
                    },
                ));
            };
            match provider.mutation_eligibility(source, mutation) {
                MutationEligibility::Allowed => None,
                MutationEligibility::Denied(denial) => Some((source.clone(), denial)),
            }
        })
    }

    fn preflight_mutation(&mut self, sources: &[ResourceRef], mutation: MutationKind) -> bool {
        let Some((source, denial)) = self.mutation_denial(sources, mutation) else {
            return true;
        };
        let action = match mutation {
            MutationKind::Trash => "Move to Trash",
            MutationKind::Delete => "Delete permanently",
            MutationKind::Wipe => "Wipe",
            _ => "Mutate resource",
        };
        let alternative = match denial.alternative {
            Some(MutationAlternative::Eject) => {
                "Use Commands → Hotplug devices to eject this resource safely."
            }
            Some(MutationAlternative::Unmount) => {
                "Use Commands → Hotplug devices to unmount or eject this volume safely."
            }
            Some(MutationAlternative::Disconnect) => "Use the provider disconnect command instead.",
            None => "Choose an ordinary file, directory, or symbolic link instead.",
            Some(_) => "Use the provider-specific safe alternative instead.",
        };
        self.status = format!("{action} denied: {}", denial.reason);
        self.overlay = Some(Overlay::Message {
            title: format!("Cannot {action}"),
            body: format!(
                "Near did not create an operation plan for {}.\n\n{}\n\n{}",
                source.location.as_str(),
                denial.reason,
                alternative
            ),
        });
        false
    }

    fn execute_operation(&mut self, invocation: &CommandInvocation) {
        let plan = invocation
            .arguments
            .get("plan")
            .and_then(near_core::CommandValue::as_str)
            .map(near_core::OperationId::from);
        let conflict = invocation
            .arguments
            .get("conflict")
            .and_then(near_core::CommandValue::as_str)
            .and_then(parse_conflict_action);
        let (Some(plan), Some(conflict)) = (plan, conflict) else {
            "Invalid operation confirmation".clone_into(&mut self.status);
            return;
        };
        let high_impact_confirmed = matches!(
            invocation.arguments.get("high_impact_confirmed"),
            Some(near_core::CommandValue::Boolean(true))
        );
        self.start_operation(plan, conflict, true, high_impact_confirmed);
    }

    fn start_operation(
        &mut self,
        plan: near_core::OperationId,
        conflict: ConflictAction,
        confirmed: bool,
        high_impact_confirmed: bool,
    ) {
        let Some(service) = &self.operations else {
            "No operation service is configured".clone_into(&mut self.status);
            return;
        };
        if self.operation_task.is_some() {
            "Another operation is already running".clone_into(&mut self.status);
            return;
        }
        let service = Arc::clone(service);
        let generation = self.generation;
        let retry = ElevatedRetry {
            plan: plan.clone(),
            authorization: ExecutionAuthorization {
                context_generation: generation,
                confirmed,
                high_impact_confirmed,
            },
            conflict: ConflictDecision {
                action: conflict,
                scope: DecisionScope::Remaining,
            },
            elevated: false,
        };
        self.overlay = None;
        match self.tasks.spawn(move |cancellation| {
            let result = service
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .execute(
                    &plan,
                    ExecutionAuthorization {
                        context_generation: generation,
                        confirmed,
                        high_impact_confirmed,
                    },
                    &cancellation,
                    ConflictDecision {
                        action: conflict,
                        scope: DecisionScope::Remaining,
                    },
                );
            WorkspaceTaskResult::Operation { result }
        }) {
            Ok(task) => {
                self.operation_contexts.insert(task.id().0, retry);
                self.track_visible_task(
                    &task,
                    "operation",
                    TaskRecord::running(&task, "File operation", None, "Running in background"),
                );
                self.operation_task = Some(task);
                "Operation running in background".clone_into(&mut self.status);
            }
            Err(error) => self.status = format!("Cannot queue operation: {error}"),
        }
    }

    fn start_elevated_retry(&mut self) {
        let Some(service) = &self.operations else {
            "No operation service is configured".clone_into(&mut self.status);
            return;
        };
        if self.operation_task.is_some() {
            "Another operation is already running".clone_into(&mut self.status);
            return;
        }
        let Some(mut retry) = self.elevated_retry.take() else {
            "No permission-failed operation is available for elevation"
                .clone_into(&mut self.status);
            return;
        };
        retry.elevated = true;
        let task_context = retry.clone();
        let service = Arc::clone(service);
        let plan = retry.plan.clone();
        let authorization = retry.authorization;
        let conflict = retry.conflict;
        match self.tasks.spawn(move |_| WorkspaceTaskResult::Operation {
            result: service
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .execute_elevated(&plan, authorization, conflict),
        }) {
            Ok(task) => {
                self.track_visible_task(
                    &task,
                    "elevated-operation",
                    TaskRecord::running(
                        &task,
                        "Elevated file operation",
                        None,
                        "Waiting for platform authorization",
                    ),
                );
                self.operation_contexts.insert(task.id().0, task_context);
                self.operation_task = Some(task);
                "Platform authorization requested for the preserved operation plan"
                    .clone_into(&mut self.status);
            }
            Err(error) => {
                self.elevated_retry = Some(retry);
                self.status = format!("Cannot queue elevated operation: {error}");
            }
        }
    }

    fn refresh_collections(&mut self) {
        let left_location = self.left.location().clone();
        let right_location = self.right.location().clone();
        if !self.refresh_temporary_panel(FocusedPanel::Left)
            && !self.refresh_generated_panel(FocusedPanel::Left)
        {
            if let Some(provider) = self.providers.for_location(&left_location) {
                self.start_listing(FocusedPanel::Left, &provider, &left_location);
            } else {
                self.status = format!("No provider for {}", left_location.as_str());
            }
        }
        if !self.refresh_temporary_panel(FocusedPanel::Right)
            && !self.refresh_generated_panel(FocusedPanel::Right)
        {
            if let Some(provider) = self.providers.for_location(&right_location) {
                self.start_listing(FocusedPanel::Right, &provider, &right_location);
            } else {
                self.status = format!("No provider for {}", right_location.as_str());
            }
        }
    }

    fn move_menu(&mut self, rows: isize) {
        match &mut self.overlay {
            Some(Overlay::Menu(menu)) => {
                menu.configure_interaction(
                    self.settings.interface.menu_wrap_navigation,
                    self.settings.interface.dialog_wrap_focus,
                );
                let command = if rows < 0 {
                    "near.menu.up"
                } else {
                    "near.menu.down"
                };
                let action = ActionContext::default();
                menu.update(
                    &SurfaceEvent::Command(CommandInvocation {
                        id: CommandId::from(command),
                        arguments: BTreeMap::default(),
                    }),
                    &mut UpdateContext { action: &action },
                );
            }
            Some(Overlay::CommandPalette {
                selected,
                entries,
                search,
            }) => {
                let visible = palette_visible_indices(entries, search);
                if !visible.is_empty() {
                    let position = visible
                        .iter()
                        .position(|index| index == selected)
                        .unwrap_or_default();
                    let next = if self.settings.interface.menu_wrap_navigation
                        && rows < 0
                        && position == 0
                    {
                        visible.len() - 1
                    } else if self.settings.interface.menu_wrap_navigation
                        && rows > 0
                        && position + 1 == visible.len()
                    {
                        0
                    } else {
                        position
                            .saturating_add_signed(rows)
                            .min(visible.len().saturating_sub(1))
                    };
                    *selected = visible[next];
                }
            }
            _ => {}
        }
    }

    fn navigate_menu(&mut self, command: &str) {
        match &mut self.overlay {
            Some(Overlay::Menu(menu)) => {
                menu.configure_interaction(
                    self.settings.interface.menu_wrap_navigation,
                    self.settings.interface.dialog_wrap_focus,
                );
                let action = ActionContext::default();
                menu.update(
                    &SurfaceEvent::Command(CommandInvocation {
                        id: CommandId::from(command),
                        arguments: BTreeMap::default(),
                    }),
                    &mut UpdateContext { action: &action },
                );
            }
            Some(Overlay::CommandPalette {
                selected,
                entries,
                search,
            }) => {
                let visible = palette_visible_indices(entries, search);
                if visible.is_empty() {
                    return;
                }
                let position = visible
                    .iter()
                    .position(|index| index == selected)
                    .unwrap_or_default();
                let position = match command {
                    "near.menu.first" => 0,
                    "near.menu.last" => visible.len() - 1,
                    "near.menu.page-up" => position.saturating_sub(10),
                    "near.menu.page-down" => position.saturating_add(10).min(visible.len() - 1),
                    _ => position,
                };
                *selected = visible[position];
            }
            _ => {}
        }
    }

    fn activate_menu(&mut self) {
        match self.overlay.take() {
            Some(Overlay::Menu(mut menu)) => {
                menu.configure_interaction(
                    self.settings.interface.menu_wrap_navigation,
                    self.settings.interface.dialog_wrap_focus,
                );
                let enabled = menu
                    .items()
                    .get(menu.selected())
                    .is_some_and(|item| item.enabled);
                if !enabled {
                    if let Some(item) = menu.items().get(menu.selected()) {
                        self.status = format!("Unavailable: {} — {}", item.label, item.description);
                    }
                    self.overlay = Some(Overlay::Menu(menu));
                    return;
                }
                let action = ActionContext::default();
                let result = menu.update(
                    &SurfaceEvent::Command(CommandInvocation {
                        id: CommandId::from("near.menu.activate"),
                        arguments: BTreeMap::default(),
                    }),
                    &mut UpdateContext { action: &action },
                );
                let command = result.command;
                if let Some(command) = command {
                    self.dispatch(&command);
                    if self.overlay.is_some() {
                        self.overlay_history.push(Overlay::Menu(menu));
                    }
                }
            }
            Some(Overlay::CommandPalette {
                selected, entries, ..
            }) => {
                let invocation = entries.get(selected).map(|entry| CommandInvocation {
                    id: entry.id.clone(),
                    arguments: BTreeMap::default(),
                });
                if let Some(invocation) = invocation {
                    self.dispatch(&invocation);
                }
            }
            overlay => self.overlay = overlay,
        }
    }

    pub(crate) fn render(
        &self,
        frame: &mut Frame<'_>,
        theme: &SemanticTheme,
        keymap: &Keymap,
        roles: &mut RoleBuffer,
    ) {
        let area = frame.area();
        self.viewport.set((area.width, area.height));
        roles.fill(area, "workspace.background");
        frame.render_widget(
            Block::default().style(theme.style("workspace.background")),
            area,
        );
        if self.terminal_is_full_screen() {
            if self.settings.interface.show_keybar && area.height > 1 {
                let vertical = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(area);
                self.render_terminal_view(frame, vertical[0], true, theme, roles);
                self.render_terminal_workspace_keybar(frame, vertical[1], theme, keymap, roles);
            } else {
                self.render_terminal_view(frame, area, true, theme, roles);
            }
            self.render_overlay(frame, area, theme, roles);
            return;
        }
        if let Some(editor) = self.active_editor() {
            Self::render_surface(frame, area, editor, true, theme, roles);
            self.render_overlay(frame, area, theme, roles);
            return;
        }
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),
                Constraint::Length(
                    if self.settings.interface.show_status_line || self.filename_lookup.is_some() {
                        theme.density().status_rows.max(1)
                    } else {
                        0
                    },
                ),
                Constraint::Length(self.command_dock_rows()),
                Constraint::Length(if self.settings.interface.show_keybar {
                    theme.density().keybar_rows.max(1)
                } else {
                    0
                }),
            ])
            .split(area);
        if self.temporary_panel_is_full_screen(self.focused) {
            self.render_panel(frame, vertical[0], self.focused, theme, roles);
            if self.settings.interface.show_status_line || self.filename_lookup.is_some() {
                self.render_status(frame, vertical[1], theme, roles);
            }
            self.render_command_line(frame, vertical[2], theme, roles);
            if self.settings.interface.show_keybar {
                self.render_keybar(frame, vertical[3], theme, keymap, roles);
            }
            self.render_overlay(frame, area, theme, roles);
            return;
        }
        let geometry = self
            .panel_layout
            .geometry(vertical[0].width, vertical[0].height, 8, 3);
        let left_panel = Rect::new(
            vertical[0].x,
            vertical[0].y,
            geometry.first_width,
            geometry.pane_height,
        );
        let right_panel = Rect::new(
            vertical[0].x.saturating_add(geometry.first_width),
            vertical[0].y,
            geometry.second_width,
            geometry.pane_height,
        );
        self.render_panel(frame, left_panel, FocusedPanel::Left, theme, roles);
        self.render_panel(frame, right_panel, FocusedPanel::Right, theme, roles);
        if self.settings.interface.show_status_line || self.filename_lookup.is_some() {
            self.render_status(frame, vertical[1], theme, roles);
        }
        self.render_command_line(frame, vertical[2], theme, roles);
        if self.settings.interface.show_keybar {
            self.render_keybar(frame, vertical[3], theme, keymap, roles);
        }
        self.render_overlay(frame, area, theme, roles);
    }

    fn render_panel(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        panel: FocusedPanel,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) {
        if self.terminal_pane() == Some(panel) {
            self.render_terminal_view(frame, area, self.focused == panel, theme, roles);
            return;
        }
        let focused = if self.panel_type(panel) == PanelType::QuickView {
            self.quick_view_interactive
        } else {
            self.focused == panel && !self.quick_view_interactive
        };
        match self.panel_type(panel) {
            PanelType::File => {
                Self::render_surface(frame, area, self.panel(panel), focused, theme, roles);
            }
            PanelType::Tree => {
                let tree = self.tree_panel(panel);
                Self::render_surface(frame, area, &tree, focused, theme, roles);
            }
            PanelType::Information => {
                let information = self.information_panel(panel);
                Self::render_surface(frame, area, &information, focused, theme, roles);
            }
            PanelType::QuickView => {
                if let Some(viewer) = &self.quick_view {
                    Self::render_surface(frame, area, viewer, focused, theme, roles);
                } else {
                    let viewer = ViewerSurface::text(
                        "near-fm.quick-view",
                        "Quick view",
                        "No preview is loaded",
                    );
                    Self::render_surface(frame, area, &viewer, focused, theme, roles);
                }
            }
        }
    }

    fn render_surface(
        frame: &mut Frame<'_>,
        area: Rect,
        surface: &dyn Surface,
        focused: bool,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) {
        let action = ActionContext::default();
        let scene = surface.scene(
            SceneRect::new(area.x, area.y, area.width, area.height),
            &RenderContext {
                focused,
                action: &action,
            },
        );
        render_scene(frame, &scene, theme, roles);
    }

    fn render_status(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) {
        if self.render_filename_lookup_status(frame, area, theme, roles) {
            return;
        }
        let selected = self
            .focused_panel()
            .entries()
            .iter()
            .filter(|item| item.selected)
            .count();
        let text = format!(" {} | selected: {selected} ", self.status);
        frame.render_widget(
            Paragraph::new(truncate(&text, area.width as usize))
                .style(theme.style("status.normal")),
            area,
        );
        roles.fill(area, "status.normal");
    }

    fn render_command_line(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) {
        if area.height == 0 {
            return;
        }
        if self.render_shell_dock(frame, area, theme, roles) {
            return;
        }
        let prompt = if self.pending_sequence.is_empty() {
            format!(
                "{}> {}_",
                self.focused_panel().location().display_compact(),
                self.command_line.buffer()
            )
        } else {
            format!("keys: {}", self.pending_sequence)
        };
        frame.render_widget(
            Paragraph::new(truncate(&prompt, area.width as usize)).style(theme.style("text")),
            area,
        );
        roles.fill(area, "text");
    }

    fn render_keybar(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        keymap: &Keymap,
        roles: &mut RoleBuffer,
    ) {
        if self.render_filename_lookup_keybar(frame, area, theme, roles) {
            return;
        }
        if self.terminal_pane().is_some()
            && self.render_terminal_workspace_keybar(frame, area, theme, keymap, roles)
        {
            return;
        }
        let hints =
            keymap.function_hints_for_modifiers(&self.active_contexts(), self.keybar_modifiers());
        let mut spans = Vec::new();
        let mut column = area.x;
        for (slot, binding) in hints {
            let key = format!("{slot}");
            let label = format!("{} ", short_description(binding));
            spans.push(Span::styled(
                key.clone(),
                theme.style("keybar.key").add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(label.clone(), theme.style("keybar.label")));
            let key_width = u16::try_from(key.chars().count()).unwrap_or(u16::MAX);
            let label_width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
            roles.fill(Rect::new(column, area.y, key_width, 1), "keybar.key");
            column = column.saturating_add(key_width);
            roles.fill(Rect::new(column, area.y, label_width, 1), "keybar.label");
            column = column.saturating_add(label_width);
        }
        if column < area.right() {
            roles.fill(
                Rect::new(column, area.y, area.right() - column, 1),
                "keybar.label",
            );
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    #[allow(clippy::too_many_lines)]
    fn render_overlay(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &SemanticTheme,
        roles: &mut RoleBuffer,
    ) {
        let Some(overlay) = &self.overlay else {
            return;
        };
        match overlay {
            Overlay::Menu(menu) => {
                let popup = menu_popup(area, menu);
                let action = ActionContext::default();
                let scene = menu.scene(
                    SceneRect::new(popup.x, popup.y, popup.width, popup.height),
                    &RenderContext {
                        focused: true,
                        action: &action,
                    },
                );
                frame.render_widget(Clear, popup);
                render_scene(frame, &scene, theme, roles);
                if let Some(scene) =
                    menu.main_menu_scene(SceneRect::new(area.x, area.y, area.width, 1))
                {
                    let bar = Rect::new(area.x, area.y, area.width, 1);
                    frame.render_widget(Clear, bar);
                    render_scene(frame, &scene, theme, roles);
                }
            }
            Overlay::CommandHistory(surface) => {
                let popup = centered(area, 76, 20);
                let action = self.action_context();
                let scene = surface.scene(
                    SceneRect::new(popup.x, popup.y, popup.width, popup.height),
                    &RenderContext {
                        focused: true,
                        action: &action,
                    },
                );
                frame.render_widget(Clear, popup);
                render_scene(frame, &scene, theme, roles);
            }
            Overlay::FolderHistory(surface) => {
                let popup = centered(area, 82, 22);
                let action = self.action_context();
                let scene = surface.scene(
                    SceneRect::new(popup.x, popup.y, popup.width, popup.height),
                    &RenderContext {
                        focused: true,
                        action: &action,
                    },
                );
                frame.render_widget(Clear, popup);
                render_scene(frame, &scene, theme, roles);
            }
            Overlay::ResourceHistory(surface) => {
                let popup = centered(area, 88, 22);
                let action = self.action_context();
                let scene = surface.scene(
                    SceneRect::new(popup.x, popup.y, popup.width, popup.height),
                    &RenderContext {
                        focused: true,
                        action: &action,
                    },
                );
                frame.render_widget(Clear, popup);
                render_scene(frame, &scene, theme, roles);
            }
            Overlay::CommandPalette {
                selected,
                entries,
                search,
            } => {
                let popup = centered(area, 66, 20);
                let inner = inset(popup, 1);
                roles.fill(popup, "dialog.border");
                roles.fill(inner, "dialog.background");
                frame.render_widget(Clear, popup);
                let items = palette_visible_indices(entries, search)
                    .into_iter()
                    .enumerate()
                    .map(|(visible_row, index)| {
                        let entry = &entries[index];
                        let role = if index == *selected {
                            "control.focused"
                        } else {
                            "dialog.background"
                        };
                        if let Ok(row_offset) = u16::try_from(visible_row)
                            && row_offset < inner.height
                        {
                            roles.fill(
                                Rect::new(inner.x, inner.y + row_offset, inner.width, 1),
                                role,
                            );
                        }
                        let content = format!(" {:<16} {}", entry.shortcut, entry.title);
                        let spans = search
                            .segments(&content)
                            .into_iter()
                            .map(|(segment, matched)| {
                                Span::styled(
                                    segment.to_owned(),
                                    theme.style(if matched { "selection.match" } else { role }),
                                )
                            })
                            .collect::<Vec<_>>();
                        ListItem::new(Line::from(spans))
                    });
                frame.render_widget(
                    List::new(items).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(border_type(theme))
                            .title(" Command Palette ")
                            .style(theme.style("dialog.border")),
                    ),
                    popup,
                );
                if search.is_active() {
                    frame.render_widget(
                        Paragraph::new(search.prompt()).style(theme.style("text")),
                        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
                    );
                }
            }
            Overlay::Surface(surface) => {
                let surface_area = match surface.presentation() {
                    SurfacePresentation::Modal => centered(area, 70, 16),
                    SurfacePresentation::FullScreen => area,
                };
                let action = self.action_context();
                let scene = surface.scene(
                    SceneRect::new(
                        surface_area.x,
                        surface_area.y,
                        surface_area.width,
                        surface_area.height,
                    ),
                    &RenderContext {
                        focused: true,
                        action: &action,
                    },
                );
                frame.render_widget(Clear, surface_area);
                render_scene(frame, &scene, theme, roles);
            }
            Overlay::Message { title, body } => {
                let popup = centered(area, 70, 16);
                roles.fill(popup, "dialog.border");
                roles.fill(inset(popup, 1), "dialog.background");
                frame.render_widget(Clear, popup);
                frame.render_widget(
                    Paragraph::new(body.as_str())
                        .wrap(Wrap { trim: false })
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_type(border_type(theme))
                                .title(format!(" {title} "))
                                .style(theme.style("dialog.border")),
                        )
                        .style(theme.style("dialog.background")),
                    popup,
                );
            }
        }
    }

    fn effective_help_surface(&self, keymap: Option<&Keymap>, start: &str) -> HelpSurface {
        let contexts = self.active_contexts();
        let mut bindings = keymap.map_or_else(Vec::new, |keymap| keymap.bindings_for(&contexts));
        bindings.sort_by(|left, right| {
            left.function_hint
                .is_none()
                .cmp(&right.function_hint.is_none())
                .then(left.function_hint.cmp(&right.function_hint))
                .then_with(|| {
                    format_key_sequence(&left.sequence).cmp(&format_key_sequence(&right.sequence))
                })
        });
        let context_entries = bindings
            .into_iter()
            .map(|binding| HelpEntry {
                keys: format_key_sequence(&binding.sequence),
                command: binding.invocation.id.to_string(),
                description: binding
                    .description
                    .clone()
                    .unwrap_or_else(|| binding.invocation.id.to_string()),
            })
            .collect::<Vec<_>>();
        let mut topics = vec![
            HelpTopic::new(
                "context",
                "Context Help",
                format!(
                    "Effective commands for {}.",
                    contexts
                        .first()
                        .map_or("the active workspace", ContextId::as_str)
                ),
                context_entries,
            )
            .with_links(vec![
                HelpLink {
                    label: "Contents".to_owned(),
                    target: "contents".to_owned(),
                    description: "Browse all core help categories".to_owned(),
                },
                HelpLink {
                    label: "Extensions".to_owned(),
                    target: "extensions".to_owned(),
                    description: "Discover installed extension commands and prefixes".to_owned(),
                },
            ]),
        ];

        let mut categories = BTreeMap::<String, Vec<HelpEntry>>::new();
        for descriptor in self.registry.descriptors() {
            if self.extension_commands.contains_key(&descriptor.id) {
                continue;
            }
            let category = descriptor
                .category
                .first()
                .cloned()
                .unwrap_or_else(|| "General".to_owned());
            categories.entry(category).or_default().push(HelpEntry {
                keys: keymap
                    .and_then(|keymap| {
                        keymap
                            .bindings_for(&contexts)
                            .into_iter()
                            .find(|binding| binding.invocation.id == descriptor.id)
                    })
                    .map_or_else(String::new, |binding| {
                        format_key_sequence(&binding.sequence)
                    }),
                command: descriptor.id.to_string(),
                description: descriptor.description.clone(),
            });
        }
        let mut contents_links = vec![HelpLink {
            label: "Current context".to_owned(),
            target: "context".to_owned(),
            description: "Effective commands and rebound keys".to_owned(),
        }];
        for (category, mut entries) in categories {
            entries.sort_by(|left, right| left.command.cmp(&right.command));
            let id = format!(
                "category:{}",
                category.to_ascii_lowercase().replace(' ', "-")
            );
            contents_links.push(HelpLink {
                label: category.clone(),
                target: id.clone(),
                description: format!("{} commands", entries.len()),
            });
            topics.push(
                HelpTopic::new(
                    id,
                    format!("{category} Commands"),
                    "Semantic commands, effective keys, and command descriptions.",
                    entries,
                )
                .with_links(vec![HelpLink {
                    label: "Contents".to_owned(),
                    target: "contents".to_owned(),
                    description: "Return to help contents".to_owned(),
                }]),
            );
        }

        let mut extension_links = Vec::new();
        for (extension_id, extension) in &self.extensions {
            let topic_id = format!("extension:{extension_id}");
            let mut entries = self
                .extension_commands
                .iter()
                .filter(|(_, owner)| *owner == extension_id)
                .filter_map(|(command, _)| {
                    self.registry
                        .descriptors()
                        .find(|descriptor| descriptor.id == *command)
                })
                .map(|descriptor| HelpEntry {
                    keys: String::new(),
                    command: descriptor.id.to_string(),
                    description: descriptor.description.clone(),
                })
                .collect::<Vec<_>>();
            entries.extend(
                self.command_prefixes
                    .iter()
                    .filter(|(_, registered)| {
                        matches!(
                            &registered.owner,
                            CommandPrefixOwner::Extension { extension, .. }
                                if extension == extension_id
                        )
                    })
                    .map(|(prefix, registered)| HelpEntry {
                        keys: format!("{prefix}:"),
                        command: "command prefix".to_owned(),
                        description: registered.description.clone(),
                    }),
            );
            extension_links.push(HelpLink {
                label: extension_id.clone(),
                target: topic_id.clone(),
                description: format!("{} contributed commands and prefixes", entries.len()),
            });
            let mut topic_links = vec![HelpLink {
                label: "Extensions".to_owned(),
                target: "extensions".to_owned(),
                description: "Return to installed extensions".to_owned(),
            }];
            if let Ok(help_topics) = extension.help_topics() {
                for help_topic in help_topics {
                    let authored_id = format!("extension:{extension_id}:{}", help_topic.id);
                    topic_links.push(HelpLink {
                        label: help_topic.title.clone(),
                        target: authored_id.clone(),
                        description: "Extension-authored help".to_owned(),
                    });
                    topics.push(
                        HelpTopic::new(authored_id, help_topic.title, help_topic.body, Vec::new())
                            .with_source(format!("extension:{extension_id}"))
                            .with_links(vec![HelpLink {
                                label: extension_id.clone(),
                                target: topic_id.clone(),
                                description: "Return to extension overview".to_owned(),
                            }]),
                    );
                }
            }
            topics.push(
                HelpTopic::new(
                    topic_id,
                    format!("Extension: {extension_id}"),
                    "Commands and command-line prefixes contributed by this installed extension.",
                    entries,
                )
                .with_source(format!("extension:{extension_id}"))
                .with_links(topic_links),
            );
        }
        if extension_links.is_empty() {
            extension_links.push(HelpLink {
                label: "No extensions installed".to_owned(),
                target: "contents".to_owned(),
                description: "Return to core help contents".to_owned(),
            });
        }
        contents_links.push(HelpLink {
            label: "Extensions".to_owned(),
            target: "extensions".to_owned(),
            description: format!("{} installed extension topics", self.extensions.len()),
        });
        topics.push(
            HelpTopic::new(
                "extensions",
                "Extension Help",
                "Installed extension commands and command-line prefixes.",
                Vec::new(),
            )
            .with_source("extension registry")
            .with_links(extension_links),
        );
        topics.push(
            HelpTopic::new(
                "contents",
                "Help Contents",
                format!(
                    "Near {} help generated from the running command registry, extension catalog, and keymap.",
                    env!("CARGO_PKG_VERSION")
                ),
                Vec::new(),
            )
            .with_links(contents_links),
        );
        HelpSurface::with_topics("near-fm.help", topics, start)
    }

    pub(crate) fn action_context(&self) -> ActionContext {
        if self.terminal_owns_focus()
            && let Some(terminal) = self.active_terminal_surface()
        {
            let state = terminal.state();
            let peer = self.terminal_pane().map(|pane| {
                self.panel(match pane {
                    FocusedPanel::Left => FocusedPanel::Right,
                    FocusedPanel::Right => FocusedPanel::Left,
                })
            });
            return ActionContext {
                focused_surface: Some(terminal.id()),
                peer_surface: peer.map(Surface::id),
                current: state.current,
                selected: state.selected,
                location: state.location,
                peer_location: peer.map(|surface| surface.location().clone()),
                capabilities: terminal.capabilities(),
                peer_capabilities: peer.map_or_else(CapabilitySet::default, Surface::capabilities),
            };
        }
        let (focused, peer) = match self.focused {
            FocusedPanel::Left => (&self.left, &self.right),
            FocusedPanel::Right => (&self.right, &self.left),
        };
        let focused_state = focused.state();
        let peer_state = peer.state();
        let mut capabilities = focused.capabilities();
        for capability in self
            .providers
            .container_capabilities(
                &focused_state
                    .location
                    .clone()
                    .unwrap_or_else(|| focused.location().clone()),
            )
            .iter()
        {
            capabilities.insert(capability.clone());
        }
        if let Some(current) = &focused_state.current
            && let Some(provider) = self.providers.get(&current.provider)
        {
            for capability in provider.capabilities(current).iter() {
                capabilities.insert(capability.clone());
            }
        }
        ActionContext {
            focused_surface: Some(focused.id()),
            peer_surface: Some(peer.id()),
            current: focused_state.current,
            selected: focused_state.selected,
            location: focused_state.location,
            peer_location: peer_state.location,
            capabilities,
            peer_capabilities: self.providers.container_capabilities(peer.location()),
        }
    }

    #[cfg(test)]
    fn main_menu(&self) -> MenuSurface {
        let items = [
            ("&Left", "Left panel commands", "near.menu.left"),
            (
                "&Files",
                "Current and selected resources",
                "near.menu.files",
            ),
            (
                "&Commands",
                "Search, history, automation, and tools",
                "near.menu.commands",
            ),
            (
                "&Options",
                "Configuration and extension catalogs",
                "near.menu.options",
            ),
            ("&Right", "Right panel commands", "near.menu.right"),
        ]
        .into_iter()
        .map(|(label, description, command)| self.menu_item(label, description, command))
        .collect();
        MenuSurface::new("near-fm.menu", "Near Menu", items)
    }

    fn open_main_menu_category(&mut self, category: MainMenuCategory) {
        if let MainMenuCategory::Left | MainMenuCategory::Right = category {
            self.focused = if category == MainMenuCategory::Left {
                FocusedPanel::Left
            } else {
                FocusedPanel::Right
            };
        }
        let menu = match category {
            MainMenuCategory::Left => self.panel_menu(FocusedPanel::Left),
            MainMenuCategory::Files => self.files_menu(),
            MainMenuCategory::Commands => self.commands_menu(),
            MainMenuCategory::Options => self.options_menu(),
            MainMenuCategory::Right => self.panel_menu(FocusedPanel::Right),
        }
        .with_main_menu(
            ["Left", "Files", "Commands", "Options", "Right"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            category.index(),
        );
        self.overlay = Some(Overlay::Menu(menu));
    }

    fn switch_main_menu_category(&mut self, delta: isize) {
        let Some(index) = self.overlay.as_ref().and_then(|overlay| match overlay {
            Overlay::Menu(menu) => menu.main_menu_index(),
            _ => None,
        }) else {
            return;
        };
        let count = isize::try_from(MainMenuCategory::ALL.len()).unwrap_or(5);
        let next = isize::try_from(index)
            .unwrap_or_default()
            .saturating_add(delta)
            .rem_euclid(count);
        self.open_main_menu_category(MainMenuCategory::from_index(
            usize::try_from(next).unwrap_or_default(),
        ));
    }

    fn switch_main_menu_panel(&mut self) {
        let category = match self
            .overlay
            .as_ref()
            .and_then(|overlay| match overlay {
                Overlay::Menu(menu) => menu.main_menu_index(),
                _ => None,
            })
            .map(MainMenuCategory::from_index)
        {
            Some(MainMenuCategory::Left) => MainMenuCategory::Right,
            Some(MainMenuCategory::Right) => MainMenuCategory::Left,
            _ if self.focused == FocusedPanel::Left => MainMenuCategory::Right,
            _ => MainMenuCategory::Left,
        };
        self.open_main_menu_category(category);
    }

    fn panel_menu(&self, panel: FocusedPanel) -> MenuSurface {
        let side = match panel {
            FocusedPanel::Left => "Left",
            FocusedPanel::Right => "Right",
        };
        let active = &self.panel(panel).view_mode().id;
        let mut items = self
            .settings
            .panel_modes
            .modes()
            .iter()
            .map(|mode| MenuItem {
                label: format!(
                    "{} {}",
                    if &mode.id == active { "√" } else { " " },
                    far_panel_mode_label(&mode.id, &mode.label)
                ),
                description: mode
                    .columns
                    .iter()
                    .map(|column| format!("{:?}", column.kind).to_lowercase())
                    .collect::<Vec<_>>()
                    .join(", "),
                command: CommandInvocation {
                    id: CommandId::from("near.panel.view-mode.set"),
                    arguments: BTreeMap::from([(
                        "id".to_owned(),
                        near_core::CommandValue::String(mode.id.clone()),
                    )]),
                },
                enabled: true,
            })
            .collect::<Vec<_>>();
        items.extend(
            [
                (
                    "&Information panel",
                    "Change this side to panel metadata",
                    "near.panel.toggle-information",
                ),
                (
                    "&Tree panel",
                    "Change this side to hierarchical navigation",
                    "near.panel.toggle-tree",
                ),
                (
                    "&Quick view",
                    "Preview the peer selection on this side",
                    "near.panel.toggle-quick-view",
                ),
                (
                    "&Sort modes",
                    "Ordering and sort modifiers",
                    "near.collection.sort.menu",
                ),
                (
                    "File panel fi&lter",
                    "Named metadata and mask filters",
                    "near.filters.show",
                ),
                (
                    "&Re-read",
                    "Reload both provider panels",
                    "near.panel.refresh",
                ),
                (
                    "&Change location",
                    "Choose a provider, filesystem root, or mount",
                    "near.provider.choose",
                ),
                (
                    "Configure la&youts",
                    "Open every built-in and custom panel layout",
                    "near.panel.view-mode.menu",
                ),
            ]
            .into_iter()
            .map(|(label, description, command)| self.menu_item(label, description, command)),
        );
        MenuSurface::new(
            format!("near-fm.menu.{}", side.to_ascii_lowercase()),
            format!("{side} Menu"),
            items,
        )
    }

    fn files_menu(&self) -> MenuSurface {
        let items = [
            ("&View", "View the current resource", "near.resource.view"),
            ("&Edit", "Edit the current resource", "near.resource.edit"),
            (
                "&Copy",
                "Copy selected/current resources to the peer",
                "near.resource.copy-to-peer",
            ),
            (
                "&Rename or move",
                "Move selected/current resources to the peer",
                "near.resource.move-to-peer",
            ),
            (
                "Rename in &place",
                "Rename selected/current resources without changing folders",
                "near.resource.rename",
            ),
            ("Create &link", "Create a typed link", "near.resource.link"),
            (
                "&Make folder",
                "Create a directory",
                "near.fs.create-directory",
            ),
            ("&Trash", "Plan reversible trash", "near.resource.trash"),
            (
                "Restore last Tras&h",
                "Restore completed items to their recorded original locations",
                "near.resource.restore-last-trash",
            ),
            (
                "&Delete permanently",
                "Irreversible delete with two-step confirmation",
                "near.resource.delete",
            ),
            (
                "&Wipe files",
                "Overwrite regular files, then delete",
                "near.resource.wipe",
            ),
            (
                "Add to &archive",
                "Create or update a ZIP archive from selected resources",
                "near.archive.create",
            ),
            (
                "File attrib&utes",
                "Permissions, ownership, and timestamps",
                "near.resource.attributes",
            ),
            (
                "Appl&y command",
                "Expand and run a command over resources",
                "near.operation.apply-command",
            ),
            (
                "Descri&be files",
                "Edit file descriptions",
                "near.resource.description",
            ),
            (
                "&Select group",
                "Select resources using include and exclude masks",
                "near.selection.select-mask",
            ),
            (
                "Deselect &group",
                "Remove matching resources from the selection",
                "near.selection.unselect-mask",
            ),
            (
                "&Invert selection",
                "Toggle every selectable resource",
                "near.selection.invert",
            ),
            (
                "Restore selectio&n",
                "Restore the saved resource set",
                "near.selection.restore",
            ),
            (
                "Selecti&on commands",
                "Same-name, same-extension, saved sets, and comparison",
                "near.selection.menu",
            ),
        ]
        .into_iter()
        .map(|(label, description, command)| self.menu_item(label, description, command))
        .collect();
        MenuSurface::new("near-fm.menu.files", "Files Menu", items)
    }

    fn commands_menu(&self) -> MenuSurface {
        let items = [
            (
                "&Find files",
                "Recursive provider search",
                "near.search.start",
            ),
            (
                "&Command history",
                "Searchable command-line history",
                "near.command-line.history-show",
            ),
            (
                "File &view history",
                "Viewed and edited resource history",
                "near.history.menu",
            ),
            (
                "F&olders history",
                "Searchable folder navigation history",
                "near.location.history-show",
            ),
            (
                "&Swap panels",
                "Exchange the left and right panel surfaces",
                "near.workspace.swap-peers",
            ),
            (
                "Co&mpare folders",
                "Select unique and changed peer entries",
                "near.selection.compare-folders",
            ),
            (
                "&User menu",
                "Global typed automation",
                "near.user-menu.global",
            ),
            (
                "&Local user menu",
                "Location-oriented automation",
                "near.user-menu.local",
            ),
            (
                "File &associations",
                "View, edit, and execute handlers",
                "near.resource.associations",
            ),
            (
                "File panel f&ilter",
                "Control the focused panel contents",
                "near.filters.show",
            ),
            (
                "&Extension commands",
                "Installed components and commands",
                "near.extensions.show",
            ),
            (
                "Scree&ns list",
                "Panels, editors, and user screen",
                "near.screen.list",
            ),
            (
                "Task &queue",
                "Progress and cancellation",
                "near.demo.tasks",
            ),
            (
                "Hotplug &devices",
                "List and safely disconnect attached devices",
                "near.devices.show",
            ),
            (
                "&Keep search panel",
                "Keep the current result collection for this session",
                "near.search.keep-panel",
            ),
            (
                "Saved search &panels",
                "Reopen a retained search result collection",
                "near.search.panels",
            ),
            (
                "Temporar&y panels",
                "Open one of ten mutable reference panels",
                "near.temp-panel.list",
            ),
            (
                "Macro manage&r",
                "List, play, edit, bind, delete, and diagnose macros",
                "near.macro.manage",
            ),
            (
                "&Terminal tabs",
                "Persistent shells, panes, tabs, and zoom",
                "near.terminal.menu",
            ),
        ]
        .into_iter()
        .map(|(label, description, command)| self.menu_item(label, description, command))
        .collect();
        MenuSurface::new("near-fm.menu.commands", "Commands Menu", items)
    }

    fn options_menu(&self) -> MenuSurface {
        let items = [
            (
                "&System settings",
                "System and provider behavior in the typed settings catalog",
                "near.settings.show",
            ),
            (
                "&Panel settings",
                "Panel behavior and independent side defaults",
                "near.settings.show",
            ),
            (
                "&Tree settings",
                "Tree panel behavior and indentation",
                "near.settings.show",
            ),
            (
                "&Interface settings",
                "Status, keybar, focus, and interaction behavior",
                "near.settings.show",
            ),
            (
                "&Dialog settings",
                "Dialog focus and interaction behavior",
                "near.settings.show",
            ),
            (
                "&Menu settings",
                "Menu boundaries, filtering, and navigation",
                "near.settings.show",
            ),
            (
                "&Command line settings",
                "Completion, history, and shell behavior",
                "near.settings.show",
            ),
            (
                "&AutoComplete settings",
                "Command-line completion sources",
                "near.settings.show",
            ),
            (
                "C&onfirmations",
                "Operation preview and confirmation policy",
                "near.settings.show",
            ),
            (
                "&File panel modes",
                "Built-in and custom panel layouts",
                "near.panel.view-mode.menu",
            ),
            (
                "File desc&riptions",
                "View the configured folder description",
                "near.folder-description.view",
            ),
            (
                "Fo&lder descriptions",
                "Create or edit folder descriptions",
                "near.folder-description.edit",
            ),
            (
                "&Viewer settings",
                "Internal and external viewer policies",
                "near.settings.show",
            ),
            (
                "&Editor settings",
                "Internal and external editor policies",
                "near.settings.show",
            ),
            (
                "Colors and t&hemes",
                "Live presets and semantic role editor",
                "near.theme.show",
            ),
            (
                "File hi&ghlighting and sort groups",
                "Rules, marks, roles, and sort groups",
                "near.highlighting.report",
            ),
            (
                "T&yped settings catalog",
                "Search every effective setting and its provenance",
                "near.settings.show",
            ),
            (
                "Command prefi&xes",
                "Provider and extension routes",
                "near.command-prefixes.show",
            ),
            (
                "Exte&nsions",
                "Installed components and settings",
                "near.extensions.show",
            ),
            (
                "Help (&1)",
                "Effective bindings and context help",
                "near.help.context",
            ),
            ("A&bout", "Platform information", "near.about.show"),
        ]
        .into_iter()
        .map(|(label, description, command)| {
            let category = match label {
                "&System settings" => Some("System"),
                "&Panel settings" => Some("Panel"),
                "&Tree settings" => Some("Tree"),
                "&Interface settings" => Some("Interface"),
                "&Dialog settings" => Some("Dialog"),
                "&Menu settings" => Some("Menu"),
                "&Command line settings" => Some("Command-line"),
                "&AutoComplete settings" => Some("Completion"),
                "C&onfirmations" => Some("Confirmation"),
                "&Viewer settings" => Some("Viewer"),
                "&Editor settings" => Some("Editor"),
                _ => None,
            };
            category.map_or_else(
                || self.menu_item(label, description, command),
                |category| self.settings_menu_item(label, description, category),
            )
        })
        .collect();
        MenuSurface::new("near-fm.menu.options", "Options Menu", items)
    }

    fn menu_item(&self, label: &str, description: &str, command: &str) -> MenuItem {
        self.menu_item_invocation(
            label,
            description,
            CommandInvocation {
                id: CommandId::from(command),
                arguments: BTreeMap::new(),
            },
        )
    }

    fn settings_menu_item(&self, label: &str, description: &str, category: &str) -> MenuItem {
        self.menu_item_invocation(
            label,
            description,
            CommandInvocation {
                id: CommandId::from("near.settings.show"),
                arguments: BTreeMap::from([(
                    "category".to_owned(),
                    CommandValue::String(category.to_owned()),
                )]),
            },
        )
    }

    fn menu_item_invocation(
        &self,
        label: &str,
        description: &str,
        invocation: CommandInvocation,
    ) -> MenuItem {
        let command = invocation.id.to_string();
        match self
            .registry
            .check(&invocation, &self.action_context())
            .map_err(|error| error.to_string())
            .and_then(|_| self.workspace_command_availability(&command))
        {
            Ok(()) => MenuItem {
                label: label.to_owned(),
                description: description.to_owned(),
                command: invocation,
                enabled: true,
            },
            Err(error) => {
                let reason = error
                    .rsplit_once(": ")
                    .map_or(error.as_str(), |(_, reason)| reason);
                MenuItem {
                    label: label.to_owned(),
                    description: format!("Unavailable: {reason}; {description}"),
                    command: invocation,
                    enabled: false,
                }
            }
        }
    }

    fn workspace_command_availability(&self, command: &str) -> Result<(), String> {
        match command {
            "near.resource.open" => {
                let item = self
                    .focused_panel()
                    .current()
                    .ok_or_else(|| "no current resource".to_owned())?;
                let provider = if is_parent_entry(item) {
                    self.providers.for_location(&item.resource.location)
                } else {
                    self.providers.get(&item.resource.provider)
                };
                provider
                    .map(|_| ())
                    .ok_or_else(|| "no provider is registered for the current resource".to_owned())
            }
            "near.resource.view" => self
                .resource_open_availability(ExternalAction::View, self.settings.viewer.open_policy),
            "near.resource.edit" => self
                .resource_open_availability(ExternalAction::Edit, self.settings.editor.open_policy),
            "near.resource.copy-to-peer"
            | "near.resource.move-to-peer"
            | "near.resource.rename"
            | "near.resource.link"
            | "near.resource.attributes"
            | "near.archive.create" => {
                if self.operations.is_none() {
                    Err("no operation service is configured".to_owned())
                } else {
                    self.actionable_targets_availability()
                }
            }
            "near.resource.trash" | "near.resource.delete" | "near.resource.wipe" => {
                if self.operations.is_none() {
                    return Err("no operation service is configured".to_owned());
                }
                self.actionable_targets_availability()?;
                let mutation = match command {
                    "near.resource.trash" => MutationKind::Trash,
                    "near.resource.delete" => MutationKind::Delete,
                    _ => MutationKind::Wipe,
                };
                let sources = self.canonical_targets();
                if let Some((_, denial)) = self.mutation_denial(&sources, mutation) {
                    Err(denial.reason)
                } else {
                    Ok(())
                }
            }
            "near.resource.restore-last-trash" => {
                if self.operations.is_none() {
                    Err("no operation service is configured".to_owned())
                } else if self.last_trash_restoration.is_empty() {
                    Err("no completed Trash operation is available".to_owned())
                } else {
                    Ok(())
                }
            }
            "near.resource.description" | "near.operation.apply-command" => {
                self.actionable_targets_availability()
            }
            "near.fs.create-directory" if self.operations.is_none() => {
                Err("no operation service is configured".to_owned())
            }
            "near.resource.associations" => {
                let item = self.current_actionable_entry()?;
                let resolver = self
                    .external_tools
                    .as_ref()
                    .ok_or_else(|| "no external handler resolver is configured".to_owned())?;
                let has_match = [
                    ExternalAction::View,
                    ExternalAction::Edit,
                    ExternalAction::Execute,
                ]
                .into_iter()
                .any(|action| {
                    resolver
                        .alternatives(action, &item.resource)
                        .is_ok_and(|items| !items.is_empty())
                });
                if has_match {
                    Ok(())
                } else {
                    Err("no association matches the current resource".to_owned())
                }
            }
            "near.search.keep-panel" => {
                let generated = self.extension_panels.contains_key(&self.focused)
                    || self.searches.get(&self.focused).is_some_and(|state| {
                        self.focused_panel().location() == state.provider.location()
                    });
                if generated {
                    Ok(())
                } else {
                    Err("focused panel is not a generated result panel".to_owned())
                }
            }
            "near.user-menu.global" => self.user_menu_availability(UserMenuScope::Global),
            "near.user-menu.local" => self.user_menu_availability(UserMenuScope::Local),
            "near.theme.show" if self.working_theme.is_none() => {
                Err("no editable runtime theme is configured".to_owned())
            }
            "near.devices.show" if self.removable_devices.is_none() => {
                Err("removable-device provider is unavailable".to_owned())
            }
            "near.panel.refresh" => {
                let available = |panel| {
                    let location = self.panel(panel).location();
                    self.active_temporary_panel_slot(panel).is_some()
                        || self
                            .extension_panels
                            .get(&panel)
                            .is_some_and(|state| location == state.provider.location())
                        || self
                            .searches
                            .get(&panel)
                            .is_some_and(|state| location == state.provider.location())
                        || self.providers.for_location(location).is_some()
                };
                if available(FocusedPanel::Left) && available(FocusedPanel::Right) {
                    Ok(())
                } else {
                    Err("one or both panel providers are unavailable".to_owned())
                }
            }
            "near.folder-description.view" | "near.folder-description.edit"
                if self
                    .providers
                    .for_location(self.focused_panel().location())
                    .is_none() =>
            {
                Err("current panel provider is unavailable".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn current_actionable_entry(&self) -> Result<&CollectionEntry, String> {
        let item = self
            .focused_panel()
            .current()
            .ok_or_else(|| "no current resource".to_owned())?;
        if is_parent_entry(item) {
            Err("parent entry is navigation-only".to_owned())
        } else {
            Ok(item)
        }
    }

    fn actionable_targets_availability(&self) -> Result<(), String> {
        if self.canonical_targets().is_empty() {
            Err("no actionable resource; parent entry is navigation-only".to_owned())
        } else {
            Ok(())
        }
    }

    fn user_menu_availability(&self, scope: UserMenuScope) -> Result<(), String> {
        let context = self
            .user_menu_context()
            .ok_or_else(|| "no current resource for the user menu".to_owned())?;
        let entries = self.user_menus.entries(scope);
        if entries.is_empty() {
            return Err(format!(
                "no {} user-menu entries are configured",
                scope.as_str()
            ));
        }
        if entries
            .iter()
            .any(|entry| entry.predicate.matches_metadata(&context.focused.metadata))
        {
            Ok(())
        } else {
            Err(format!(
                "no {} user-menu entry matches the current resource",
                scope.as_str()
            ))
        }
    }

    fn resource_open_availability(
        &self,
        action: ExternalAction,
        policy: ResourceOpenPolicy,
    ) -> Result<(), String> {
        let item = self
            .focused_panel()
            .current()
            .ok_or_else(|| "no current resource".to_owned())?;
        if is_parent_entry(item) {
            return Err("parent entry is navigation-only".to_owned());
        }
        match policy {
            ResourceOpenPolicy::Internal => {
                let provider = self.providers.get(&item.resource.provider).ok_or_else(|| {
                    format!("no provider registered for {}", item.resource.provider)
                })?;
                let capabilities = provider.capabilities(&item.resource);
                match action {
                    ExternalAction::View => {
                        if matches!(
                            item.metadata.kind,
                            ResourceKind::Directory | ResourceKind::Package
                        ) {
                            return Err(
                                "internal viewer requires a non-container resource".to_owned()
                            );
                        }
                        if !capabilities
                            .iter()
                            .any(|capability| capability.as_str() == "resource.read")
                        {
                            return Err("current resource is not readable".to_owned());
                        }
                    }
                    ExternalAction::Edit => {
                        if item.metadata.kind != ResourceKind::File {
                            return Err("internal editor requires a file".to_owned());
                        }
                        if !capabilities
                            .iter()
                            .any(|capability| capability.as_str() == "resource.write")
                        {
                            return Err("current resource is read-only".to_owned());
                        }
                    }
                    ExternalAction::Execute => {}
                    _ => {}
                }
                Ok(())
            }
            ResourceOpenPolicy::External | ResourceOpenPolicy::Association => {
                let resolver = self
                    .external_tools
                    .as_ref()
                    .ok_or_else(|| "no external tool resolver is configured".to_owned())?;
                if policy == ResourceOpenPolicy::Association {
                    resolver.alternatives(action, &item.resource).map(|_| ())
                } else {
                    resolver
                        .resolve_explained(action, &item.resource)
                        .map(|_| ())
                }
            }
        }
    }

    fn sort_menu(&self) -> MenuSurface {
        let state = self.focused_panel().sort_state();
        let mut items = [
            (SortMode::Name, "near.collection.sort.name"),
            (SortMode::Extension, "near.collection.sort.extension"),
            (SortMode::Modified, "near.collection.sort.modified"),
            (SortMode::Size, "near.collection.sort.size"),
            (SortMode::Created, "near.collection.sort.created"),
            (SortMode::Accessed, "near.collection.sort.accessed"),
            (SortMode::Kind, "near.collection.sort.kind"),
            (SortMode::Owner, "near.collection.sort.owner"),
            (SortMode::Permissions, "near.collection.sort.permissions"),
            (SortMode::Unsorted, "near.collection.sort.unsorted"),
        ]
        .into_iter()
        .map(|(mode, command)| MenuItem {
            label: format!(
                "{} {}",
                if state.mode == mode { "√" } else { " " },
                mode.label()
            ),
            description: format!("Order the focused panel by {}", mode.label().to_lowercase()),
            command: CommandInvocation {
                id: CommandId::from(command),
                arguments: BTreeMap::default(),
            },
            enabled: true,
        })
        .collect::<Vec<_>>();
        items.extend([
            sort_toggle_item(
                "Reverse order",
                state.reverse,
                "near.collection.sort.toggle-reverse",
            ),
            sort_toggle_item(
                "Numeric names",
                state.numeric,
                "near.collection.sort.toggle-numeric",
            ),
            sort_toggle_item(
                "Selected first",
                state.selected_first,
                "near.collection.sort.toggle-selected-first",
            ),
            sort_toggle_item(
                "Directories first",
                state.directories_first,
                "near.collection.sort.toggle-directories-first",
            ),
            sort_toggle_item(
                "Highlighting sort groups",
                state.sort_groups,
                "near.collection.sort.toggle-groups",
            ),
        ]);
        MenuSurface::new("near-fm.sort-menu", "Sort Modes", items)
    }

    fn panel_mode_menu(&self) -> MenuSurface {
        let active = &self.focused_panel().view_mode().id;
        let items = self
            .settings
            .panel_modes
            .modes()
            .iter()
            .map(|mode| MenuItem {
                label: format!(
                    "{} {}",
                    if &mode.id == active { "√" } else { " " },
                    mode.label
                ),
                description: mode
                    .columns
                    .iter()
                    .map(|column| format!("{:?}", column.kind).to_lowercase())
                    .collect::<Vec<_>>()
                    .join(", "),
                command: CommandInvocation {
                    id: CommandId::from("near.panel.view-mode.set"),
                    arguments: BTreeMap::from([(
                        "id".to_owned(),
                        near_core::CommandValue::String(mode.id.clone()),
                    )]),
                },
                enabled: true,
            })
            .collect();
        MenuSurface::new("near-fm.panel-mode-menu", "Panel View Modes", items)
    }

    fn location_menu(&self, target: FocusedPanel) -> MenuSurface {
        let current = self.panel(target).location();
        let mut items = self
            .providers
            .providers()
            .flat_map(|provider| {
                let provider_id = provider.id();
                provider.locations().into_iter().map(move |location| {
                    let active = &location.location == current;
                    MenuItem {
                        label: format!("{} {}", if active { "√" } else { " " }, location.label),
                        description: format!(
                            "{} • {} • {}",
                            provider_id,
                            location.detail,
                            location.location.as_str()
                        ),
                        command: CommandInvocation {
                            id: CommandId::from("near.provider.navigate"),
                            arguments: BTreeMap::from([
                                (
                                    "target".to_owned(),
                                    near_core::CommandValue::String(panel_name(target).to_owned()),
                                ),
                                (
                                    "provider".to_owned(),
                                    near_core::CommandValue::String(provider_id.to_string()),
                                ),
                                (
                                    "location".to_owned(),
                                    near_core::CommandValue::String(
                                        location.location.as_str().to_owned(),
                                    ),
                                ),
                            ]),
                        },
                        enabled: true,
                    }
                })
            })
            .collect::<Vec<_>>();
        let persisted_count = self
            .temporary_panels
            .values()
            .map(|panel| panel.hits.len())
            .sum::<usize>();
        let populated_slots = self
            .temporary_panels
            .values()
            .filter(|panel| !panel.hits.is_empty())
            .count();
        items.sort_by(|left, right| {
            left.description
                .cmp(&right.description)
                .then_with(|| left.label.cmp(&right.label))
        });
        items.push(MenuItem {
            label: " &Temporary…".to_owned(),
            description: format!(
                "{populated_slots} populated slot(s), {persisted_count} persisted reference(s); source resources never move into this panel"
            ),
            command: CommandInvocation {
                id: CommandId::from("near.temp-panel.list"),
                arguments: BTreeMap::from([(
                    "target".to_owned(),
                    CommandValue::String(panel_name(target).to_owned()),
                )]),
            },
            enabled: true,
        });
        if items.is_empty() {
            items.push(MenuItem {
                label: "No provider locations".to_owned(),
                description: "Registered providers exposed no roots".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.overlay.cancel"),
                    arguments: BTreeMap::new(),
                },
                enabled: false,
            });
        }
        MenuSurface::new(
            format!("near-fm.{}.locations", panel_name(target)),
            format!("{} Panel Locations", capitalize(panel_name(target))),
            items,
        )
    }

    fn selection_menu() -> MenuSurface {
        let items = [
            (
                "Select by mask",
                "Include and exclude wildcard masks",
                "near.selection.select-mask",
            ),
            (
                "Unselect by mask",
                "Remove matching resources from selection",
                "near.selection.unselect-mask",
            ),
            (
                "Select same extension",
                "Select resources matching the current extension",
                "near.selection.same-extension",
            ),
            (
                "Select same name",
                "Select resources matching the current stem",
                "near.selection.same-name",
            ),
            (
                "Invert selection",
                "Toggle all selectable resources",
                "near.selection.invert",
            ),
            (
                "Save selection",
                "Remember selected resource identities",
                "near.selection.save",
            ),
            (
                "Restore selection",
                "Restore the saved resource set",
                "near.selection.restore",
            ),
            (
                "Compare folders",
                "Select unique and changed items across both panels",
                "near.selection.compare-folders",
            ),
        ]
        .into_iter()
        .map(|(label, description, command)| MenuItem {
            label: label.to_owned(),
            description: description.to_owned(),
            command: CommandInvocation {
                id: CommandId::from(command),
                arguments: BTreeMap::default(),
            },
            enabled: true,
        })
        .collect();
        MenuSurface::new("near-fm.selection-menu", "Selection", items)
    }

    fn tree_panel(&self, panel: FocusedPanel) -> TreeSurface {
        let collection = self.panel(panel);
        TreeSurface::new(
            format!("near-fm.{}.tree", panel_name(panel)),
            format!("Tree — {}", collection.location().as_str()),
            vec![TreeNode {
                id: collection.location().as_str().to_owned(),
                label: collection.location().as_str().to_owned(),
                resource: collection
                    .entries()
                    .iter()
                    .find(|entry| !is_parent_entry(entry))
                    .map(|entry| ResourceRef {
                        provider: entry.resource.provider.clone(),
                        location: collection.location().clone(),
                    }),
                expanded: true,
                children: collection
                    .entries()
                    .iter()
                    .filter(|entry| {
                        !is_parent_entry(entry)
                            && matches!(
                                entry.metadata.kind,
                                ResourceKind::Directory | ResourceKind::Package
                            )
                    })
                    .map(|entry| TreeNode {
                        id: entry.resource.to_string(),
                        label: entry.metadata.name.clone(),
                        resource: Some(entry.resource.clone()),
                        expanded: false,
                        children: Vec::new(),
                    })
                    .collect(),
            }],
        )
        .with_indent_width(self.settings.interface.tree_indent_width)
    }

    fn information_panel(&self, panel: FocusedPanel) -> InspectorSurface {
        let collection = self.panel(panel);
        let current = collection.current();
        let selected = collection
            .entries()
            .iter()
            .filter(|entry| entry.selected && !is_parent_entry(entry))
            .count();
        let known_size = collection
            .entries()
            .iter()
            .filter(|entry| !is_parent_entry(entry))
            .filter_map(|entry| entry.metadata.size)
            .fold(0_u64, u64::saturating_add);
        InspectorSurface::new(
            format!("near-fm.{}.information", panel_name(panel)),
            format!("Information — {}", collection.location().as_str()),
            current.map(|entry| entry.resource.clone()),
            vec![
                InspectorField {
                    label: "Location".to_owned(),
                    value: collection.location().as_str().to_owned(),
                    warning: false,
                },
                InspectorField {
                    label: "Provider".to_owned(),
                    value: current
                        .or_else(|| collection.entries().first())
                        .map_or("-", |entry| entry.resource.provider.as_str())
                        .to_owned(),
                    warning: false,
                },
                InspectorField {
                    label: "Entries".to_owned(),
                    value: collection
                        .entries()
                        .iter()
                        .filter(|entry| !is_parent_entry(entry))
                        .count()
                        .to_string(),
                    warning: false,
                },
                InspectorField {
                    label: "Selected".to_owned(),
                    value: selected.to_string(),
                    warning: false,
                },
                InspectorField {
                    label: "Known size".to_owned(),
                    value: format!("{known_size} B"),
                    warning: false,
                },
                InspectorField {
                    label: "Current".to_owned(),
                    value: current
                        .map_or("-", |entry| entry.metadata.name.as_str())
                        .to_owned(),
                    warning: false,
                },
                InspectorField {
                    label: "View mode".to_owned(),
                    value: collection.view_mode().label.clone(),
                    warning: false,
                },
            ],
        )
    }

    fn task_surface(&self) -> TaskSurface {
        let mut tasks = self.task_records.values().cloned().collect::<Vec<_>>();
        if tasks.is_empty() {
            tasks.push(TaskRecord {
                id: "none".to_owned(),
                title: "No tasks".to_owned(),
                state: TaskState::Completed,
                completed: 0,
                total: Some(0),
                message: "Background task history is empty".to_owned(),
            });
        }
        TaskSurface::new("near-fm.tasks", tasks)
    }

    fn track_visible_task(&mut self, task: &TaskHandle, name: &str, record: TaskRecord) {
        self.track_task(task, name);
        self.task_records.insert(task.id().0, record);
    }

    fn demo_terminal() -> TerminalSurface {
        let mut terminal = TerminalSurface::new("near-fm.terminal", "shell surface", 1_000);
        terminal.append_output(
            "Near terminal surface model\n$ pwd\n/Users/alex/Projects/Near\n$ echo PTY hosting arrives in M3",
        );
        terminal.set_cursor(Some((2, 4)));
        terminal
    }

    fn command_palette_entries(&self, keymap: Option<&Keymap>) -> Vec<PaletteEntry> {
        let contexts = [ContextId::from("workspace.panel")];
        let mut entries: Vec<_> = self
            .registry
            .descriptors()
            .filter(|descriptor| descriptor.invokable_without_arguments())
            .map(|descriptor| {
                let shortcut = keymap.map_or_else(String::new, |keymap| {
                    keymap
                        .bindings_for_command(&contexts, &descriptor.id)
                        .first()
                        .map_or_else(String::new, |binding| {
                            format_key_sequence(&binding.sequence)
                        })
                });
                PaletteEntry {
                    id: descriptor.id.clone(),
                    title: descriptor.title.clone(),
                    shortcut,
                }
            })
            .collect();
        entries.sort_by(|left, right| {
            match (left.shortcut.is_empty(), right.shortcut.is_empty()) {
                (false, false) => left
                    .shortcut
                    .cmp(&right.shortcut)
                    .then_with(|| left.title.cmp(&right.title)),
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                (true, true) => left.title.cmp(&right.title),
            }
        });
        entries
    }
}

fn parent_collection_entry(
    provider: &Arc<dyn ResourceProvider>,
    location: &Location,
) -> Option<CollectionEntry> {
    let parent = provider.parent(location)?;
    let mut metadata = ResourceMetadata {
        name: "..".to_owned(),
        kind: ResourceKind::Directory,
        ..ResourceMetadata::default()
    };
    metadata.extensions.insert(
        PARENT_ENTRY_EXTENSION.to_owned(),
        MetadataValue::Boolean(true),
    );
    Some(
        CollectionEntry::new(
            ResourceRef {
                provider: provider.id(),
                location: parent,
            },
            metadata,
            "parent",
        )
        .with_selection_denial("The parent entry cannot be selected")
        .with_sort_priority(i64::MIN),
    )
}

fn is_parent_entry(entry: &CollectionEntry) -> bool {
    entry.metadata.name == ".."
        || matches!(
            entry.metadata.extensions.get(PARENT_ENTRY_EXTENSION),
            Some(MetadataValue::Boolean(true))
        )
}

fn sort_toggle_item(label: &str, active: bool, command: &str) -> MenuItem {
    MenuItem {
        label: format!("{} {label}", if active { "√" } else { " " }),
        description: format!("Toggle {}", label.to_lowercase()),
        command: CommandInvocation {
            id: CommandId::from(command),
            arguments: BTreeMap::default(),
        },
        enabled: true,
    }
}

fn far_panel_mode_label(id: &str, configured_label: &str) -> String {
    match id {
        "compact" => "&Brief".to_owned(),
        "medium" => "&Medium".to_owned(),
        "full" => "&Full".to_owned(),
        "wide" => "&Wide".to_owned(),
        "metadata" => "&Detailed".to_owned(),
        _ => configured_label.to_owned(),
    }
}

fn macro_id(invocation: &CommandInvocation) -> Option<&str> {
    invocation
        .arguments
        .get("id")
        .and_then(CommandValue::as_str)
}

fn macro_invocation(command: &str, id: &str) -> CommandInvocation {
    CommandInvocation {
        id: CommandId::from(command),
        arguments: BTreeMap::from([("id".to_owned(), CommandValue::String(id.to_owned()))]),
    }
}

fn macro_dialog_field(id: &str, label: &str, value: &str, required: bool) -> DialogField {
    DialogField {
        id: id.to_owned(),
        label: label.to_owned(),
        value: value.to_owned(),
        required,
        secret: false,
    }
}

fn presence_label(condition: PresenceCondition) -> &'static str {
    match condition {
        PresenceCondition::Any => "any",
        PresenceCondition::Present => "present",
        PresenceCondition::Absent => "absent",
    }
}

fn parse_presence(value: &str) -> Result<PresenceCondition, String> {
    match value.to_ascii_lowercase().as_str() {
        "any" => Ok(PresenceCondition::Any),
        "present" | "yes" => Ok(PresenceCondition::Present),
        "absent" | "no" => Ok(PresenceCondition::Absent),
        _ => Err("must be any, present, or absent".to_owned()),
    }
}

fn macro_condition_summary(condition: &MacroCondition) -> String {
    let contexts = if condition.required_contexts.is_empty() {
        "any context".to_owned()
    } else {
        condition
            .required_contexts
            .iter()
            .map(ContextId::as_str)
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "{contexts}; current={}; peer={}",
        presence_label(condition.current_resource),
        presence_label(condition.peer_surface)
    )
}

struct StaticCommand(CommandDescriptor);

struct WorkspaceMacroHost<'a> {
    workspace: &'a mut FarWorkspace,
}

impl MacroHost for WorkspaceMacroHost<'_> {
    fn macro_context(&self) -> MacroContext {
        MacroContext {
            contexts: self.workspace.active_contexts(),
            action: self.workspace.action_context(),
        }
    }

    fn validate_macro_command(
        &self,
        invocation: &CommandInvocation,
    ) -> Result<SafetyClass, String> {
        self.workspace
            .registry
            .check(invocation, &self.workspace.action_context())
            .map(|command| command.descriptor().safety)
            .map_err(|error| error.to_string())
    }

    fn invoke_macro_command(&mut self, invocation: &CommandInvocation) -> Result<(), String> {
        self.workspace.dispatch_with_keymap(invocation, None);
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct PaletteEntry {
    id: CommandId,
    title: String,
    shortcut: String,
}

fn palette_visible_indices(entries: &[PaletteEntry], search: &SelectionSearch) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            search.matches([
                entry.title.as_str(),
                entry.shortcut.as_str(),
                entry.id.as_str(),
            ])
        })
        .map(|(index, _)| index)
        .collect()
}

impl Command for StaticCommand {
    fn descriptor(&self) -> &CommandDescriptor {
        &self.0
    }

    fn availability(&self, context: &ActionContext) -> Availability {
        match self.0.id.as_str() {
            "near.resource.open"
            | "near.resource.view"
            | "near.resource.edit"
            | "near.resource.edit-external"
            | "near.resource.execute-external"
            | "near.resource.associations"
            | "near.resource.association-run"
            | "near.resource.description"
            | "near.resource.description-confirmed"
            | "near.resource.rename"
            | "near.resource.link"
            | "near.resource.attributes"
            | "near.resource.trash"
            | "near.resource.trash-current"
            | "near.resource.delete"
            | "near.resource.wipe"
            | "near.resource.wipe-confirmed"
                if context.current.is_none() =>
            {
                Availability::Unavailable {
                    reason: "no current resource".to_owned(),
                }
            }
            "near.resource.copy-to-peer"
            | "near.resource.copy-current-to-peer"
            | "near.resource.move-to-peer"
            | "near.resource.move-current-to-peer"
            | "near.archive.create"
                if context.current.is_none() && context.selected.is_empty() =>
            {
                Availability::Unavailable {
                    reason: "no source resource".to_owned(),
                }
            }
            "near.resource.copy-to-peer"
            | "near.resource.copy-current-to-peer"
            | "near.resource.move-to-peer"
            | "near.resource.move-current-to-peer"
                if context.peer_location.is_none() =>
            {
                Availability::Unavailable {
                    reason: "no peer destination".to_owned(),
                }
            }
            "near.resource.copy-to-peer" | "near.resource.copy-current-to-peer"
                if context
                    .peer_location
                    .as_ref()
                    .is_some_and(|location| location.as_str().starts_with("archive://"))
                    && !context
                        .peer_capabilities
                        .contains(&CapabilityId::from("archive.update")) =>
            {
                Availability::Unavailable {
                    reason: "peer archive format is read-only".to_owned(),
                }
            }
            "near.archive.create"
                if !context
                    .capabilities
                    .contains(&CapabilityId::from("archive.create")) =>
            {
                Availability::Unavailable {
                    reason: "current provider has no writable archive format".to_owned(),
                }
            }
            "near.fs.create-directory" | "near.location.parent" | "near.search.start"
                if context.location.is_none() =>
            {
                Availability::Unavailable {
                    reason: "surface has no location".to_owned(),
                }
            }
            "near.operation.apply-command"
                if context.current.is_none() && context.selected.is_empty() =>
            {
                Availability::Unavailable {
                    reason: "no source resource".to_owned(),
                }
            }
            "near.device.disconnect"
                if !context
                    .capabilities
                    .contains(&CapabilityId::from("device.disconnect")) =>
            {
                Availability::Unavailable {
                    reason: "current resource is not a disconnectable device".to_owned(),
                }
            }
            _ => Availability::Available,
        }
    }
}

fn command_arguments(id: &str) -> BTreeMap<String, ArgumentSchema> {
    let entries = match id {
        "near.collection.move" | "near.collection.toggle-selection-move" => {
            vec![("rows", ArgumentKind::Integer, true)]
        }
        "near.collection.page" => vec![("pages", ArgumentKind::Integer, true)],
        "near.collection.scroll-horizontal" => {
            vec![("columns", ArgumentKind::Integer, true)]
        }
        "near.workspace.resize-panels" => vec![
            ("columns", ArgumentKind::Integer, false),
            ("rows", ArgumentKind::Integer, false),
        ],
        "near.provider.choose" => vec![("target", ArgumentKind::String, false)],
        "near.temp-panel.open" => vec![
            ("slot", ArgumentKind::Integer, false),
            ("target", ArgumentKind::String, false),
        ],
        "near.temp-panel.list" => vec![("target", ArgumentKind::String, false)],
        "near.temp-panel.import-confirmed" => vec![
            ("path", ArgumentKind::String, true),
            ("mode", ArgumentKind::String, true),
            ("any", ArgumentKind::Boolean, false),
        ],
        "near.temp-panel.export-confirmed" => {
            vec![("path", ArgumentKind::String, true)]
        }
        "near.temp-panel.menu-select" => vec![("text", ArgumentKind::String, true)],
        "near.provider.navigate" => vec![
            ("target", ArgumentKind::String, true),
            ("provider", ArgumentKind::String, true),
            ("location", ArgumentKind::String, true),
        ],
        "near.fs.create-directory.confirmed" | "near.archive.create-confirmed" => {
            vec![("name", ArgumentKind::String, true)]
        }
        "near.resource.rename-confirmed" => vec![
            ("template", ArgumentKind::String, true),
            ("start", ArgumentKind::String, true),
        ],
        "near.resource.link-confirmed" => vec![
            ("name", ArgumentKind::String, true),
            ("type", ArgumentKind::String, true),
        ],
        "near.resource.attributes-confirmed" => vec![
            ("readonly", ArgumentKind::String, true),
            ("unix_mode", ArgumentKind::String, false),
            ("owner", ArgumentKind::String, false),
            ("group", ArgumentKind::String, false),
            ("modified", ArgumentKind::String, false),
            ("accessed", ArgumentKind::String, false),
            ("recursive", ArgumentKind::String, true),
        ],
        "near.resource.wipe-confirmed" => vec![("passes", ArgumentKind::String, true)],
        "near.resource.association-run" => vec![
            ("action", ArgumentKind::String, true),
            ("handler", ArgumentKind::String, true),
        ],
        "near.resource.description-confirmed" => {
            vec![("description", ArgumentKind::String, false)]
        }
        "near.user-menu.run" => vec![
            ("scope", ArgumentKind::String, true),
            ("entry", ArgumentKind::String, true),
        ],
        "near.filters.toggle" => vec![("filter", ArgumentKind::String, true)],
        "near.command-line.history-use" | "near.command-line.history-toggle-lock" => {
            vec![("command", ArgumentKind::String, true)]
        }
        "near.resource-history.open-selected" | "near.resource-history.toggle-lock-selected" => {
            vec![
                ("kind", ArgumentKind::String, true),
                ("provider", ArgumentKind::String, true),
                ("location", ArgumentKind::String, true),
                ("label", ArgumentKind::String, true),
            ]
        }
        "near.resource-history.clear" => vec![("kind", ArgumentKind::String, true)],
        "near.location.history-open" | "near.location.history-toggle-lock" => vec![
            ("provider", ArgumentKind::String, true),
            ("location", ArgumentKind::String, true),
        ],
        "near.location.shortcut-assign" | "near.location.shortcut-open" => {
            vec![("slot", ArgumentKind::Integer, true)]
        }
        "near.panel.view-mode.set" => vec![("id", ArgumentKind::String, true)],
        "near.theme.preview" => vec![("name", ArgumentKind::String, true)],
        "near.theme.edit" => vec![("role", ArgumentKind::String, true)],
        "near.theme.edit-confirmed" => vec![
            ("role", ArgumentKind::String, true),
            ("foreground", ArgumentKind::String, false),
            ("background", ArgumentKind::String, false),
        ],
        "near.settings.show" => vec![("category", ArgumentKind::String, false)],
        "near.settings.apply-candidate" => vec![
            ("id", ArgumentKind::String, true),
            ("value", ArgumentKind::String, true),
        ],
        "near.settings.edit-value" => vec![
            ("id", ArgumentKind::String, true),
            ("value", ArgumentKind::String, false),
        ],
        "near.selection.mask-confirmed" => vec![
            ("include", ArgumentKind::String, true),
            ("exclude", ArgumentKind::String, false),
            ("selected", ArgumentKind::Boolean, true),
        ],
        "near.selection.compare-folders-confirmed" => vec![
            ("compare_size", ArgumentKind::String, true),
            ("compare_modified", ArgumentKind::String, true),
            ("tolerance_seconds", ArgumentKind::String, true),
            ("case_sensitive", ArgumentKind::String, true),
            ("selection", ArgumentKind::String, true),
        ],
        "near.operation.apply-command-confirmed" => vec![
            ("template", ArgumentKind::String, true),
            ("mode", ArgumentKind::String, true),
            ("continue_on_error", ArgumentKind::String, true),
        ],
        "near.editor.save-as-confirmed" => vec![
            ("provider", ArgumentKind::String, true),
            ("location", ArgumentKind::String, true),
            ("encoding", ArgumentKind::String, true),
            ("bom", ArgumentKind::String, true),
            ("eol", ArgumentKind::String, true),
            ("replace", ArgumentKind::String, true),
            ("lossy", ArgumentKind::String, true),
        ],
        "near.search.confirmed" => search_argument_entries(),
        "near.search.open-panel" => vec![("session", ArgumentKind::Integer, true)],
        "near.macro.actions"
        | "near.macro.play"
        | "near.macro.edit"
        | "near.macro.bind"
        | "near.macro.delete"
        | "near.macro.diagnose" => vec![("id", ArgumentKind::String, true)],
        "near.macro.edit-confirmed" => vec![
            ("id", ArgumentKind::String, true),
            ("title", ArgumentKind::String, true),
            ("trust", ArgumentKind::String, true),
            ("contexts", ArgumentKind::String, false),
            ("capabilities", ArgumentKind::String, false),
            ("current", ArgumentKind::String, true),
            ("peer", ArgumentKind::String, true),
        ],
        "near.macro.bind-confirmed" => vec![
            ("id", ArgumentKind::String, true),
            ("binding", ArgumentKind::String, false),
        ],
        "near.macro.delete-confirmed" => vec![
            ("id", ArgumentKind::String, true),
            ("confirm", ArgumentKind::String, true),
        ],
        "near.operation.confirmed" => vec![
            ("plan", ArgumentKind::String, true),
            ("conflict", ArgumentKind::String, true),
            ("high_impact_confirmed", ArgumentKind::Boolean, false),
        ],
        "near.viewer.bookmark-set" | "near.viewer.bookmark-jump" => {
            vec![("slot", ArgumentKind::Integer, true)]
        }
        "near.screen.editor" => vec![("index", ArgumentKind::Integer, true)],
        "near.screen.terminal" => vec![("tab", ArgumentKind::Integer, false)],
        "near.terminal.select" => vec![("tab", ArgumentKind::Integer, true)],
        "near.task.cancel" | "near.task.retry" => vec![("task", ArgumentKind::String, true)],
        "near.terminal.input" => vec![("text", ArgumentKind::String, true)],
        "near.terminal.send-key" => vec![("key", ArgumentKind::String, true)],
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .map(|(name, kind, required)| {
            (
                name.to_owned(),
                ArgumentSchema {
                    kind,
                    required,
                    description: format!("{name} argument"),
                    default: None,
                },
            )
        })
        .collect()
}

fn extension_setting_field(setting: ExtensionSetting) -> DialogField {
    DialogField {
        id: setting.id,
        label: setting.label,
        value: setting.value,
        required: setting.required,
        secret: setting.secret,
    }
}

fn editable_color(color: Option<crate::SemanticColor>) -> String {
    color
        .map(format_semantic_color)
        .unwrap_or_else(|| "inherit".to_owned())
}

fn parse_setting_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true or false, received {value}")),
    }
}
fn parse_positive_usize(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| error.to_string())
        .and_then(|value| {
            (value > 0)
                .then_some(value)
                .ok_or_else(|| "value must be greater than zero".to_owned())
        })
}

const fn open_policy_name(policy: ResourceOpenPolicy) -> &'static str {
    match policy {
        ResourceOpenPolicy::Internal => "internal",
        ResourceOpenPolicy::External => "external",
        ResourceOpenPolicy::Association => "association",
    }
}

fn parse_open_policy(value: &str) -> Result<ResourceOpenPolicy, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "internal" => Ok(ResourceOpenPolicy::Internal),
        "external" => Ok(ResourceOpenPolicy::External),
        "association" => Ok(ResourceOpenPolicy::Association),
        _ => Err("open policy must be internal, external, or association".to_owned()),
    }
}

fn generated_placeholder_metadata(resource: &ResourceRef) -> ResourceMetadata {
    let name = resource
        .location
        .as_str()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(resource.location.as_str())
        .to_owned();
    let mut metadata = ResourceMetadata {
        name,
        kind: ResourceKind::Other,
        ..ResourceMetadata::default()
    };
    metadata.extensions.insert(
        "near.generated.pending-refresh".to_owned(),
        MetadataValue::Boolean(true),
    );
    metadata
}

fn search_argument_entries() -> Vec<(&'static str, ArgumentKind, bool)> {
    [
        ("scope", false),
        ("archives", false),
        ("symlinks", false),
        ("streams", false),
        ("name", true),
        ("name_mode", false),
        ("content", false),
        ("content_mode", false),
        ("encoding", false),
        ("case_sensitive", false),
        ("kinds", false),
        ("minimum_size", false),
        ("maximum_size", false),
        ("modified_after", false),
        ("modified_before", false),
        ("readonly", false),
        ("executable", false),
        ("hidden", false),
        ("ignore", false),
        ("mode", false),
    ]
    .into_iter()
    .map(|(name, required)| (name, ArgumentKind::String, required))
    .collect()
}

fn expand_rename_template(template: &str, name: &str, index: u64) -> String {
    let (stem, extension) = split_rename_name(name);
    let dot_extension = if extension.is_empty() {
        String::new()
    } else {
        format!(".{extension}")
    };
    template
        .replace("{name}", name)
        .replace("{stem}", stem)
        .replace("{dotext}", &dot_extension)
        .replace("{ext}", extension)
        .replace("{index}", &index.to_string())
}

fn split_rename_name(name: &str) -> (&str, &str) {
    name.rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && !extension.is_empty())
        .unwrap_or((name, ""))
}

fn validate_rename_target(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("target name is empty");
    }
    if matches!(name, "." | "..") {
        return Err("target name is reserved");
    }
    if name.contains(['/', '\\', '\0']) {
        return Err("target name contains a path separator or NUL");
    }
    Ok(())
}

fn external_action_name(action: ExternalAction) -> &'static str {
    match action {
        ExternalAction::View => "view",
        ExternalAction::Edit => "edit",
        ExternalAction::Execute => "execute",
        ExternalAction::Open => "open",
        ExternalAction::Inspect => "inspect",
        ExternalAction::Shell => "shell",
        _ => "unknown",
    }
}

fn external_action_label(action: ExternalAction) -> &'static str {
    match action {
        ExternalAction::View => "View",
        ExternalAction::Edit => "Edit",
        ExternalAction::Execute => "Execute",
        ExternalAction::Open => "Open",
        ExternalAction::Inspect => "Inspect",
        ExternalAction::Shell => "Shell",
        _ => "External",
    }
}

fn parse_external_action(value: &str) -> Option<ExternalAction> {
    match value {
        "view" => Some(ExternalAction::View),
        "edit" => Some(ExternalAction::Edit),
        "execute" => Some(ExternalAction::Execute),
        "open" => Some(ExternalAction::Open),
        "inspect" => Some(ExternalAction::Inspect),
        "shell" => Some(ExternalAction::Shell),
        _ => None,
    }
}

fn external_mode_label(mode: near_core::ExternalInvocationMode) -> &'static str {
    match mode {
        near_core::ExternalInvocationMode::StructuredArgv => "structured argv",
        near_core::ExternalInvocationMode::ExplicitShell => "EXPLICIT SHELL",
        _ => "external mode",
    }
}

fn main_menu_command(category: MainMenuCategory) -> &'static str {
    match category {
        MainMenuCategory::Left => "near.menu.left",
        MainMenuCategory::Files => "near.menu.files",
        MainMenuCategory::Commands => "near.menu.commands",
        MainMenuCategory::Options => "near.menu.options",
        MainMenuCategory::Right => "near.menu.right",
    }
}

fn is_main_menu_command(command: &CommandId) -> bool {
    MainMenuCategory::ALL
        .into_iter()
        .any(|category| command.as_str() == main_menu_command(category))
}

fn menu_popup(area: Rect, menu: &MenuSurface) -> Rect {
    if menu.is_main_menu() {
        let (width, height) = menu
            .main_menu_popup_size(area.width.saturating_sub(1), area.height.saturating_sub(1))
            .unwrap_or((1, 1));
        let desired_x = menu.main_menu_column(area.x).unwrap_or(area.x);
        let maximum_x = area.right().saturating_sub(width);
        Rect::new(
            desired_x.min(maximum_x),
            area.y.saturating_add(1),
            width,
            height,
        )
    } else {
        centered(area, 76, 20)
    }
}

fn centered(area: Rect, desired_width: u16, desired_height: u16) -> Rect {
    let width = desired_width.min(area.width.saturating_sub(2)).max(1);
    let height = desired_height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn panel_item_at(
    layout: DualSurfaceLayout,
    columns: u16,
    panel_height: u16,
    column: u16,
    row: u16,
) -> Option<(FocusedPanel, Option<usize>)> {
    let geometry = layout.geometry(columns, panel_height, 8, 3);
    let panel_height = geometry.pane_height;
    if column >= columns || row >= panel_height {
        return None;
    }
    let panel = match layout.side_at(column, columns, 8)? {
        DualSurfaceSide::First => FocusedPanel::Left,
        DualSurfaceSide::Second => FocusedPanel::Right,
    };
    let item = (row > 0 && row < panel_height.saturating_sub(1))
        .then(|| usize::from(row.saturating_sub(1)));
    Some((panel, item))
}

fn full_screen_panel_item_at(
    panel: FocusedPanel,
    columns: u16,
    panel_height: u16,
    column: u16,
    row: u16,
) -> Option<(FocusedPanel, Option<usize>)> {
    if column >= columns || row >= panel_height {
        return None;
    }
    Some((
        panel,
        (row > 0 && row < panel_height.saturating_sub(1))
            .then(|| usize::from(row.saturating_sub(1))),
    ))
}

fn inset(area: Rect, amount: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(amount),
        area.y.saturating_add(amount),
        area.width.saturating_sub(amount.saturating_mul(2)),
        area.height.saturating_sub(amount.saturating_mul(2)),
    )
}

fn border_type(theme: &SemanticTheme) -> BorderType {
    match theme.glyphs().border.as_str() {
        "double" => BorderType::Double,
        "thick" => BorderType::Thick,
        "rounded" => BorderType::Rounded,
        _ => BorderType::Plain,
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut result: String = value.chars().take(width - 1).collect();
    result.push('…');
    result
}

fn short_description(binding: &KeyBinding) -> &str {
    binding.description.as_deref().unwrap_or("Command")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-/".contains(character))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_conflict_action(value: &str) -> Option<ConflictAction> {
    match value {
        "replace" => Some(ConflictAction::Replace),
        "skip" => Some(ConflictAction::Skip),
        "rename" => Some(ConflictAction::Rename),
        "cancel" => Some(ConflictAction::Cancel),
        _ => None,
    }
}

fn folder_comparison_policy(
    invocation: &CommandInvocation,
) -> Result<FolderComparisonPolicy, String> {
    let value = |name: &str| {
        invocation
            .arguments
            .get(name)
            .and_then(near_core::CommandValue::as_str)
            .unwrap_or_default()
            .trim()
    };
    let compare_size = parse_yes_no(value("compare_size"), "Compare size")?;
    let compare_modified = parse_yes_no(value("compare_modified"), "Compare time")?;
    let case_sensitive_names = parse_yes_no(value("case_sensitive"), "Case sensitive")?;
    let tolerance_seconds = value("tolerance_seconds")
        .parse::<u64>()
        .map_err(|_| "Time tolerance must be a non-negative whole number of seconds".to_owned())?;
    let selection = match value("selection").to_ascii_lowercase().as_str() {
        "newer" | "newer-or-unique" => ComparisonSelection::NewerOrUnique,
        "both" | "both-differing" => ComparisonSelection::BothDiffering,
        _ => return Err("Select must be 'newer' or 'both'".to_owned()),
    };
    Ok(FolderComparisonPolicy {
        case_sensitive_names,
        compare_size,
        compare_modified,
        timestamp_tolerance_ms: tolerance_seconds.saturating_mul(1_000),
        selection,
    })
}

fn parse_yes_no(value: &str, label: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "yes" | "true" | "on" | "1" => Ok(true),
        "no" | "false" | "off" | "0" => Ok(false),
        _ => Err(format!("{label} must be yes or no")),
    }
}

fn search_options(invocation: &CommandInvocation) -> Result<SearchOptions, String> {
    let scope = SearchScope::parse(argument_text(invocation, "scope"))
        .ok_or_else(|| "Scope must be current, selected, providers, or archives".to_owned())?;
    let archives = match argument_text(invocation, "archives") {
        "" | "exclude" | "no" => ArchiveSearchPolicy::Exclude,
        "include" | "yes" => ArchiveSearchPolicy::Include,
        _ => return Err("Nested archives must be exclude or include".to_owned()),
    };
    let symlinks = match argument_text(invocation, "symlinks") {
        "skip" => SymlinkSearchPolicy::Skip,
        "" | "match" => SymlinkSearchPolicy::Match,
        "follow" => SymlinkSearchPolicy::Follow,
        _ => return Err("Symbolic links must be skip, match, or follow".to_owned()),
    };
    let streams = match argument_text(invocation, "streams") {
        "" | "exclude" | "no" => AlternateStreamSearchPolicy::Exclude,
        "include" | "yes" => AlternateStreamSearchPolicy::Include,
        _ => return Err("Alternate streams must be exclude or include".to_owned()),
    };
    Ok(SearchOptions {
        scope,
        archives,
        symlinks,
        streams,
    })
}

fn search_root_label(request: &ScopedSearchRequest) -> String {
    let mut labels = request
        .roots
        .iter()
        .map(|root| format!("{}:{}", root.provider, root.location.as_str()))
        .collect::<Vec<_>>();
    labels.sort();
    if labels.len() > 2 {
        format!("{} roots", labels.len())
    } else {
        labels.join(", ")
    }
}

fn search_predicate(invocation: &CommandInvocation) -> Result<ResourcePredicate, String> {
    let name = argument_text(invocation, "name");
    let name = if name.is_empty() || name == "*" {
        None
    } else {
        Some(match argument_text(invocation, "name_mode") {
            "" | "glob" | "mask" => TextPredicate::Glob(name.to_owned()),
            "regex" => TextPredicate::Regex(name.to_owned()),
            "contains" => TextPredicate::Contains(name.to_owned()),
            "exact" => TextPredicate::Exact(name.to_owned()),
            _ => return Err("Name mode must be glob, regex, contains, or exact".to_owned()),
        })
    };
    let content_text = argument_text(invocation, "content");
    let content = if content_text.is_empty() {
        None
    } else {
        let match_kind = match argument_text(invocation, "content_mode") {
            "" | "text" => ContentMatch::Text,
            "regex" => ContentMatch::Regex,
            "hex" => ContentMatch::Hex,
            _ => return Err("Content mode must be text, regex, or hex".to_owned()),
        };
        let encoding = match argument_text(invocation, "encoding") {
            "" | "auto" => SearchEncoding::Auto,
            "utf8" | "utf-8" => SearchEncoding::Utf8,
            "utf16le" | "utf-16le" => SearchEncoding::Utf16Le,
            "utf16be" | "utf-16be" => SearchEncoding::Utf16Be,
            "latin1" | "latin-1" | "iso-8859-1" => SearchEncoding::Latin1,
            _ => return Err("Encoding must be auto, utf8, utf16le, utf16be, or latin1".to_owned()),
        };
        let case_sensitive = match argument_text(invocation, "case_sensitive") {
            "" => false,
            value => parse_yes_no(value, "Content case sensitive")?,
        };
        Some(ContentPredicate {
            text: content_text.to_owned(),
            case_sensitive,
            match_kind,
            encoding,
        })
    };
    let predicate = ResourcePredicate {
        name,
        kinds: parse_search_kinds(argument_text(invocation, "kinds"))?,
        minimum_size: parse_search_size(argument_text(invocation, "minimum_size"), "Minimum size")?,
        maximum_size: parse_search_size(argument_text(invocation, "maximum_size"), "Maximum size")?,
        modified_after_unix_ms: parse_search_date(
            argument_text(invocation, "modified_after"),
            "Modified after",
        )?,
        modified_before_unix_ms: parse_search_date(
            argument_text(invocation, "modified_before"),
            "Modified before",
        )?,
        readonly: parse_search_boolean(argument_text(invocation, "readonly"), "Read only")?,
        executable: parse_search_boolean(argument_text(invocation, "executable"), "Executable")?,
        hidden: match argument_text(invocation, "hidden") {
            "" | "exclude" => HiddenPolicy::Exclude,
            "include" => HiddenPolicy::Include,
            "only" => HiddenPolicy::Only,
            _ => return Err("Hidden must be exclude, include, or only".to_owned()),
        },
        ignore: match argument_text(invocation, "ignore") {
            "" | "none" => IgnorePolicy::None,
            "vcs" | "version-control" => IgnorePolicy::VersionControl,
            "common" => IgnorePolicy::Common,
            _ => return Err("Ignore must be none, vcs, or common".to_owned()),
        },
        content,
        ..ResourcePredicate::default()
    };
    predicate.validate().map_err(|error| error.to_string())?;
    Ok(predicate)
}

fn parse_search_kinds(value: &str) -> Result<Vec<ResourceKind>, String> {
    if value.is_empty() || value.eq_ignore_ascii_case("all") {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "file" | "files" => Ok(ResourceKind::File),
            "directory" | "directories" | "dir" => Ok(ResourceKind::Directory),
            "package" | "packages" | "archive" | "archives" => Ok(ResourceKind::Package),
            "symlink" | "symlinks" | "link" | "links" => Ok(ResourceKind::Symlink),
            "virtual" => Ok(ResourceKind::Virtual),
            "other" => Ok(ResourceKind::Other),
            invalid => Err(format!("Kinds contains unsupported value `{invalid}`")),
        })
        .collect()
}

fn parse_search_size(value: &str, label: &str) -> Result<Option<u64>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.len());
    let number = value[..split]
        .parse::<u64>()
        .map_err(|_| format!("{label} must start with a non-negative whole number"))?;
    let multiplier = match value[split..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024_u64.pow(2),
        "g" | "gb" | "gib" => 1024_u64.pow(3),
        _ => return Err(format!("{label} unit must be B, K, M, or G")),
    };
    number
        .checked_mul(multiplier)
        .map(Some)
        .ok_or_else(|| format!("{label} is too large"))
}

fn parse_search_date(value: &str, label: &str) -> Result<Option<i64>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    if let Ok(timestamp) = value.parse::<i64>() {
        return Ok(Some(timestamp));
    }
    let parts = value
        .split('-')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("{label} must be YYYY-MM-DD or Unix milliseconds"))?;
    let [year, month, day] = parts.as_slice() else {
        return Err(format!("{label} must be YYYY-MM-DD or Unix milliseconds"));
    };
    let leap_year =
        year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0);
    let days_in_month = match month {
        2 if leap_year => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    if *day < 1 || *day > days_in_month {
        return Err(format!("{label} contains an invalid calendar date"));
    }
    let adjusted_year = year - i64::from(*month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if *month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days.checked_mul(86_400_000)
        .map(Some)
        .ok_or_else(|| format!("{label} is outside the supported range"))
}

fn parse_search_boolean(value: &str, label: &str) -> Result<Option<bool>, String> {
    match value.to_ascii_lowercase().as_str() {
        "" | "any" => Ok(None),
        "yes" | "true" | "on" | "1" => Ok(Some(true)),
        "no" | "false" | "off" | "0" => Ok(Some(false)),
        _ => Err(format!("{label} must be any, yes, or no")),
    }
}

fn argument_text<'a>(invocation: &'a CommandInvocation, name: &str) -> &'a str {
    invocation
        .arguments
        .get(name)
        .and_then(near_core::CommandValue::as_str)
        .unwrap_or_default()
        .trim()
}

fn parse_keep_boolean(value: &str) -> Result<Option<bool>, String> {
    match value.to_ascii_lowercase().as_str() {
        "" | "keep" | "unchanged" => Ok(None),
        "yes" | "true" | "on" | "1" => Ok(Some(true)),
        "no" | "false" | "off" | "0" => Ok(Some(false)),
        _ => Err("Read only must be keep, yes, or no".to_owned()),
    }
}

fn parse_optional_octal(value: &str) -> Result<Option<u32>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    let mode = u32::from_str_radix(value.trim_start_matches("0o"), 8)
        .map_err(|_| "Unix mode must be an octal value such as 755 or 0644".to_owned())?;
    if mode > 0o7777 {
        return Err("Unix mode must be between 0000 and 7777".to_owned());
    }
    Ok(Some(mode))
}

fn parse_optional_u32(value: &str, label: &str) -> Result<Option<u32>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        value
            .parse::<u32>()
            .map(Some)
            .map_err(|_| format!("{label} must be a non-negative integer"))
    }
}

fn parse_optional_timestamp(value: &str) -> Result<Option<i64>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    if value.eq_ignore_ascii_case("now") {
        let milliseconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis();
        return i64::try_from(milliseconds)
            .map(Some)
            .map_err(|_| "current time is outside the supported range".to_owned());
    }
    value
        .parse::<i64>()
        .map(Some)
        .map_err(|_| "use Unix milliseconds, 'now', or blank".to_owned())
}

fn command_result_failed(result: &Result<CommandLineOutput, String>) -> bool {
    match result {
        Ok(output) => output.exit_code != Some(0),
        Err(_) => true,
    }
}

fn command_slot(invocation: &CommandInvocation) -> Option<usize> {
    invocation
        .arguments
        .get("slot")
        .and_then(near_core::CommandValue::as_i64)
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| *slot < 10)
}

const fn opposite_panel(panel: FocusedPanel) -> FocusedPanel {
    match panel {
        FocusedPanel::Left => FocusedPanel::Right,
        FocusedPanel::Right => FocusedPanel::Left,
    }
}

const fn panel_name(panel: FocusedPanel) -> &'static str {
    match panel {
        FocusedPanel::Left => "left",
        FocusedPanel::Right => "right",
    }
}

fn directory_quick_view_summary(location: &Location, page: &near_core::ListPage) -> String {
    let mut directories = 0_usize;
    let mut files = 0_usize;
    let mut links = 0_usize;
    let mut packages = 0_usize;
    let mut known_size = 0_u64;
    let mut metadata_errors = 0_usize;
    for entry in &page.entries {
        match entry.metadata.kind {
            ResourceKind::Directory => directories = directories.saturating_add(1),
            ResourceKind::Package => packages = packages.saturating_add(1),
            ResourceKind::Symlink => links = links.saturating_add(1),
            _ => files = files.saturating_add(1),
        }
        known_size = known_size.saturating_add(entry.metadata.size.unwrap_or(0));
        metadata_errors = metadata_errors.saturating_add(entry.metadata.field_errors.len());
    }
    let mut summary = format!(
        "Directory summary\nLocation: {}\nVisible items: {}{}\nDirectories: {directories}\nPackages: {packages}\nFiles: {files}\nLinks: {links}\nKnown file size: {known_size} bytes\nMetadata errors: {metadata_errors}\n\nContents:\n",
        location.as_str(),
        page.entries.len(),
        if page.complete { "" } else { "+" },
    );
    for entry in &page.entries {
        let marker = match entry.metadata.kind {
            ResourceKind::Directory => "/",
            ResourceKind::Package => "#",
            ResourceKind::Symlink => "@",
            _ => " ",
        };
        summary.push_str(&format!("{marker} {}\n", entry.metadata.name));
    }
    if !page.complete {
        summary.push_str("… additional entries are available\n");
    }
    summary
}

const fn panel_type_label(panel_type: PanelType) -> &'static str {
    match panel_type {
        PanelType::File => "file",
        PanelType::Tree => "tree",
        PanelType::Information => "information",
        PanelType::QuickView => "quick view",
    }
}

fn provider_target(invocation: &CommandInvocation, focused: FocusedPanel) -> FocusedPanel {
    match invocation
        .arguments
        .get("target")
        .and_then(near_core::CommandValue::as_str)
        .unwrap_or("focused")
    {
        "left" => FocusedPanel::Left,
        "right" => FocusedPanel::Right,
        "peer" => opposite_panel(focused),
        _ => focused,
    }
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

fn resource_description(metadata: &ResourceMetadata) -> Option<String> {
    metadata
        .extensions
        .get(near_core::RESOURCE_DESCRIPTION_KEY)
        .and_then(|value| match value {
            MetadataValue::String(value) => Some(value.clone()),
            _ => None,
        })
}

fn editor_resource_key(resource: &ResourceRef) -> String {
    editor_position_key(&resource.provider, &resource.location)
}

fn editor_position_key(provider: &ProviderId, location: &Location) -> String {
    format!("{provider}\0{}", location.as_str())
}

fn viewer_state_key(provider: &ProviderId, location: &Location) -> String {
    format!("{provider}\0{}", location.as_str())
}

fn resource_history_kind(invocation: &CommandInvocation) -> Option<ResourceHistoryKind> {
    match invocation
        .arguments
        .get("kind")
        .and_then(near_core::CommandValue::as_str)
    {
        Some("viewed") => Some(ResourceHistoryKind::Viewed),
        Some("edited") => Some(ResourceHistoryKind::Edited),
        _ => None,
    }
}

fn default_conflict_action(plan: &near_ops::OperationPlan) -> ConflictAction {
    match plan.policies().conflict {
        near_ops::ConflictPolicy::Replace => ConflictAction::Replace,
        near_ops::ConflictPolicy::Rename => ConflictAction::Rename,
        near_ops::ConflictPolicy::Ask | near_ops::ConflictPolicy::Skip => ConflictAction::Skip,
    }
}

#[cfg(test)]
fn block_on_provider<T>(mut future: near_core::ProviderFuture<'_, T>) -> Result<T, ProviderError> {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match Future::poll(future.as_mut(), &mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn demo_collection(
    id: &str,
    title: &str,
    location: &str,
    items: Vec<CollectionItem>,
    cursor: usize,
) -> CollectionSurface {
    let provider = ProviderId::from("near.demo");
    let entries = items
        .into_iter()
        .map(|item| {
            let item_location =
                Location::new(format!("{}/{}", location.trim_end_matches('/'), item.name));
            CollectionEntry {
                resource: ResourceRef {
                    provider: provider.clone(),
                    location: item_location,
                },
                metadata: ResourceMetadata {
                    name: item.name,
                    kind: if item.is_directory {
                        ResourceKind::Directory
                    } else {
                        ResourceKind::File
                    },
                    size: None,
                    modified_unix_ms: None,
                    ..ResourceMetadata::default()
                },
                details: item.details,
                selected: item.selected,
            }
        })
        .collect();
    CollectionSurface::new(
        id,
        "workspace.panel",
        title,
        Location::new(location),
        entries,
    )
    .with_cursor(cursor)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::{
        fs,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use near_archive::{ArchiveOperationService, ZipArchiveProvider};
    use near_core::{
        ActionContext, ArgumentKind, ArgumentSchema, CancellationToken, CapabilityId,
        CapabilitySet, Clipboard, CommandDescriptor, CommandExtension, CommandHistoryEntry,
        CommandHistoryStore, CommandId, CommandInvocation, CommandLineArgumentResolver,
        CommandLineExecutor, CommandLineOutput, CommandPrefixDescriptor, CommandValue, ContextId,
        DeviceDisconnectReport, DiagnosticDomain, DiagnosticPhase, ExtensionCommandPrefix,
        ExtensionEffect, ExtensionHelpTopic, ExtensionMenuItem, ExtensionReport, ExtensionSetting,
        FolderLocationEntry, FolderNavigationState, FolderNavigationStore, ListPage, ListRequest,
        ListingGeneration, Location, MetadataValue, OpenRequest, OperationId, ProviderError,
        ProviderFuture, ProviderId, ProviderLocation, RemovableDevice, RemovableDeviceService,
        ResourceEntry, ResourceHistoryEntry, ResourceHistoryState, ResourceHistoryStore,
        ResourceKind, ResourceMetadata, ResourceProvider, ResourceRef, ResourceStream, SafetyClass,
        StateDocumentStore, ViewerStateStore,
    };
    use near_handlers::UserMenuCatalog;
    use near_local_fs::{
        DescribedLocalFileProvider, DescriptionSettings, LocalCommandLineArgumentResolver,
        LocalEditorPositionStore, LocalExternalToolResolver, LocalFileProvider,
        LocalOperationService, LocalViewerStateStore,
    };
    use near_macros::{
        MacroCondition, MacroDocument, MacroStep, MacroStore, MacroTrust, SemanticMacro,
    };
    use near_ops::{
        ConflictAction, ConflictDecision, DecisionScope, ExecutionAuthorization, ExecutionSummary,
        ItemOutcome, ItemStatus, OperationIntent, OperationJournal, OperationKind, OperationPlan,
        OperationPlanner, OperationService, PlanPolicies, PlanRequest, PlannedItem,
    };
    use near_terminal::{
        Key, KeyKind, KeyStroke, KeyboardMode, ModifierKey, Modifiers, MouseButton, MouseEvent,
        MouseEventKind, TerminalEvent,
    };
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{
        CollectionItem, ElevatedRetry, FarWorkspace, FocusedPanel, MainMenuCategory, Overlay,
        PanelType, Rect, block_on_provider, demo_collection, macro_invocation, menu_popup,
    };
    use crate::{
        CollectionEntry, CollectionLookupMode, CollectionSurface, EditorSettings, FilterCatalog,
        HighlightingCatalog, Keymap, MenuItem, MenuSurface, PaneSlot, PanelModeCatalog,
        ResourceOpenPolicy, SemanticColor, SemanticTheme, SortMode, TaskState, TerminalColorDepth,
        TerminalSurface, ViewerSettings, ViewerSurface, WorkspaceAction, parse_key_stroke,
    };

    const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
    const THEME: &str = include_str!("../../../specs/theme.toml");
    const USER_MENU: &str = include_str!("../../../specs/user-menu.toml");

    #[derive(Clone, Default)]
    struct SharedClipboard(Arc<Mutex<String>>);

    impl Clipboard for SharedClipboard {
        fn set_text(&self, text: &str) -> Result<(), String> {
            text.clone_into(&mut self.0.lock().unwrap());
            Ok(())
        }
    }
    const HIGH_CONTRAST: &str = include_str!("../../../specs/theme-high-contrast.toml");
    const TERMINAL_NATIVE: &str = include_str!("../../../specs/theme-terminal-native.toml");
    const HIGHLIGHTING: &str = include_str!("../../../specs/highlighting.toml");

    fn key(name: &str) -> TerminalEvent {
        TerminalEvent::Key(parse_key_stroke(name).unwrap())
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> TerminalEvent {
        TerminalEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: Modifiers::default(),
        })
    }

    struct TestExtension;

    struct TestOpenExtension {
        resource: ResourceRef,
    }

    #[derive(Clone, Default)]
    struct TestMacroStore(Arc<Mutex<Option<MacroDocument>>>);

    impl MacroStore for TestMacroStore {
        fn load(&self) -> Result<MacroDocument, String> {
            self.0
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| "no macro catalog".to_owned())
        }

        fn save(&self, document: &MacroDocument) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(document.clone());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingDeviceService(Arc<Mutex<Vec<String>>>);

    impl RemovableDeviceService for RecordingDeviceService {
        fn list_devices(&self) -> Result<Vec<RemovableDevice>, String> {
            Ok(Vec::new())
        }

        fn disconnect(&self, id: &str) -> Result<DeviceDisconnectReport, String> {
            self.0.lock().unwrap().push(id.to_owned());
            Ok(DeviceDisconnectReport {
                device: id.to_owned(),
                action: "test safe disconnect".to_owned(),
                audit: "executable=test-device args=[disconnect] status=0".to_owned(),
            })
        }
    }

    struct TestDeviceProvider;

    struct PermissiveDemoProvider;

    impl ResourceProvider for PermissiveDemoProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from("near.demo")
        }

        fn schemes(&self) -> &[&str] {
            &["demo"]
        }

        fn list<'a>(
            &'a self,
            _location: &'a Location,
            _request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            Box::pin(async {
                Err(ProviderError::Unsupported(
                    "demo listing is unavailable".to_owned(),
                ))
            })
        }

        fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            Box::pin(async move { Err(ProviderError::NotFound(resource.clone())) })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            _request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async {
                Err(ProviderError::Unsupported(
                    "demo opening is unavailable".to_owned(),
                ))
            })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            CapabilitySet::default()
        }
    }

    struct TestConnectionProvider {
        disconnected: Arc<AtomicBool>,
        reconnected: Arc<AtomicBool>,
    }

    impl ResourceProvider for TestConnectionProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from("test.connection")
        }

        fn schemes(&self) -> &[&str] {
            &["test-remote"]
        }

        fn list<'a>(
            &'a self,
            location: &'a Location,
            request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            Box::pin(async move {
                Ok(ListPage {
                    generation: request.generation,
                    entries: vec![ResourceEntry {
                        resource: ResourceRef {
                            provider: ProviderId::from("test.connection"),
                            location: Location::new(format!(
                                "{}/remote.txt",
                                location.as_str().trim_end_matches('/')
                            )),
                        },
                        metadata: ResourceMetadata {
                            name: "remote.txt".to_owned(),
                            kind: ResourceKind::File,
                            ..ResourceMetadata::default()
                        },
                        details: "remote".to_owned(),
                    }],
                    continuation: None,
                    complete: true,
                })
            })
        }

        fn stat<'a>(&'a self, _resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            Box::pin(async {
                Ok(ResourceMetadata {
                    name: "remote.txt".to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                })
            })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            _request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async {
                Ok(ResourceStream {
                    offset: 0,
                    bytes: b"remote".to_vec(),
                    total_size: Some(6),
                    complete: true,
                })
            })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            CapabilitySet::default()
        }

        fn disconnect(&self, _location: &Location) -> Result<bool, ProviderError> {
            self.disconnected.store(true, Ordering::Relaxed);
            Ok(true)
        }

        fn reconnect(&self, _location: &Location) -> Result<bool, ProviderError> {
            self.reconnected.store(true, Ordering::Relaxed);
            Ok(true)
        }
    }

    impl ResourceProvider for TestDeviceProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from("test.devices")
        }

        fn schemes(&self) -> &[&str] {
            &["test-device"]
        }

        fn list<'a>(
            &'a self,
            _location: &'a Location,
            _request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            Box::pin(async { Err(ProviderError::Unsupported("test listing".to_owned())) })
        }

        fn stat<'a>(&'a self, _resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            Box::pin(async { Err(ProviderError::Unsupported("test stat".to_owned())) })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            _request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async { Err(ProviderError::Unsupported("test open".to_owned())) })
        }

        fn capabilities(&self, resource: &ResourceRef) -> CapabilitySet {
            let mut capabilities = CapabilitySet::default();
            if resource.location.as_str() == "test-device://usb" {
                capabilities.insert("device.disconnect");
            }
            capabilities
        }
    }

    #[derive(Clone, Copy)]
    struct TestCommandLineExecutor;

    struct TemporaryPanelCommandExecutor;

    #[derive(Clone, Default)]
    struct RecordingCommandExecutor {
        commands: Arc<Mutex<Vec<String>>>,
        fail_when_contains: Option<String>,
    }

    #[derive(Clone, Default)]
    struct TestCommandHistoryStore {
        entries: Arc<Mutex<Vec<CommandHistoryEntry>>>,
    }

    #[derive(Clone, Default)]
    struct TestFolderNavigationStore {
        state: Arc<Mutex<FolderNavigationState>>,
    }

    #[derive(Clone, Default)]
    struct TestResourceHistoryStore {
        state: Arc<Mutex<ResourceHistoryState>>,
    }

    #[derive(Clone, Default)]
    struct TestStateDocumentStore {
        documents: Arc<Mutex<BTreeMap<String, String>>>,
    }

    impl StateDocumentStore for TestStateDocumentStore {
        fn load(&self, document: &str) -> Result<Option<String>, String> {
            Ok(self.documents.lock().unwrap().get(document).cloned())
        }

        fn persist(&self, document: &str, contents: &str) -> Result<(), String> {
            self.documents
                .lock()
                .unwrap()
                .insert(document.to_owned(), contents.to_owned());
            Ok(())
        }
    }

    impl FolderNavigationStore for TestFolderNavigationStore {
        fn load(&self) -> Result<FolderNavigationState, String> {
            Ok(self.state.lock().unwrap().clone())
        }

        fn save(&self, state: &FolderNavigationState) -> Result<(), String> {
            state.clone_into(&mut self.state.lock().unwrap());
            Ok(())
        }
    }

    impl CommandHistoryStore for TestCommandHistoryStore {
        fn load(&self) -> Result<Vec<CommandHistoryEntry>, String> {
            Ok(self.entries.lock().unwrap().clone())
        }

        fn save(&self, entries: &[CommandHistoryEntry]) -> Result<(), String> {
            entries.clone_into(&mut self.entries.lock().unwrap());
            Ok(())
        }
    }

    impl ResourceHistoryStore for TestResourceHistoryStore {
        fn load(&self) -> Result<ResourceHistoryState, String> {
            Ok(self.state.lock().unwrap().clone())
        }

        fn save(&self, state: &ResourceHistoryState) -> Result<(), String> {
            state.clone_into(&mut self.state.lock().unwrap());
            Ok(())
        }
    }

    impl CommandLineExecutor for TestCommandLineExecutor {
        fn execute(&self, location: &Location, command: &str) -> Result<CommandLineOutput, String> {
            Ok(CommandLineOutput {
                exit_code: Some(0),
                stdout: format!("executed {command} in {}", location.as_str()),
                stderr: String::new(),
            })
        }
    }

    impl CommandLineExecutor for TemporaryPanelCommandExecutor {
        fn execute(
            &self,
            _location: &Location,
            command: &str,
        ) -> Result<CommandLineOutput, String> {
            Ok(CommandLineOutput {
                exit_code: Some(0),
                stdout: format!("first from {command}\nsecond from {command}\n"),
                stderr: String::new(),
            })
        }
    }

    impl CommandLineExecutor for RecordingCommandExecutor {
        fn execute(
            &self,
            _location: &Location,
            command: &str,
        ) -> Result<CommandLineOutput, String> {
            self.commands.lock().unwrap().push(command.to_owned());
            if self
                .fail_when_contains
                .as_ref()
                .is_some_and(|needle| command.contains(needle))
            {
                return Ok(CommandLineOutput {
                    exit_code: Some(7),
                    stdout: String::new(),
                    stderr: "intentional failure".to_owned(),
                });
            }
            Ok(CommandLineOutput {
                exit_code: Some(0),
                stdout: format!("ran {command}"),
                stderr: String::new(),
            })
        }
    }

    impl CommandExtension for TestExtension {
        fn id(&self) -> &'static str {
            "test.extension"
        }

        fn commands(&self) -> Result<Vec<CommandDescriptor>, String> {
            Ok(vec![CommandDescriptor {
                id: CommandId::from("test.extension.hello"),
                title: "Extension Hello".to_owned(),
                description: "Message from an isolated extension".to_owned(),
                category: vec!["Extensions".to_owned()],
                safety: SafetyClass::ReadOnly,
                arguments: BTreeMap::from([(
                    "text".to_owned(),
                    ArgumentSchema {
                        kind: ArgumentKind::String,
                        required: false,
                        default: None,
                        description: "Prefix argument text".to_owned(),
                    },
                )]),
            }])
        }

        fn command_prefixes(&self) -> Result<Vec<ExtensionCommandPrefix>, String> {
            Ok(vec![ExtensionCommandPrefix {
                prefix: CommandPrefixDescriptor {
                    name: "hello".to_owned(),
                    description: "Send text to the test extension".to_owned(),
                },
                command: CommandId::from("test.extension.hello"),
                argument: "text".to_owned(),
            }])
        }

        fn menu_items(&self) -> Result<Vec<ExtensionMenuItem>, String> {
            Ok(vec![ExtensionMenuItem {
                label: "Say hello".to_owned(),
                description: "Run the greeting action".to_owned(),
                command: CommandId::from("test.extension.hello"),
            }])
        }

        fn settings(&self) -> Result<Vec<ExtensionSetting>, String> {
            Ok(vec![ExtensionSetting {
                id: "greeting".to_owned(),
                label: "Greeting".to_owned(),
                description: "Default greeting text".to_owned(),
                value: "Near".to_owned(),
                required: true,
                secret: false,
            }])
        }

        fn update_settings(&self, settings: &BTreeMap<String, String>) -> Result<(), String> {
            settings
                .get("greeting")
                .filter(|value| !value.trim().is_empty())
                .map(|_| ())
                .ok_or_else(|| "greeting is required".to_owned())
        }

        fn help_topics(&self) -> Result<Vec<ExtensionHelpTopic>, String> {
            Ok(vec![ExtensionHelpTopic {
                id: "greeting".to_owned(),
                title: "Greeting Workflow".to_owned(),
                body: "Configure a greeting, then run Say hello from F11.".to_owned(),
            }])
        }

        fn invoke(
            &self,
            invocation: &CommandInvocation,
            _context: &ActionContext,
        ) -> Result<ExtensionReport, String> {
            if invocation.id.as_str() != "test.extension.hello" {
                return Err("unexpected command".to_owned());
            }
            let message = invocation
                .arguments
                .get("text")
                .and_then(CommandValue::as_str)
                .map_or_else(
                    || "Hello from extension".to_owned(),
                    |text| format!("Hello {text}"),
                );
            Ok(ExtensionReport {
                effect: ExtensionEffect::Message(message),
                diagnostics: vec!["info: invoked".to_owned()],
            })
        }
    }

    impl CommandExtension for TestOpenExtension {
        fn id(&self) -> &'static str {
            "test.open-extension"
        }

        fn commands(&self) -> Result<Vec<CommandDescriptor>, String> {
            Ok(vec![CommandDescriptor {
                id: CommandId::from("test.open-extension.results"),
                title: "Open generated results".to_owned(),
                description: "Open provider-backed extension results".to_owned(),
                category: vec!["Extensions".to_owned()],
                safety: SafetyClass::ReadOnly,
                arguments: BTreeMap::new(),
            }])
        }

        fn invoke(
            &self,
            _invocation: &CommandInvocation,
            _context: &ActionContext,
        ) -> Result<ExtensionReport, String> {
            Ok(ExtensionReport {
                effect: ExtensionEffect::Open(vec![self.resource.clone()]),
                diagnostics: Vec::new(),
            })
        }
    }

    fn filesystem_collection(
        provider: &dyn ResourceProvider,
        path: &std::path::Path,
        id: &str,
    ) -> CollectionSurface {
        let location = LocalFileProvider::location(path);
        let page = block_on_provider(provider.list(
            &location,
            ListRequest {
                generation: ListingGeneration(1),
                continuation: None,
                page_size: 100,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        CollectionSurface::new(
            id,
            "workspace.panel",
            id,
            location,
            page.entries
                .into_iter()
                .map(|entry| CollectionEntry {
                    resource: entry.resource,
                    metadata: entry.metadata,
                    details: entry.details,
                    selected: false,
                })
                .collect(),
        )
    }

    fn zip_fixture(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        use std::io::Write as _;

        let file = fs::File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn wait_for_listing(workspace: &mut FarWorkspace, panel: FocusedPanel) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while workspace
            .listing_state(panel)
            .is_some_and(|state| !state.tasks.is_empty())
            && Instant::now() < deadline
        {
            workspace.poll_background_tasks();
            std::thread::sleep(Duration::from_millis(1));
        }
        workspace.poll_background_tasks();
        assert!(
            workspace
                .listing_state(panel)
                .is_some_and(|state| state.tasks.is_empty()),
            "listing did not settle before timeout"
        );
    }

    fn wait_for_apply_command(workspace: &mut FarWorkspace) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while workspace.apply_command_task.is_some() && Instant::now() < deadline {
            workspace.poll_background_tasks();
            std::thread::sleep(Duration::from_millis(1));
        }
        workspace.poll_background_tasks();
        assert!(
            workspace.apply_command_task.is_none(),
            "Apply command did not settle before timeout"
        );
    }

    fn wait_for_search(workspace: &mut FarWorkspace, panel: FocusedPanel) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while workspace
            .searches
            .get(&panel)
            .is_some_and(|state| state.task.is_some())
            && Instant::now() < deadline
        {
            workspace.poll_background_tasks();
            std::thread::sleep(Duration::from_millis(1));
        }
        workspace.poll_background_tasks();
        assert!(
            workspace
                .searches
                .get(&panel)
                .is_some_and(|state| state.task.is_none()),
            "search did not settle before timeout: {}",
            workspace.status
        );
    }

    fn wait_for_command_line(workspace: &mut FarWorkspace) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while workspace.command_line_task.is_some() && Instant::now() < deadline {
            workspace.poll_background_tasks();
            std::thread::sleep(Duration::from_millis(1));
        }
        workspace.poll_background_tasks();
        assert!(
            workspace.command_line_task.is_none(),
            "command line did not settle before timeout"
        );
    }

    #[test]
    fn renders_dual_panel_far_workspace() {
        let workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();
        let snapshot = workspace.snapshot(&theme, &keymap, 120, 30);
        let screen = snapshot.join("\n");
        assert!(screen.contains("Macintosh HD"));
        assert!(screen.contains("Home"));
        assert!(screen.contains("Cargo.toml"));
        assert!(screen.contains("F10") || screen.contains("10Quit"));
    }

    #[test]
    fn internal_viewer_replaces_the_workspace_full_screen() {
        let mut workspace = FarWorkspace::demo();
        workspace.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
            "test.viewer",
            "Full-screen viewer",
            "viewer-content",
        ))));
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();
        let snapshot = workspace.snapshot(&theme, &keymap, 100, 30);
        let screen = snapshot.join("\n");
        assert!(snapshot[0].contains("Full-screen viewer"));
        assert!(snapshot[0].starts_with('┌'));
        assert!(snapshot[0].ends_with('┐'));
        assert!(screen.contains("viewer-content"));
        assert!(!screen.contains("Macintosh HD"));
        assert!(!screen.contains("Cargo.toml"));
    }

    #[test]
    fn viewer_copy_and_provider_scoped_state_survive_reopen() {
        let root =
            std::env::temp_dir().join(format!("near-fm-viewer-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("document.txt"), b"alpha beta\nsecond line\n").unwrap();
        let state_store = LocalViewerStateStore::new(root.join("viewer-state.toml"));
        let clipboard = SharedClipboard::default();
        let recorded = Arc::clone(&clipboard.0);
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "viewer-state.left");
        let right = filesystem_collection(provider.as_ref(), &root, "viewer-state.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider.clone())
            .with_viewer_state_store(state_store.clone())
            .with_clipboard(clipboard);

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.view".into(),
            arguments: BTreeMap::new(),
        });
        for _ in 0..5 {
            workspace.dispatch(&CommandInvocation {
                id: "near.viewer.select-right".into(),
                arguments: BTreeMap::new(),
            });
        }
        workspace.dispatch(&CommandInvocation {
            id: "near.viewer.copy".into(),
            arguments: BTreeMap::new(),
        });
        assert_eq!(&*recorded.lock().unwrap(), "alpha");
        workspace.dispatch(&CommandInvocation {
            id: "near.viewer.bookmark-set".into(),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(2))]),
        });
        workspace.dispatch(&CommandInvocation {
            id: "near.viewer.down".into(),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation::new("near.viewer.toggle-wrap"));
        workspace.dispatch(&CommandInvocation::new("near.viewer.toggle-hex"));
        workspace.dispatch(&CommandInvocation::new("near.viewer.cycle-encoding"));
        workspace.dispatch(&CommandInvocation {
            id: "near.overlay.cancel".into(),
            arguments: BTreeMap::new(),
        });

        let persisted = state_store.load().unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].offset, 11);
        assert_eq!(persisted[0].bookmarks.get(&2), Some(&0));
        assert_eq!(persisted[0].encoding.as_deref(), Some("latin-1"));
        assert_eq!(persisted[0].wrap, Some(true));
        assert_eq!(persisted[0].hex, Some(true));

        let left = filesystem_collection(provider.as_ref(), &root, "viewer-state.left.reopen");
        let right = filesystem_collection(provider.as_ref(), &root, "viewer-state.right.reopen");
        let mut reopened = FarWorkspace::new(left, right)
            .with_provider(provider.clone())
            .with_viewer_state_store(state_store);
        reopened.dispatch(&CommandInvocation {
            id: "near.resource.view".into(),
            arguments: BTreeMap::new(),
        });
        let Some(Overlay::Surface(surface)) = &reopened.overlay else {
            panic!("viewer should reopen as a surface");
        };
        let restored = surface.viewer_state().unwrap();
        assert_eq!(restored.offset, 11);
        assert_eq!(restored.bookmarks.get(&2), Some(&0));
        assert_eq!(restored.encoding.as_deref(), Some("latin-1"));
        assert_eq!(restored.wrap, Some(true));
        assert_eq!(restored.hex, Some(true));

        let mut policy = ViewerSettings::default();
        policy.remember_position = false;
        policy.remember_bookmarks = false;
        policy.remember_encoding = false;
        policy.remember_view_mode = false;
        let left = filesystem_collection(provider.as_ref(), &root, "viewer-state.left.filtered");
        let right = filesystem_collection(provider.as_ref(), &root, "viewer-state.right.filtered");
        let mut filtered = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_viewer_settings(policy)
            .with_viewer_state_store(LocalViewerStateStore::new(root.join("viewer-state.toml")));
        filtered.dispatch(&CommandInvocation::new("near.resource.view"));
        let Some(Overlay::Surface(surface)) = &filtered.overlay else {
            panic!("viewer should reopen with filtered state");
        };
        let restored = surface.viewer_state().unwrap();
        assert_eq!(restored.offset, 0);
        assert!(restored.bookmarks.is_empty());
        assert_eq!(restored.encoding.as_deref(), Some("utf-8"));
        assert_eq!(restored.wrap, Some(false));
        assert_eq!(restored.hex, Some(false));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn held_modifiers_project_alternate_function_key_hints() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let base = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(base.contains("3View"));

        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(KeyStroke {
                key: Key::Modifier(ModifierKey::Alt),
                modifiers: Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
                kind: KeyKind::Press,
            }),
        );
        let alternate = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(alternate.contains("1Choose left/provider"));
        assert!(alternate.contains("7Find files"));
        assert!(!alternate.contains("3View"));

        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(KeyStroke {
                key: Key::Modifier(ModifierKey::Alt),
                modifiers: Modifiers::default(),
                kind: KeyKind::Release,
            }),
        );
        let restored = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(restored.contains("3View"));
    }

    #[test]
    fn far_sort_shortcuts_update_the_panel_and_open_the_sort_menu() {
        let mut workspace = FarWorkspace::demo()
            .with_highlighting(HighlightingCatalog::from_toml(HIGHLIGHTING).unwrap());
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+F6"));
        assert_eq!(workspace.focused_panel().sort_state().mode, SortMode::Size);
        assert!(workspace.status.contains("Size ↑"));
        let panel = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(panel.contains("[Size ↑]"));

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+F12"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "overlay.menu");
        let menu = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(menu.contains("Sort Modes"));
        assert!(menu.contains("Name"));

        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        workspace.handle_terminal_event(&mut keymap, key("Shift+F11"));
        assert!(workspace.focused_panel().sort_state().sort_groups);
        assert!(workspace.status.contains('G'));
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.highlighting.report"),
            arguments: BTreeMap::new(),
        });
        let report = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(report.contains("readonly-executable"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.collection.sort.toggle-reverse"),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.focused_panel().sort_state().reverse);
    }

    #[test]
    fn mouse_click_right_click_wheel_and_keybar_follow_workspace_semantics() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.snapshot(&theme, &keymap, 100, 30);

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 75, 3),
        );
        assert_eq!(workspace.focused, FocusedPanel::Right);
        assert_eq!(workspace.right.cursor(), 2);

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Right), 75, 3),
        );
        assert!(workspace.right.current().unwrap().selected);

        workspace.handle_terminal_event(&mut keymap, mouse(MouseEventKind::ScrollDown, 75, 3));
        assert_eq!(workspace.right.cursor(), 5);

        let action = workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 0, 29),
        );
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.help.context"
        ));
        assert!(matches!(workspace.overlay, Some(Overlay::Surface(_))));
    }

    #[test]
    fn mouse_click_activates_visible_menu_rows() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.overlay = Some(Overlay::Menu(MenuSurface::new(
            "mouse.menu",
            "Mouse Menu",
            vec![MenuItem {
                label: "&Peer".to_owned(),
                description: "Focus the peer panel".to_owned(),
                command: CommandInvocation {
                    id: CommandId::from("near.workspace.focus-peer"),
                    arguments: BTreeMap::new(),
                },
                enabled: true,
            }],
        )));
        workspace.snapshot(&theme, &keymap, 100, 30);

        let action = workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 13, 6),
        );
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.workspace.focus-peer"
        ));
        assert_eq!(workspace.focused, FocusedPanel::Right);
        assert!(workspace.overlay.is_none());
    }

    #[test]
    fn mouse_click_activates_the_last_main_menu_row() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.overlay = Some(Overlay::Menu(
            MenuSurface::new(
                "mouse.main-menu",
                "Files",
                vec![
                    MenuItem {
                        label: "&Help".to_owned(),
                        description: "Open help".to_owned(),
                        command: CommandInvocation {
                            id: CommandId::from("near.help.context"),
                            arguments: BTreeMap::new(),
                        },
                        enabled: true,
                    },
                    MenuItem {
                        label: "&Peer".to_owned(),
                        description: "Focus the peer panel".to_owned(),
                        command: CommandInvocation {
                            id: CommandId::from("near.workspace.focus-peer"),
                            arguments: BTreeMap::new(),
                        },
                        enabled: true,
                    },
                ],
            )
            .with_main_menu(vec!["Files".to_owned()], 0),
        ));
        workspace.snapshot(&theme, &keymap, 100, 30);
        let Some(Overlay::Menu(menu)) = &workspace.overlay else {
            panic!("main menu should be open");
        };
        let popup = menu_popup(Rect::new(0, 0, 100, 30), menu);

        let action = workspace.handle_terminal_event(
            &mut keymap,
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                popup.x.saturating_add(1),
                popup.bottom().saturating_sub(2),
            ),
        );

        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.workspace.focus-peer"
        ));
        assert_eq!(workspace.focused, FocusedPanel::Right);
    }

    #[test]
    fn mouse_switching_main_menu_headers_does_not_stack_category_history() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.handle_terminal_event(&mut keymap, key("F9"));
        workspace.snapshot(&theme, &keymap, 100, 30);
        let Some(Overlay::Menu(menu)) = &workspace.overlay else {
            panic!("main menu should be open");
        };
        let files_column = menu.main_menu_index_at(0, 8).unwrap();
        assert_eq!(
            MainMenuCategory::from_index(files_column),
            MainMenuCategory::Files
        );

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 8, 0),
        );
        assert!(workspace.overlay_history.is_empty());
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        assert!(workspace.overlay.is_none());
    }

    #[test]
    fn mouse_wheel_routes_to_full_screen_surfaces() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
            "mouse.viewer",
            "Mouse Viewer",
            (1..=40)
                .map(|line| format!("viewer-line-{line:02}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ))));
        let before = workspace.snapshot(&theme, &keymap, 80, 12).join("\n");
        assert!(before.contains("viewer-line-01"));

        let action =
            workspace.handle_terminal_event(&mut keymap, mouse(MouseEventKind::ScrollDown, 10, 5));
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.viewer.down"
        ));
        let after = workspace.snapshot(&theme, &keymap, 80, 12).join("\n");
        assert!(!after.contains("viewer-line-01"));
        assert!(after.contains("viewer-line-02"));
    }

    #[test]
    fn cross_panel_mouse_drag_previews_copy_and_shift_move() {
        let root = std::env::temp_dir().join(format!("near-fm-mouse-drag-{}", std::process::id()));
        let source = root.join("source");
        let destination = root.join("destination");
        let trash = root.join("trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("drag.txt"), b"drag").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &source, "mouse.source");
        let right = filesystem_collection(provider.as_ref(), &destination, "mouse.destination");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                trash,
                OperationJournal::memory(),
            ));
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.snapshot(&theme, &keymap, 100, 30);

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 10, 1),
        );
        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Drag(MouseButton::Left), 75, 1),
        );
        assert!(workspace.status.contains("Drag preview: copy"));
        let action = workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Up(MouseButton::Left), 75, 1),
        );
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.resource.copy-to-peer"
        ));
        let copy_preview = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(copy_preview.contains("operation: Copy"));

        workspace.overlay = None;
        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 10, 1),
        );
        let TerminalEvent::Mouse(mut shifted_drag) =
            mouse(MouseEventKind::Drag(MouseButton::Left), 75, 1)
        else {
            unreachable!()
        };
        shifted_drag.modifiers.shift = true;
        workspace.handle_terminal_event(&mut keymap, TerminalEvent::Mouse(shifted_drag));
        let action = workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: 75,
                row: 1,
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
            }),
        );
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref invocation)
                if invocation.id.as_str() == "near.resource.move-to-peer"
        ));
        let move_preview = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(move_preview.contains("operation: Move"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incremental_filename_lookup_filters_cycles_and_restores_cursor() {
        let left = demo_collection(
            "lookup.left",
            "Lookup",
            "/lookup",
            vec![
                CollectionItem::file("alpha.txt", "file"),
                CollectionItem::file("cargo.toml", "file"),
                CollectionItem::file("cat.txt", "file"),
                CollectionItem::file("my-docs.md", "file"),
            ],
            0,
        );
        let right = demo_collection(
            "lookup.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        #[cfg(feature = "embedded-pty")]
        workspace.ensure_embedded_terminal().unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.command_line.insert("printf buffered");

        workspace.handle_terminal_event(&mut keymap, key("Alt+C"));
        assert_eq!(workspace.command_line.buffer(), "printf buffered");
        assert_eq!(
            workspace.focused_panel().current().unwrap().metadata.name,
            "cargo.toml"
        );
        let prompt = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(prompt.contains("Find: c_   1 of 2"));
        assert!(prompt.contains("↑↓ next"));
        assert!(!prompt.contains("1Help"));
        let semantic = workspace.semantic_snapshot(&theme, &keymap, 100, 30);
        let prompt_row = semantic
            .text_lines()
            .iter()
            .position(|line| line.contains("Find: c_"))
            .unwrap();
        assert_eq!(prompt_row, usize::from(semantic.height()) - 2);
        assert!(
            semantic
                .role_lines()
                .iter()
                .filter(|line| line.contains("lookup.match"))
                .count()
                >= 2
        );

        workspace.handle_terminal_event(&mut keymap, key("Alt+C"));
        assert_eq!(
            workspace.focused_panel().current().unwrap().metadata.name,
            "cat.txt"
        );

        workspace.handle_terminal_event(&mut keymap, key("Alt+A"));
        workspace.handle_terminal_event(&mut keymap, key("Alt+T"));
        assert_eq!(
            workspace.focused_panel().current().unwrap().metadata.name,
            "cat.txt"
        );
        assert_eq!(workspace.filename_lookup.as_ref().unwrap().query, "cat");
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        assert!(workspace.filename_lookup.is_none());
        assert_eq!(
            workspace.focused_panel().current().unwrap().metadata.name,
            "cat.txt"
        );

        workspace.focused_panel_mut().set_cursor(0);
        workspace.handle_terminal_event(&mut keymap, key("Alt+C"));
        workspace.handle_terminal_event(&mut keymap, key("Alt+A"));
        workspace.handle_terminal_event(&mut keymap, key("Backspace"));
        assert_eq!(workspace.filename_lookup.as_ref().unwrap().query, "c");

        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        assert!(workspace.filename_lookup.is_none());
        assert_eq!(workspace.focused_panel().cursor(), 0);
        assert_eq!(
            workspace.focused_panel().current().unwrap().metadata.name,
            "alpha.txt"
        );

        workspace.handle_terminal_event(&mut keymap, key("Alt+O"));
        workspace.handle_terminal_event(&mut keymap, key("Alt+C"));
        workspace.handle_terminal_event(&mut keymap, key("Alt+S"));
        assert_eq!(
            workspace.focused_panel().current().unwrap().metadata.name,
            "my-docs.md"
        );
        assert_eq!(
            workspace.filename_lookup.as_ref().unwrap().mode,
            CollectionLookupMode::Contains
        );
        let fallback = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(fallback.contains("Find: ocs_   contains 1 of 1"));
    }

    #[test]
    fn panel_interaction_conformance_plain_arrows_preserve_selection_and_clamp() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let initial = workspace.focused_panel().cursor();

        workspace.handle_terminal_event(&mut keymap, key("Space"));
        let selected = workspace.focused_panel().selected_resources();
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        assert_eq!(workspace.focused_panel().cursor(), initial + 2);
        assert_eq!(workspace.focused_panel().selected_resources(), selected);

        for _ in 0..workspace.focused_panel().entries().len() + 2 {
            workspace.handle_terminal_event(&mut keymap, key("Up"));
        }
        assert_eq!(workspace.focused_panel().cursor(), 0);
        for _ in 0..workspace.focused_panel().entries().len() + 2 {
            workspace.handle_terminal_event(&mut keymap, key("Down"));
        }
        assert_eq!(
            workspace.focused_panel().cursor(),
            workspace.focused_panel().entries().len() - 1
        );
        assert_eq!(workspace.focused_panel().selected_resources(), selected);
    }

    #[test]
    fn panel_navigation_acknowledges_enhanced_key_repeat_on_both_panes() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let left_start = workspace.left.cursor();
        let mut down_repeat = parse_key_stroke("Down").unwrap();
        down_repeat.kind = KeyKind::Repeat;

        for _ in 0..4 {
            workspace.handle_terminal_event(&mut keymap, TerminalEvent::Key(down_repeat.clone()));
        }
        assert_eq!(workspace.left.cursor(), left_start + 4);

        workspace.handle_terminal_event(&mut keymap, key("Tab"));
        let right_start = workspace.right.cursor();
        let mut up_repeat = parse_key_stroke("Up").unwrap();
        up_repeat.kind = KeyKind::Repeat;
        for _ in 0..2 {
            workspace.handle_terminal_event(&mut keymap, TerminalEvent::Key(up_repeat.clone()));
        }
        assert_eq!(workspace.right.cursor(), right_start - 2);
    }

    #[test]
    fn panel_interaction_conformance_edges_are_visible() {
        let items = (0..64)
            .map(|index| CollectionItem::file(format!("item-{index:02}.txt"), "file"))
            .collect();
        let left = demo_collection("edges.left", "Edges", "/edges", items, 8);
        let right = demo_collection(
            "edges.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Left"));
        assert_eq!(workspace.focused_panel().cursor(), 0);
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 24)
                .join("\n")
                .contains("item-00.txt")
        );

        workspace.handle_terminal_event(&mut keymap, key("Right"));
        assert_eq!(workspace.focused_panel().cursor(), 63);
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 24)
                .join("\n")
                .contains("item-63.txt")
        );
        let final_page_start = workspace.focused_panel().viewport().start();
        workspace.handle_terminal_event(&mut keymap, key("Up"));
        workspace.snapshot(&theme, &keymap, 80, 24);
        assert_eq!(workspace.focused_panel().cursor(), 62);
        assert_eq!(
            workspace.focused_panel().viewport().start(),
            final_page_start,
            "plain movement inside the viewport must not scroll the page"
        );

        workspace.handle_terminal_event(&mut keymap, key("Home"));
        assert_eq!(workspace.focused_panel().cursor(), 0);
        workspace.handle_terminal_event(&mut keymap, key("End"));
        assert_eq!(workspace.focused_panel().cursor(), 63);
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 24)
                .join("\n")
                .contains("item-63.txt")
        );
    }

    #[test]
    fn panel_interaction_conformance_horizontal_scroll_uses_far_bindings() {
        let long_name = "alpha-界-bravo-charlie-delta-echo-foxtrot.txt";
        let left = demo_collection(
            "horizontal.left",
            "Horizontal",
            "/horizontal",
            vec![CollectionItem::file(long_name, "file")],
            0,
        );
        let right = demo_collection(
            "horizontal.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        let initial = workspace.snapshot(&theme, &keymap, 52, 12).join("\n");
        assert!(initial.contains("alpha-界"));
        workspace.handle_terminal_event(&mut keymap, key("Alt+End"));
        let shifted = workspace.snapshot(&theme, &keymap, 52, 12).join("\n");
        assert!(workspace.focused_panel().horizontal_offset() > 0);
        assert!(shifted.contains("rot.txt"), "shifted panel:\n{shifted}");
        assert!(!shifted.contains("alpha-界"));

        workspace.handle_terminal_event(&mut keymap, key("Alt+Home"));
        assert_eq!(workspace.focused_panel().horizontal_offset(), 0);
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+Right"));
        assert!(workspace.focused_panel().horizontal_offset() > 0);
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+Left"));
        assert_eq!(workspace.focused_panel().horizontal_offset(), 0);
    }

    #[test]
    fn panel_interaction_conformance_resize_updates_render_and_mouse_geometry() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.snapshot(&theme, &keymap, 100, 30);
        assert_eq!(
            workspace.panel_layout.geometry(100, 27, 8, 3).first_width,
            50
        );

        let resize_width = workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+Right"));
        assert!(
            matches!(resize_width, WorkspaceAction::Command(ref invocation) if invocation.id.as_str() == "near.workspace.resize-panels"),
            "resize action: {resize_width:?}"
        );
        let WorkspaceAction::Command(resize_invocation) = &resize_width else {
            unreachable!();
        };
        assert_eq!(
            resize_invocation
                .arguments
                .get("columns")
                .and_then(CommandValue::as_i64),
            Some(10)
        );
        assert_eq!(workspace.status, "Panel layout resized");
        assert_eq!(
            workspace.panel_layout.geometry(100, 27, 8, 3).first_width,
            60
        );
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+Up"));
        let resized = workspace.panel_layout.geometry(100, 27, 8, 3);
        assert_eq!(resized.first_width, 60);
        assert_eq!(resized.pane_height, 22);
        workspace.snapshot(&theme, &keymap, 100, 30);

        workspace.focused = FocusedPanel::Right;
        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 55, 1),
        );
        assert_eq!(workspace.focused, FocusedPanel::Left);

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+Home"));
        let reset = workspace.panel_layout.geometry(100, 27, 8, 3);
        assert_eq!(reset.first_width, 50);
        assert_eq!(reset.pane_height, 27);
    }

    #[test]
    fn panel_interaction_conformance_paging_and_resize_follow_viewport() {
        let items = (0..64)
            .map(|index| CollectionItem::file(format!("item-{index:02}.txt"), "file"))
            .collect();
        let left = demo_collection("paging.left", "Paging", "/paging", items, 0);
        let right = demo_collection(
            "paging.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.snapshot(&theme, &keymap, 80, 24);
        let page_rows = workspace.focused_panel().viewport().visible_rows();
        assert!(page_rows > 1);

        workspace.handle_terminal_event(&mut keymap, key("End"));
        assert_eq!(workspace.focused_panel().cursor(), 63);
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 24)
                .join("\n")
                .contains("item-63.txt")
        );

        workspace.handle_terminal_event(&mut keymap, key("Home"));
        assert_eq!(workspace.focused_panel().cursor(), 0);
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 24)
                .join("\n")
                .contains("item-00.txt")
        );

        workspace.handle_terminal_event(&mut keymap, key("PageDown"));
        assert_eq!(workspace.focused_panel().cursor(), page_rows);
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 24)
                .join("\n")
                .contains(&format!("item-{page_rows:02}.txt"))
        );

        workspace.handle_terminal_event(&mut keymap, key("PageDown"));
        assert_eq!(workspace.focused_panel().cursor(), page_rows * 2);
        workspace.handle_terminal_event(&mut keymap, key("PageUp"));
        assert_eq!(workspace.focused_panel().cursor(), page_rows);

        workspace.snapshot(&theme, &keymap, 80, 16);
        let resized_page_rows = workspace.focused_panel().viewport().visible_rows();
        assert!(resized_page_rows < page_rows);
        workspace.handle_terminal_event(&mut keymap, key("PageDown"));
        assert_eq!(
            workspace.focused_panel().cursor(),
            page_rows + resized_page_rows
        );
        let current = workspace
            .focused_panel()
            .current()
            .unwrap()
            .metadata
            .name
            .clone();
        assert!(
            workspace
                .snapshot(&theme, &keymap, 80, 16)
                .join("\n")
                .contains(&current)
        );
    }

    #[test]
    fn panel_interaction_conformance_shift_and_insert_select_while_moving() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let initial = workspace.focused_panel().cursor();

        workspace.handle_terminal_event(&mut keymap, key("Shift+Down"));
        assert!(workspace.focused_panel().entries()[initial].selected);
        assert_eq!(workspace.focused_panel().cursor(), initial + 1);

        let snapshot = workspace.semantic_snapshot(&theme, &keymap, 100, 30);
        assert!(
            snapshot
                .role_lines()
                .join("\n")
                .contains("panel.item.selected")
        );
        assert!(snapshot.text_lines().join("\n").contains('√'));

        workspace.handle_terminal_event(&mut keymap, key("Shift+Up"));
        assert!(workspace.focused_panel().entries()[initial + 1].selected);
        assert_eq!(workspace.focused_panel().cursor(), initial);

        workspace.handle_terminal_event(&mut keymap, key("Insert"));
        assert!(!workspace.focused_panel().entries()[initial].selected);
        assert_eq!(workspace.focused_panel().cursor(), initial + 1);
    }

    #[test]
    fn panel_interaction_conformance_non_contiguous_selection() {
        let items = (0..8)
            .map(|index| CollectionItem::file(format!("item-{index}.txt"), "file"))
            .collect();
        let left = demo_collection("selection.left", "Selection", "/selection", items, 0);
        let right = demo_collection(
            "selection.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Shift+Down"));
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        assert_eq!(workspace.focused_panel().cursor(), 2);
        assert!(workspace.focused_panel().entries()[0].selected);
        assert!(!workspace.focused_panel().entries()[1].selected);
        assert!(!workspace.focused_panel().entries()[2].selected);

        workspace.handle_terminal_event(&mut keymap, key("Insert"));
        assert_eq!(workspace.focused_panel().cursor(), 3);
        let selected = workspace
            .focused_panel()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.selected.then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(selected, vec![0, 2]);

        let roles = workspace
            .semantic_snapshot(&theme, &keymap, 100, 30)
            .role_lines()
            .join("\n");
        assert!(roles.contains("panel.item.selected"));
        assert!(roles.contains("panel.item.focused"));
    }

    #[test]
    fn panel_interaction_conformance_selection_survives_navigation() {
        let items = (0..64)
            .map(|index| CollectionItem::file(format!("item-{index:02}.txt"), "file"))
            .collect();
        let left = demo_collection("selection-nav.left", "Selection", "/selection", items, 0);
        let right = demo_collection(
            "selection-nav.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.snapshot(&theme, &keymap, 80, 24);

        workspace.handle_terminal_event(&mut keymap, key("Shift+Down"));
        workspace.handle_terminal_event(&mut keymap, key("PageDown"));
        workspace.handle_terminal_event(&mut keymap, key("End"));
        assert!(workspace.focused_panel().entries()[0].selected);
        workspace.handle_terminal_event(&mut keymap, key("Home"));
        assert!(workspace.focused_panel().entries()[0].selected);
        assert!(
            workspace
                .semantic_snapshot(&theme, &keymap, 80, 24)
                .role_lines()
                .join("\n")
                .contains("panel.item.selected.focused")
        );
    }

    #[test]
    fn panel_interaction_conformance_mouse_targets_scrolled_rows() {
        let items = (0..64)
            .map(|index| CollectionItem::file(format!("item-{index:02}.txt"), "file"))
            .collect();
        let left = demo_collection("mouse-scroll.left", "Mouse", "/mouse", items, 0);
        let right = demo_collection(
            "mouse-scroll.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.snapshot(&theme, &keymap, 80, 24);
        workspace.handle_terminal_event(&mut keymap, key("End"));
        workspace.snapshot(&theme, &keymap, 80, 24);
        let visible_start = workspace.focused_panel().viewport().start();

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 4, 1),
        );
        assert_eq!(workspace.focused_panel().cursor(), visible_start);
        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Right), 4, 2),
        );
        assert_eq!(workspace.focused_panel().cursor(), visible_start + 1);
        assert!(workspace.focused_panel().entries()[visible_start + 1].selected);
    }

    #[test]
    fn enhanced_modifier_hold_starts_lookup_and_bound_alt_chords_win() {
        let mut workspace = FarWorkspace::demo().with_keyboard_mode(KeyboardMode::Enhanced);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(KeyStroke {
                key: Key::Modifier(ModifierKey::Alt),
                modifiers: Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
                kind: KeyKind::Press,
            }),
        );
        workspace.handle_terminal_event(&mut keymap, key("c"));
        assert_eq!(workspace.filename_lookup.as_ref().unwrap().query, "c");
        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(KeyStroke {
                key: Key::Modifier(ModifierKey::Alt),
                modifiers: Modifiers::default(),
                kind: KeyKind::Release,
            }),
        );
        workspace.handle_terminal_event(&mut keymap, key("Esc"));

        let action = workspace.handle_terminal_event(&mut keymap, key("Alt+1"));
        assert!(matches!(action, WorkspaceAction::Command(_)));
        assert!(workspace.filename_lookup.is_none());
    }

    #[test]
    fn legacy_keyboard_uses_direct_alt_chords_without_fake_hold_layers() {
        let mut workspace = FarWorkspace::demo().with_keyboard_mode(KeyboardMode::Legacy);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let base = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");

        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Key(KeyStroke {
                key: Key::Modifier(ModifierKey::Alt),
                modifiers: Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
                kind: KeyKind::Press,
            }),
        );
        let unchanged = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert_eq!(base, unchanged);
        assert!(
            workspace
                .configuration_diagnostics
                .contains("keyboard=legacy")
        );

        workspace.handle_terminal_event(&mut keymap, key("Alt+C"));
        assert_eq!(workspace.filename_lookup.as_ref().unwrap().query, "c");
    }

    #[test]
    fn selection_menu_masks_and_saved_sets_drive_panel_selection() {
        let left = demo_collection(
            "selection.left",
            "Selection",
            "/selection",
            vec![
                CollectionItem::file("report.rs", "file"),
                CollectionItem::file("report.md", "file"),
                CollectionItem::file("notes.rs", "file"),
            ],
            0,
        );
        let right = demo_collection(
            "selection.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+S"));
        let menu = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(menu.contains("Selection"));
        assert!(menu.contains("Select by mask"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.selection.mask-confirmed"),
            arguments: BTreeMap::from([
                (
                    "include".to_owned(),
                    CommandValue::String("*.rs;*.md".to_owned()),
                ),
                (
                    "exclude".to_owned(),
                    CommandValue::String("notes*".to_owned()),
                ),
                ("selected".to_owned(), CommandValue::Boolean(true)),
            ]),
        });
        assert_eq!(workspace.focused_panel().selected_resources().len(), 2);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.selection.save"),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.selection.invert"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.focused_panel().selected_resources().len(), 1);
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.selection.restore"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.focused_panel().selected_resources().len(), 2);
    }

    #[test]
    fn configurable_folder_comparison_selects_panel_differences() {
        let comparison_entry = |root: &str, name: &str, size: u64, modified: i64| {
            CollectionEntry::new(
                ResourceRef {
                    provider: ProviderId::from("near.demo"),
                    location: Location::new(format!("{root}/{name}")),
                },
                ResourceMetadata {
                    name: name.to_owned(),
                    kind: ResourceKind::File,
                    size: Some(size),
                    modified_unix_ms: Some(modified),
                    ..ResourceMetadata::default()
                },
                format!("{size} B"),
            )
        };
        let left = CollectionSurface::new(
            "compare.left",
            "workspace.panel",
            "Left",
            Location::new("/left"),
            vec![
                comparison_entry("/left", "equal.txt", 10, 100),
                comparison_entry("/left", "changed.txt", 20, 300),
                comparison_entry("/left", "left-only.txt", 1, 100),
            ],
        );
        let right = CollectionSurface::new(
            "compare.right",
            "workspace.panel",
            "Right",
            Location::new("/right"),
            vec![
                comparison_entry("/right", "equal.txt", 10, 100),
                comparison_entry("/right", "changed.txt", 15, 200),
                comparison_entry("/right", "right-only.txt", 1, 100),
            ],
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+C"));
        let dialog = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(dialog.contains("Compare folders"));
        assert!(dialog.contains("Compare size"));
        assert!(dialog.contains("Time tolerance"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.selection.compare-folders-confirmed"),
            arguments: BTreeMap::from([
                (
                    "compare_size".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
                (
                    "compare_modified".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
                (
                    "tolerance_seconds".to_owned(),
                    CommandValue::String("0".to_owned()),
                ),
                (
                    "case_sensitive".to_owned(),
                    CommandValue::String("no".to_owned()),
                ),
                (
                    "selection".to_owned(),
                    CommandValue::String("newer".to_owned()),
                ),
            ]),
        });

        let left_selected = workspace.left.selected_resources();
        let right_selected = workspace.right.selected_resources();
        assert_eq!(left_selected.len(), 2);
        assert_eq!(right_selected.len(), 1);
        assert!(
            left_selected
                .iter()
                .any(|resource| resource.location.as_str() == "/left/changed.txt")
        );
        assert!(
            left_selected
                .iter()
                .any(|resource| resource.location.as_str() == "/left/left-only.txt")
        );
        assert!(
            right_selected
                .iter()
                .any(|resource| resource.location.as_str() == "/right/right-only.txt")
        );
        assert!(workspace.status.contains("1 differing"));
        assert!(workspace.status.contains("3 selected"));
    }

    #[test]
    fn panel_modes_are_configurable_and_assigned_independently_per_panel() {
        let catalog = PanelModeCatalog::from_toml(
            r#"
                schema = 1
                [defaults]
                left = "compact"
                right = "metadata"

            "#,
        )
        .unwrap();
        let mut workspace = FarWorkspace::demo().with_panel_modes(catalog);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        assert_eq!(workspace.left.view_mode().id, "compact");
        assert_eq!(workspace.right.view_mode().id, "metadata");
        workspace.handle_terminal_event(&mut keymap, key("Alt+3"));
        assert_eq!(workspace.left.view_mode().id, "full");
        assert_eq!(workspace.right.view_mode().id, "metadata");
    }

    #[test]
    fn folder_comparison_rejects_invalid_policy_values_without_changing_selection() {
        let mut workspace = FarWorkspace::demo();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.selection.compare-folders-confirmed"),
            arguments: BTreeMap::from([
                (
                    "compare_size".to_owned(),
                    CommandValue::String("sometimes".to_owned()),
                ),
                (
                    "compare_modified".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
                (
                    "tolerance_seconds".to_owned(),
                    CommandValue::String("0".to_owned()),
                ),
                (
                    "case_sensitive".to_owned(),
                    CommandValue::String("no".to_owned()),
                ),
                (
                    "selection".to_owned(),
                    CommandValue::String("newer".to_owned()),
                ),
            ]),
        });

        assert_eq!(workspace.status, "Compare size must be yes or no");
        assert!(workspace.left.selected_resources().is_empty());
        assert!(workspace.right.selected_resources().is_empty());
    }

    #[test]
    fn accepting_filename_lookup_keeps_the_matched_cursor() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let status = workspace.status.clone();
        workspace.handle_terminal_event(&mut keymap, key("Alt+C"));
        let matched = workspace.focused_panel().cursor();
        workspace.handle_terminal_event(&mut keymap, key("Enter"));

        assert!(workspace.filename_lookup.is_none());
        assert_eq!(workspace.focused_panel().cursor(), matched);
        assert_eq!(workspace.status, status);
    }

    #[test]
    fn keymap_drives_workspace_commands() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        assert!(matches!(
            workspace.handle_terminal_event(&mut keymap, key("F9")),
            WorkspaceAction::Command(_)
        ));
        assert_eq!(workspace.active_contexts()[0].as_str(), "overlay.menu");
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "workspace.panel");
    }

    #[test]
    fn f9_opens_the_active_panel_menu_and_switches_far_categories() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F9"));
        let top_lines = workspace.snapshot(&theme, &keymap, 100, 30);
        let top = top_lines.join("\n");
        for label in ["Left", "Files", "Commands", "Options", "Right"] {
            assert!(top.contains(label), "missing top-menu label {label}");
        }
        for label in ["Left", "Files", "Commands", "Options", "Right"] {
            assert!(
                top_lines[0].contains(label),
                "{label} should be rendered in the top screen row"
            );
        }
        assert!(
            top_lines[1].contains("Left Menu"),
            "the active submenu must drop directly below the top bar"
        );
        assert!(top.contains("Left Menu"));
        assert!(top.contains("[B]rief"));

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 15, 0),
        );
        let clicked = workspace.snapshot(&theme, &keymap, 100, 30);
        assert!(clicked[0].contains("Commands"));
        assert!(clicked[1].contains("Commands Menu"));
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.menu.left"),
            arguments: BTreeMap::new(),
        });

        workspace.handle_terminal_event(&mut keymap, key("Right"));
        let files = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(files.contains("Files Menu"));

        workspace.handle_terminal_event(&mut keymap, key("Right"));
        let commands = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(commands.contains("Commands Menu"));

        workspace.handle_terminal_event(&mut keymap, key("Right"));
        let options = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(options.contains("Options Menu"));

        workspace.handle_terminal_event(&mut keymap, key("Right"));
        assert_eq!(workspace.focused, FocusedPanel::Right);
        let right = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(right.contains("Right Menu"));
        assert!(right.contains("[B]rief"));
        assert!(right.contains("[S]ort modes"));

        workspace.handle_terminal_event(&mut keymap, key("Tab"));
        assert_eq!(workspace.focused, FocusedPanel::Left);
        let left = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(left.contains("Left Menu"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.menu.files"),
            arguments: BTreeMap::new(),
        });
        let files = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(files.contains("Files Menu"));
        assert!(files.contains("[C]opy"));
        assert!(files.contains("[T]rash"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.menu.commands"),
            arguments: BTreeMap::new(),
        });
        let commands = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(commands.contains("Commands Menu"));
        assert!(commands.contains("[F]ind files"));
        let Some(Overlay::Menu(commands_menu)) = &workspace.overlay else {
            panic!("commands menu should remain open");
        };
        assert!(
            commands_menu
                .items()
                .iter()
                .any(|item| item.label == "&Terminal tabs")
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.menu.options"),
            arguments: BTreeMap::new(),
        });
        let options = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(options.contains("Options Menu"));
        assert!(options.contains("[S]ystem settings"));
        let Some(Overlay::Menu(options_menu)) = &workspace.overlay else {
            panic!("options menu should remain open");
        };
        assert!(
            options_menu
                .items()
                .iter()
                .any(|item| item.label == "Exte&nsions")
        );
    }

    #[test]
    fn file_menu_disables_unavailable_actions_with_registry_reasons() {
        let left = CollectionSurface::new(
            "empty.left",
            "workspace.panel",
            "Empty",
            Location::new("test://empty-left"),
            Vec::new(),
        );
        let right = CollectionSurface::new(
            "empty.right",
            "workspace.panel",
            "Empty peer",
            Location::new("test://empty-right"),
            Vec::new(),
        );
        let mut workspace = FarWorkspace::new(left, right);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F9"));
        workspace.handle_terminal_event(&mut keymap, key("Right"));
        let files = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(files.contains("Files Menu"));
        assert!(files.contains("no current resource"));
        workspace.handle_terminal_event(&mut keymap, key("v"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "overlay.menu");
        assert_eq!(
            workspace.status,
            "command near.resource.view is unavailable: no current resource"
        );
    }

    #[test]
    fn file_menu_only_enables_view_and_edit_when_the_current_resource_can_complete_them() {
        let root =
            std::env::temp_dir().join(format!("near-fm-menu-applicability-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("folder")).unwrap();
        fs::write(root.join("document.txt"), b"menu-action-content").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &root, "applicability.left");
        let right = filesystem_collection(provider.as_ref(), &root, "applicability.right");
        let folder = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "folder")
            .unwrap();
        left.set_cursor(folder);
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                root.join(".trash"),
                OperationJournal::memory(),
            ));

        let directory_menu = workspace.files_menu();
        let view = directory_menu
            .items()
            .iter()
            .find(|item| item.command.id.as_str() == "near.resource.view")
            .unwrap();
        let edit = directory_menu
            .items()
            .iter()
            .find(|item| item.command.id.as_str() == "near.resource.edit")
            .unwrap();
        assert!(!view.enabled);
        assert!(view.description.contains("non-container resource"));
        assert!(!edit.enabled);
        assert!(edit.description.contains("requires a file"));

        workspace.overlay = Some(Overlay::Menu(directory_menu));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.handle_terminal_event(&mut keymap, key("v"));
        assert_eq!(
            workspace.status,
            "The internal viewer requires a non-container resource"
        );
        assert_eq!(workspace.active_contexts()[0].as_str(), "overlay.menu");
        workspace.overlay = None;

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.view".into(),
            arguments: BTreeMap::new(),
        });
        assert_eq!(
            workspace.status,
            "The internal viewer requires a non-container resource"
        );
        assert!(workspace.overlay.is_none());

        let document = workspace
            .focused_panel()
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "document.txt")
            .unwrap();
        workspace.focused_panel_mut().set_cursor(document);
        let file_menu = workspace.files_menu();
        assert!(
            file_menu
                .items()
                .iter()
                .find(|item| item.command.id.as_str() == "near.resource.view")
                .unwrap()
                .enabled
        );
        assert!(
            file_menu
                .items()
                .iter()
                .find(|item| item.command.id.as_str() == "near.resource.edit")
                .unwrap()
                .enabled
        );

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.view".into(),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.viewer");
        workspace.dispatch(&CommandInvocation {
            id: "near.overlay.cancel".into(),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.edit".into(),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.editor");
        workspace.overlay = None;

        let folder = workspace
            .focused_panel()
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "folder")
            .unwrap();
        workspace.focused_panel_mut().set_cursor(folder);
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.open".into(),
            arguments: BTreeMap::new(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        let parent = workspace
            .focused_panel()
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "..")
            .unwrap();
        workspace.focused_panel_mut().set_cursor(parent);
        let parent_menu = workspace.files_menu();
        for command in [
            "near.resource.view",
            "near.resource.edit",
            "near.resource.copy-to-peer",
            "near.resource.move-to-peer",
            "near.resource.rename",
            "near.resource.link",
            "near.resource.attributes",
            "near.archive.create",
            "near.resource.trash",
            "near.resource.delete",
            "near.resource.wipe",
            "near.resource.description",
        ] {
            let item = parent_menu
                .items()
                .iter()
                .find(|item| item.command.id.as_str() == command)
                .unwrap();
            assert!(!item.enabled, "{command} must not be enabled for ..");
            assert!(
                item.description.contains("navigation-only")
                    || (command == "near.archive.create"
                        && item.description.contains("writable archive format")),
                "{command}: {}",
                item.description
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_static_menu_action_activates_to_an_effect_or_explicit_denial() {
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        for menu_kind in 0..6 {
            let catalog = FarWorkspace::demo();
            let menu = match menu_kind {
                0 => catalog.main_menu(),
                1 => catalog.panel_menu(FocusedPanel::Left),
                2 => catalog.panel_menu(FocusedPanel::Right),
                3 => catalog.files_menu(),
                4 => catalog.commands_menu(),
                _ => catalog.options_menu(),
            };
            for index in 0..menu.items().len() {
                let mut workspace = FarWorkspace::demo();
                let mut menu = match menu_kind {
                    0 => workspace.main_menu(),
                    1 => workspace.panel_menu(FocusedPanel::Left),
                    2 => workspace.panel_menu(FocusedPanel::Right),
                    3 => workspace.files_menu(),
                    4 => workspace.commands_menu(),
                    _ => workspace.options_menu(),
                };
                let item = menu.items()[index].clone();
                if !item.enabled {
                    assert!(
                        item.description.starts_with("Unavailable:"),
                        "disabled action {} has no visible prerequisite",
                        item.command.id
                    );
                }
                assert!(menu.select_visible_row(index));
                workspace.overlay = Some(Overlay::Menu(menu));
                let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
                let before = workspace.snapshot(&theme, &keymap, 100, 30);
                workspace.handle_terminal_event(&mut keymap, key("Enter"));
                let after = workspace.snapshot(&theme, &keymap, 100, 30);
                assert_ne!(
                    before, after,
                    "menu action {} produced neither its effect nor an explicit denial",
                    item.command.id
                );
            }
        }
    }

    #[test]
    fn static_menu_catalog_matches_the_versioned_action_manifest() {
        let catalog = FarWorkspace::demo();
        let actual = [
            catalog.main_menu(),
            catalog.panel_menu(FocusedPanel::Left),
            catalog.panel_menu(FocusedPanel::Right),
            catalog.files_menu(),
            catalog.commands_menu(),
            catalog.options_menu(),
        ]
        .into_iter()
        .flat_map(|menu| {
            menu.items()
                .iter()
                .map(|item| item.command.id.to_string())
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>();
        let manifest: toml::Value =
            toml::from_str(include_str!("../../../specs/menu-actions.toml")).unwrap();
        assert_eq!(manifest["schema_version"].as_integer(), Some(1));
        let expected = manifest["static_commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| command.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn main_menu_order_matches_the_versioned_far_layout() {
        let catalog = FarWorkspace::demo();
        assert_eq!(
            catalog
                .panel_menu(FocusedPanel::Left)
                .items()
                .iter()
                .take(5)
                .map(|item| item.label.trim_start_matches([' ', '√']).to_owned())
                .collect::<Vec<_>>(),
            ["&Brief", "&Medium", "&Full", "&Wide", "&Detailed"]
        );
        let manifest: toml::Value =
            toml::from_str(include_str!("../../../specs/menu-actions.toml")).unwrap();
        let ordered = manifest["ordered_commands"].as_table().unwrap();
        for (name, menu) in [
            ("main", catalog.main_menu()),
            ("left_right", catalog.panel_menu(FocusedPanel::Left)),
            ("files", catalog.files_menu()),
            ("commands", catalog.commands_menu()),
            ("options", catalog.options_menu()),
        ] {
            let actual = menu
                .items()
                .iter()
                .map(|item| item.command.id.as_str())
                .collect::<Vec<_>>();
            let expected = ordered[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|command| command.as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{name} menu order drifted");
        }
    }

    #[test]
    fn canonical_main_menus_have_unique_accelerators() {
        let catalog = FarWorkspace::demo();
        for menu in [
            catalog.panel_menu(FocusedPanel::Left),
            catalog.files_menu(),
            catalog.commands_menu(),
            catalog.options_menu(),
        ] {
            let mut accelerators = BTreeSet::new();
            for item in menu.items() {
                let Some(accelerator) = item.label.split_once('&').and_then(|(_, suffix)| {
                    suffix
                        .chars()
                        .next()
                        .map(|character| character.to_ascii_lowercase())
                }) else {
                    continue;
                };
                assert!(
                    accelerators.insert(accelerator),
                    "duplicate accelerator {accelerator:?} in {}",
                    menu.title()
                );
            }
        }
    }

    #[test]
    fn main_menu_dropdowns_anchor_and_clamp_to_terminal_width() {
        let mut workspace = FarWorkspace::demo();
        workspace.open_main_menu_category(MainMenuCategory::Left);
        let Some(Overlay::Menu(left)) = &workspace.overlay else {
            panic!("left menu should be open");
        };
        let left_rect = menu_popup(Rect::new(0, 0, 100, 30), left);
        assert_eq!(left_rect.x, 1);
        assert_eq!(left_rect.y, 1);
        assert!(left_rect.width <= 60);
        assert_eq!(
            left_rect.height,
            u16::try_from(left.items().len()).unwrap() + 2
        );

        workspace.open_main_menu_category(MainMenuCategory::Right);
        let Some(Overlay::Menu(right)) = &workspace.overlay else {
            panic!("right menu should be open");
        };
        let wide = menu_popup(Rect::new(0, 0, 100, 30), right);
        assert_eq!(wide.x, right.main_menu_column(0).unwrap());
        let narrow = menu_popup(Rect::new(0, 0, 40, 12), right);
        assert_eq!(narrow.y, 1);
        assert!(narrow.right() <= 40);
        assert!(narrow.bottom() <= 12);
    }

    #[test]
    fn fixed_nested_menu_actions_activate_to_an_effect_or_explicit_denial() {
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        for menu_kind in 0..4 {
            let catalog = FarWorkspace::demo();
            let menu = match menu_kind {
                0 => catalog.sort_menu(),
                1 => catalog.panel_mode_menu(),
                2 => catalog.location_menu(FocusedPanel::Left),
                _ => FarWorkspace::selection_menu(),
            };
            for index in 0..menu.items().len() {
                let mut workspace = FarWorkspace::demo();
                let mut menu = match menu_kind {
                    0 => workspace.sort_menu(),
                    1 => workspace.panel_mode_menu(),
                    2 => workspace.location_menu(FocusedPanel::Left),
                    _ => FarWorkspace::selection_menu(),
                };
                let item = menu.items()[index].clone();
                assert!(menu.select_visible_row(index));
                workspace.overlay = Some(Overlay::Menu(menu));
                let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
                let before = workspace.snapshot(&theme, &keymap, 100, 30);
                workspace.handle_terminal_event(&mut keymap, key("Enter"));
                let after = workspace.snapshot(&theme, &keymap, 100, 30);
                assert_ne!(
                    before, after,
                    "nested menu action {} produced neither its effect nor an explicit denial",
                    item.command.id
                );
            }
        }
    }

    #[test]
    fn plain_text_uses_the_command_line_before_global_sequences() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        assert_eq!(
            workspace.handle_terminal_event(&mut keymap, key("g")),
            WorkspaceAction::Noop
        );
        assert_eq!(workspace.command_line.buffer(), "g");
        assert!(workspace.pending_sequence.is_empty());
    }

    #[test]
    fn command_line_executes_full_screen_and_supports_history() {
        let mut workspace = FarWorkspace::demo()
            .with_embedded_pty(false)
            .with_command_line_executor(TestCommandLineExecutor);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Paste("printf current=".to_owned()),
        );
        assert!(matches!(
            workspace.handle_terminal_event(&mut keymap, key("Ctrl+Enter")),
            WorkspaceAction::Command(_)
        ));
        assert!(
            workspace
                .command_line
                .buffer()
                .starts_with("printf current=")
        );
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        wait_for_command_line(&mut workspace);

        let screen = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(screen.contains("executed printf current="));
        assert!(!screen.contains("Macintosh HD"));

        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+E"));
        assert!(
            workspace
                .command_line
                .buffer()
                .starts_with("printf current=")
        );
    }

    #[cfg(all(feature = "embedded-pty", unix))]
    #[test]
    fn embedded_command_line_and_user_screen_share_persistent_shell_state() {
        let root = std::env::temp_dir().join(format!(
            "near-shared-terminal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let child = root.join("persistent-child");
        fs::create_dir_all(&child).unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "terminal.left");
        let right = filesystem_collection(provider.as_ref(), &root, "terminal.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_command_line_argument_resolver(LocalCommandLineArgumentResolver);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace
            .command_line
            .set_buffer("cd persistent-child && printf 'entered-child\\n'");
        workspace.submit_command_line();
        assert!(workspace.terminal_is_full_screen());
        assert!(workspace.overlay.is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let screen = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
            if screen.contains("entered-child") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "terminal output did not arrive: {screen}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        workspace.toggle_terminal_screen();
        assert!(!workspace.terminal_is_full_screen());
        workspace.command_line.set_buffer(format!(
            "test \"$PWD\" = {} && printf 'cwd-preserved\\n'",
            super::shell_quote(child.to_string_lossy().as_ref())
        ));
        workspace.submit_command_line();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let screen = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
            if screen.contains("cwd-preserved") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "persistent cwd was not rendered: {screen}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(workspace.overlay.is_none());
        assert!(workspace.command_line_task.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_command_expands_each_selected_resource_and_reports_source_failures() {
        let root = std::env::temp_dir().join(format!("near-apply-command-{}", std::process::id()));
        let resolver = LocalCommandLineArgumentResolver;
        let entries = ["good file.txt", "bad file.txt"]
            .into_iter()
            .map(|name| CollectionEntry {
                resource: ResourceRef {
                    provider: ProviderId::from("near.local-fs"),
                    location: LocalFileProvider::location(&root.join(name)),
                },
                metadata: ResourceMetadata {
                    name: name.to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                },
                details: "file".to_owned(),
                selected: true,
            })
            .collect();
        let left = CollectionSurface::new(
            "apply.left",
            "workspace.panel",
            "Apply",
            LocalFileProvider::location(&root),
            entries,
        );
        let right = CollectionSurface::new(
            "apply.right",
            "workspace.panel",
            "Peer",
            LocalFileProvider::location(&root.join("peer")),
            Vec::new(),
        );
        let executor = RecordingCommandExecutor {
            commands: Arc::default(),
            fail_when_contains: Some("bad file.txt".to_owned()),
        };
        let recorded = Arc::clone(&executor.commands);
        let mut workspace = FarWorkspace::new(left, right)
            .with_command_line_executor(executor)
            .with_command_line_argument_resolver(resolver);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+G"));
        let dialog = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(dialog.contains("Apply command"));
        assert!(dialog.contains("{resource}"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.operation.apply-command-confirmed"),
            arguments: BTreeMap::from([
                (
                    "template".to_owned(),
                    CommandValue::String("tool --input {resource} --name {name}".to_owned()),
                ),
                (
                    "mode".to_owned(),
                    CommandValue::String("sequential".to_owned()),
                ),
                (
                    "continue_on_error".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
            ]),
        });
        wait_for_apply_command(&mut workspace);

        let commands = recorded.lock().unwrap().clone();
        assert_eq!(commands.len(), 2);
        assert!(commands[0].contains(&resolver.quote_text("good file.txt")));
        assert!(commands[1].contains(&resolver.quote_text("bad file.txt")));
        let results = workspace.snapshot(&theme, &keymap, 120, 40).join("\n");
        assert!(results.contains("[OK] good file.txt"));
        assert!(results.contains("[FAILED] bad file.txt"));
        assert!(results.contains("intentional failure"));
        assert!(workspace.status.contains("1 succeeded, 1 failed"));
        assert!(
            workspace
                .task_records
                .values()
                .any(|record| record.state == TaskState::Failed)
        );
    }

    #[test]
    fn apply_command_batch_mode_executes_one_structured_argument_list() {
        let root = std::env::temp_dir().join(format!("near-apply-batch-{}", std::process::id()));
        let resolver = LocalCommandLineArgumentResolver;
        let entries = ["one item.txt", "two item.txt"]
            .into_iter()
            .map(|name| CollectionEntry {
                resource: ResourceRef {
                    provider: ProviderId::from("near.local-fs"),
                    location: LocalFileProvider::location(&root.join(name)),
                },
                metadata: ResourceMetadata {
                    name: name.to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                },
                details: "file".to_owned(),
                selected: true,
            })
            .collect();
        let executor = RecordingCommandExecutor::default();
        let recorded = Arc::clone(&executor.commands);
        let mut workspace = FarWorkspace::new(
            CollectionSurface::new(
                "batch.left",
                "workspace.panel",
                "Batch",
                LocalFileProvider::location(&root),
                entries,
            ),
            CollectionSurface::new(
                "batch.right",
                "workspace.panel",
                "Peer",
                LocalFileProvider::location(&root.join("peer")),
                Vec::new(),
            ),
        )
        .with_command_line_executor(executor)
        .with_command_line_argument_resolver(resolver);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.operation.apply-command-confirmed"),
            arguments: BTreeMap::from([
                (
                    "template".to_owned(),
                    CommandValue::String("tool {resources}".to_owned()),
                ),
                ("mode".to_owned(), CommandValue::String("batch".to_owned())),
                (
                    "continue_on_error".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
            ]),
        });
        wait_for_apply_command(&mut workspace);

        let commands = recorded.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert!(
            commands[0].contains(&resolver.quote_text(root.join("one item.txt").to_str().unwrap()))
        );
        assert!(
            commands[0].contains(&resolver.quote_text(root.join("two item.txt").to_str().unwrap()))
        );
        assert!(workspace.status.contains("1 succeeded, 0 failed"));
    }

    #[test]
    fn persistent_command_history_filters_locks_and_restores_entries() {
        let store = TestCommandHistoryStore::default();
        let mut locked = CommandHistoryEntry::new("git status");
        locked.locked = true;
        store
            .entries
            .lock()
            .unwrap()
            .extend([CommandHistoryEntry::new("pwd"), locked]);
        let mut workspace = FarWorkspace::demo()
            .with_command_line_executor(TestCommandLineExecutor)
            .with_command_history_store(store.clone());
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Alt+F8"));
        assert_eq!(
            workspace.active_contexts()[0].as_str(),
            "surface.command-history"
        );
        for key_name in ["g", "i", "t"] {
            workspace.handle_terminal_event(&mut keymap, key(key_name));
        }
        let history = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(history.contains("git status"));
        assert!(!history.contains("pwd"));
        assert!(history.contains("Find: git"));

        workspace.handle_terminal_event(&mut keymap, key("Space"));
        assert!(!store.entries.lock().unwrap()[1].locked);
        workspace.handle_terminal_event(&mut keymap, key("Space"));
        assert!(store.entries.lock().unwrap()[1].locked);
        workspace.handle_terminal_event(&mut keymap, key("Delete"));
        assert_eq!(store.entries.lock().unwrap().len(), 1);
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(workspace.overlay.is_none());
        assert_eq!(workspace.command_line.buffer(), "git status");

        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        wait_for_command_line(&mut workspace);
        let entries = store.entries.lock().unwrap();
        assert_eq!(entries.last().unwrap().command, "git status");
        assert_eq!(entries.last().unwrap().use_count, 2);
    }

    #[test]
    fn viewed_and_edited_histories_persist_filter_lock_clear_and_reopen() {
        let root =
            std::env::temp_dir().join(format!("near-resource-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("history-note.txt");
        fs::write(&file, "history content").unwrap();
        let resource = ResourceRef {
            provider: ProviderId::from("near.local-fs"),
            location: LocalFileProvider::location(&file),
        };
        let panel = CollectionSurface::new(
            "history.left",
            "workspace.panel",
            "History",
            LocalFileProvider::location(&root),
            vec![CollectionEntry {
                resource: resource.clone(),
                metadata: ResourceMetadata {
                    name: "history-note.txt".to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                },
                details: "file".to_owned(),
                selected: false,
            }],
        );
        let store = TestResourceHistoryStore::default();
        let mut workspace = FarWorkspace::new(
            panel,
            CollectionSurface::new(
                "history.right",
                "workspace.panel",
                "Peer",
                LocalFileProvider::location(&root),
                Vec::new(),
            ),
        )
        .with_provider(Arc::new(LocalFileProvider))
        .with_resource_history_store(store.clone());
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F3"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        workspace.handle_terminal_event(&mut keymap, key("F4"));
        workspace.handle_terminal_event(&mut keymap, key("Alt+F11"));
        let chooser = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(chooser.contains("Viewed files"));
        assert!(chooser.contains("Edited files"));
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert_eq!(
            workspace.active_contexts()[0].as_str(),
            "surface.resource-history"
        );
        for name in ["h", "i", "s", "t"] {
            workspace.handle_terminal_event(&mut keymap, key(name));
        }
        let edit_history = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(edit_history.contains("history-note.txt"));
        assert!(edit_history.contains("Find: hist"));
        workspace.handle_terminal_event(&mut keymap, key("Space"));
        workspace.handle_terminal_event(&mut keymap, key("Delete"));
        assert_eq!(store.state.lock().unwrap().edited.len(), 1);
        assert!(store.state.lock().unwrap().edited[0].locked);
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(workspace.active_editor.is_some());

        workspace.active_editor = None;
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+F3"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(matches!(workspace.overlay, Some(Overlay::Surface(_))));

        let unavailable = ResourceHistoryEntry::new(
            ResourceRef {
                provider: ProviderId::from("near.missing"),
                location: Location::new("missing:///history.txt"),
            },
            "Unavailable history",
        );
        store.state.lock().unwrap().viewed.push(unavailable);
        let mut restarted = FarWorkspace::demo().with_resource_history_store(store.clone());
        restarted.handle_terminal_event(&mut keymap, key("Ctrl+Shift+F3"));
        restarted.handle_terminal_event(&mut keymap, key("Enter"));
        let unavailable_history = restarted.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(unavailable_history.contains("Provider near.missing is unavailable"));
        assert_eq!(store.state.lock().unwrap().viewed.len(), 2);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn command_line_inserts_quoted_names_and_native_panel_paths() {
        let root = std::env::temp_dir().join(format!("near command paths {}", std::process::id()));
        let current_path = root.join("two words.txt");
        let second_path = root.join("quote's.rs");
        let peer_root = root.join("peer folder");
        let peer_path = peer_root.join("peer item.txt");
        let left = CollectionSurface::new(
            "paths.left",
            "workspace.panel",
            "Paths",
            LocalFileProvider::location(&root),
            vec![
                CollectionEntry {
                    resource: ResourceRef {
                        provider: ProviderId::from("near.local-fs"),
                        location: LocalFileProvider::location(&current_path),
                    },
                    metadata: ResourceMetadata {
                        name: "two words.txt".to_owned(),
                        kind: ResourceKind::File,
                        ..ResourceMetadata::default()
                    },
                    details: "file".to_owned(),
                    selected: true,
                },
                CollectionEntry {
                    resource: ResourceRef {
                        provider: ProviderId::from("near.local-fs"),
                        location: LocalFileProvider::location(&second_path),
                    },
                    metadata: ResourceMetadata {
                        name: "quote's.rs".to_owned(),
                        kind: ResourceKind::File,
                        ..ResourceMetadata::default()
                    },
                    details: "file".to_owned(),
                    selected: true,
                },
            ],
        );
        let right = CollectionSurface::new(
            "paths.right",
            "workspace.panel",
            "Peer",
            LocalFileProvider::location(&peer_root),
            vec![CollectionEntry {
                resource: ResourceRef {
                    provider: ProviderId::from("near.local-fs"),
                    location: LocalFileProvider::location(&peer_path),
                },
                metadata: ResourceMetadata {
                    name: "peer item.txt".to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                },
                details: "file".to_owned(),
                selected: false,
            }],
        );
        let resolver = LocalCommandLineArgumentResolver;
        let mut workspace =
            FarWorkspace::new(left, right).with_command_line_argument_resolver(resolver);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+Enter"));
        assert_eq!(
            workspace.command_line.buffer(),
            format!(
                "{} {}",
                resolver.quote_text("two words.txt"),
                resolver.quote_text("quote's.rs")
            )
        );
        workspace.command_line.clear();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+["));
        assert_eq!(
            workspace.command_line.buffer(),
            resolver.quote_text(root.to_str().unwrap())
        );
        workspace.command_line.clear();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+]"));
        assert_eq!(
            workspace.command_line.buffer(),
            resolver.quote_text(peer_root.to_str().unwrap())
        );
        workspace.command_line.clear();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+["));
        assert_eq!(
            workspace.command_line.buffer(),
            resolver.quote_text(current_path.to_str().unwrap())
        );
        workspace.command_line.clear();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+]"));
        assert_eq!(
            workspace.command_line.buffer(),
            resolver.quote_text(peer_path.to_str().unwrap())
        );
    }

    #[test]
    fn command_line_path_insertion_rejects_non_native_provider_locations() {
        let mut workspace = FarWorkspace::demo()
            .with_command_line_argument_resolver(LocalCommandLineArgumentResolver);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+["));

        assert!(workspace.command_line.buffer().is_empty());
        assert!(workspace.status.contains("no native command-line path"));
    }

    #[test]
    fn ten_folder_shortcuts_and_searchable_history_persist_and_navigate() {
        let root = std::env::temp_dir()
            .join(format!("near-folder-history-{}", std::process::id()))
            .join("parent-segment-that-forces-the-full-provider-location-outside-the-dialog-width");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("child-place")).unwrap();
        let root_location = LocalFileProvider::location(&root);
        let child_location = LocalFileProvider::location(&root.join("child-place"));
        let provider: Arc<dyn ResourceProvider> = Arc::new(LocalFileProvider);
        let store = TestFolderNavigationStore::default();
        let mut workspace = FarWorkspace::new(
            CollectionSurface::new(
                "folder.left",
                "workspace.panel",
                "Folders",
                root_location.clone(),
                Vec::new(),
            ),
            CollectionSurface::new(
                "folder.right",
                "workspace.panel",
                "Peer",
                root_location.clone(),
                Vec::new(),
            ),
        )
        .with_provider(Arc::clone(&provider))
        .with_folder_navigation_store(store.clone());
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.start_listing(FocusedPanel::Left, &provider, &root_location);
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        for slot in 0..10 {
            workspace.dispatch(&CommandInvocation {
                id: CommandId::from("near.location.shortcut-assign"),
                arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(slot))]),
            });
        }
        assert!(
            store
                .state
                .lock()
                .unwrap()
                .shortcuts
                .iter()
                .all(Option::is_some)
        );

        workspace.start_listing(FocusedPanel::Left, &provider, &child_location);
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        workspace.handle_terminal_event(&mut keymap, key("Alt+F12"));
        for key_name in ["c", "h", "i", "l", "d"] {
            workspace.handle_terminal_event(&mut keymap, key(key_name));
        }
        let history = workspace.snapshot(&theme, &keymap, 110, 32).join("\n");
        assert!(history.contains("child-place"));
        assert!(history.contains("Find: child"));
        workspace.handle_terminal_event(&mut keymap, key("Space"));
        workspace.handle_terminal_event(&mut keymap, key("Delete"));
        assert_eq!(store.state.lock().unwrap().history.len(), 1);
        assert!(store.state.lock().unwrap().history[0].locked);
        workspace.handle_terminal_event(&mut keymap, key("Esc"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.location.shortcut-open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(1))]),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.focused_panel().location(), &root_location);
        assert!(store.state.lock().unwrap().history.len() >= 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_folder_shortcuts_remain_inspectable_with_errors() {
        let store = TestFolderNavigationStore::default();
        let unavailable = FolderLocationEntry::new(
            ProviderId::from("near.missing"),
            Location::new("missing:///folder"),
            "Unavailable folder",
        );
        {
            let mut state = store.state.lock().unwrap();
            state.history.push(unavailable.clone());
            state.shortcuts = vec![None; 10];
            state.shortcuts[3] = Some(unavailable);
        }
        let mut workspace = FarWorkspace::demo().with_folder_navigation_store(store.clone());
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.location.shortcut-open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(3))]),
        });
        assert_eq!(workspace.status, "Provider near.missing is unavailable");
        assert!(
            store.state.lock().unwrap().shortcuts[3]
                .as_ref()
                .unwrap()
                .last_error
                .is_some()
        );

        workspace.handle_terminal_event(&mut keymap, key("Alt+F12"));
        let history = workspace.snapshot(&theme, &keymap, 110, 32).join("\n");
        assert!(history.contains("Unavailable folder"));
        assert!(history.contains("Provider near.missing is unavailable"));
    }

    #[test]
    fn every_shipped_binding_resolves_to_a_registered_command() {
        let workspace = FarWorkspace::demo();
        let document: toml::Value = toml::from_str(KEYMAP).unwrap();
        let mut command_ids = BTreeSet::new();
        for context in document["context"].as_array().unwrap() {
            let Some(bindings) = context.get("bindings").and_then(toml::Value::as_array) else {
                continue;
            };
            for binding in bindings {
                let run = &binding["run"];
                let command = run
                    .as_str()
                    .or_else(|| run.get("command")?.as_str())
                    .unwrap();
                command_ids.insert(CommandId::from(command));
            }
        }
        for command_id in command_ids {
            assert!(
                workspace.registry.get(&command_id).is_some(),
                "unregistered keymap command: {command_id}"
            );
        }
    }

    #[test]
    fn provider_disconnect_and_retry_preserve_panel_location_and_refresh_state() {
        let disconnected = Arc::new(AtomicBool::new(false));
        let reconnected = Arc::new(AtomicBool::new(false));
        let provider = Arc::new(TestConnectionProvider {
            disconnected: Arc::clone(&disconnected),
            reconnected: Arc::clone(&reconnected),
        });
        let location = Location::new("test-remote://prod/root");
        let entry = CollectionEntry::new(
            ResourceRef {
                provider: provider.id(),
                location: Location::new("test-remote://prod/root/remote.txt"),
            },
            ResourceMetadata {
                name: "remote.txt".to_owned(),
                kind: ResourceKind::File,
                ..ResourceMetadata::default()
            },
            "remote",
        );
        let left = CollectionSurface::new(
            "connection.left",
            "workspace.panel",
            "Remote",
            location.clone(),
            vec![entry],
        );
        let right = demo_collection("connection.right", "Right", "/right", Vec::new(), 0);
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);

        workspace.dispatch(&CommandInvocation {
            id: "near.provider.disconnect".into(),
            arguments: BTreeMap::new(),
        });
        assert!(disconnected.load(Ordering::Relaxed));
        assert_eq!(workspace.focused_panel().location(), &location);
        assert_eq!(workspace.focused_panel().entries().len(), 1);
        assert!(workspace.status.contains("panel state retained"));

        workspace.dispatch(&CommandInvocation {
            id: "near.provider.retry".into(),
            arguments: BTreeMap::new(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(reconnected.load(Ordering::Relaxed));
        assert_eq!(workspace.focused_panel().location(), &location);
        assert_eq!(
            workspace.focused_panel().entries()[0].metadata.name,
            "remote.txt"
        );
    }

    #[test]
    fn filesystem_provider_drives_navigation_parent_and_view_workflows() {
        let root = std::env::temp_dir().join(format!("near-fm-workflow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("child")).unwrap();
        fs::write(root.join("child/nested.txt"), b"nested-content").unwrap();
        fs::write(root.join("root.txt"), b"root-content").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let location = LocalFileProvider::location(&root);
        let page = block_on_provider(provider.list(
            &location,
            ListRequest {
                generation: ListingGeneration(1),
                continuation: None,
                page_size: 100,
                cancellation: CancellationToken::default(),
            },
        ))
        .unwrap();
        let entries = page
            .entries
            .into_iter()
            .map(|entry| CollectionEntry {
                resource: entry.resource,
                metadata: entry.metadata,
                details: entry.details,
                selected: false,
            })
            .collect::<Vec<_>>();
        let child = entries
            .iter()
            .position(|entry| entry.metadata.name == "child")
            .expect("child directory should be listed");
        let left = CollectionSurface::new(
            "test.left",
            "workspace.panel",
            "Fixture",
            location.clone(),
            entries.clone(),
        )
        .with_cursor(child);
        let right = CollectionSurface::new(
            "test.right",
            "workspace.panel",
            "Fixture peer",
            location,
            entries,
        );
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let current = workspace.focused_panel().current().unwrap();
        assert_eq!(current.metadata.name, "child");
        assert_eq!(current.metadata.kind, ResourceKind::Directory);
        assert_eq!(
            LocalFileProvider::path(&current.resource.location).unwrap(),
            root.join("child")
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.open"),
            arguments: BTreeMap::default(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(
            LocalFileProvider::path(workspace.focused_panel().location()).unwrap(),
            root.join("child"),
            "navigation status: {}",
            workspace.status
        );
        assert_eq!(workspace.focused_panel().entries()[0].metadata.name, "..");
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.collection.toggle-selection"),
            arguments: BTreeMap::default(),
        });
        assert!(!workspace.focused_panel().entries()[0].selected);
        assert_eq!(workspace.status, "The parent entry cannot be selected");
        workspace.handle_terminal_event(&mut keymap, key("Shift+Down"));
        assert!(!workspace.focused_panel().entries()[0].selected);
        assert_eq!(workspace.focused_panel().cursor(), 1);
        workspace.handle_terminal_event(&mut keymap, key("Left"));
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.trash"),
            arguments: BTreeMap::default(),
        });
        assert_eq!(workspace.status, "The parent entry is navigation-only");
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.open"),
            arguments: BTreeMap::default(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(
            workspace.focused_panel().location(),
            &LocalFileProvider::location(&root)
        );
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.collection.move"),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(2))]),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.view"),
            arguments: BTreeMap::default(),
        });
        let screen = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(screen.contains("root-content"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        workspace.handle_terminal_event(&mut keymap, key("F4"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.editor");
        let editor = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(editor.contains("root-content"));
        workspace.handle_terminal_event(&mut keymap, key("End"));
        for character in "-edited".chars() {
            workspace.handle_terminal_event(&mut keymap, key(&character.to_string()));
        }
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+S"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        assert_eq!(
            fs::read_to_string(root.join("root.txt")).unwrap(),
            "root-content-edited"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn panel_interaction_conformance_refresh_retains_surviving_cursor_and_selection() {
        let root =
            std::env::temp_dir().join(format!("near-fm-refresh-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for name in [
            "alpha.txt",
            "beta.txt",
            "gamma-very-long-resource-name-for-horizontal-retention.txt",
        ] {
            fs::write(root.join(name), name).unwrap();
        }
        let provider: Arc<dyn ResourceProvider> = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "refresh.left");
        let right = filesystem_collection(provider.as_ref(), &root, "refresh.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(Arc::clone(&provider));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        let alpha = workspace
            .left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "alpha.txt")
            .unwrap();
        workspace.left.set_cursor(alpha);
        workspace.left.toggle_selection();
        let gamma = workspace
            .left
            .entries()
            .iter()
            .position(|entry| {
                entry.metadata.name == "gamma-very-long-resource-name-for-horizontal-retention.txt"
            })
            .unwrap();
        workspace.left.set_cursor(gamma);
        workspace.left.toggle_selection();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        workspace.snapshot(&theme, &keymap, 52, 12);
        workspace.handle_terminal_event(&mut keymap, key("Alt+End"));
        let expected_current = workspace.left.current().unwrap().resource.clone();
        let expected_selection = workspace.left.selected_resources();
        let expected_horizontal_offset = workspace.left.horizontal_offset();
        assert!(expected_horizontal_offset > 0);

        fs::write(root.join("delta.txt"), b"delta").unwrap();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+R"));
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        wait_for_listing(&mut workspace, FocusedPanel::Right);

        assert_eq!(
            workspace.left.current().map(|entry| &entry.resource),
            Some(&expected_current)
        );
        assert_eq!(workspace.left.selected_resources(), expected_selection);
        workspace.snapshot(&theme, &keymap, 52, 12);
        assert_eq!(
            workspace.left.horizontal_offset(),
            expected_horizontal_offset
        );
        assert!(
            workspace
                .left
                .entries()
                .iter()
                .any(|entry| entry.metadata.name == "delta.txt")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archives_open_as_panels_extract_and_create_through_workspace_commands() {
        let root =
            std::env::temp_dir().join(format!("near-fm-archive-workflow-{}", std::process::id()));
        let source = root.join("source");
        let destination = root.join("destination");
        let trash = root.join("trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let archive_path = source.join("sample.zip");
        zip_fixture(&archive_path, &[("inside.txt", b"inside")]);
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &source, "archive.source");
        let right = filesystem_collection(provider.as_ref(), &destination, "archive.destination");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_provider(Arc::new(ZipArchiveProvider))
            .with_operation_service(ArchiveOperationService::new(
                LocalOperationService::new(trash, OperationJournal::memory()),
                OperationJournal::memory(),
            ));
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();
        assert!(
            workspace
                .action_context()
                .capabilities
                .contains(&CapabilityId::from("archive.create"))
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.open"),
            arguments: BTreeMap::new(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(
            workspace
                .left
                .location()
                .as_str()
                .starts_with("archive://zip/")
        );
        assert!(
            workspace
                .action_context()
                .capabilities
                .contains(&CapabilityId::from("archive.update"))
        );
        assert_eq!(workspace.left.current().unwrap().metadata.name, "..");
        workspace.left.set_cursor(1);
        assert_eq!(
            workspace.left.current().unwrap().metadata.name,
            "inside.txt"
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.copy-to-peer"),
            arguments: BTreeMap::new(),
        });
        let preview = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(preview.contains("operation: Copy"));
        assert!(preview.contains("near.archive.operation"));
        workspace.handle_terminal_event(&mut Keymap::from_toml(KEYMAP).unwrap(), key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        assert_eq!(fs::read(destination.join("inside.txt")).unwrap(), b"inside");

        workspace.focused = FocusedPanel::Right;
        assert!(
            workspace
                .action_context()
                .peer_capabilities
                .contains(&CapabilityId::from("archive.update"))
        );
        workspace.focused = FocusedPanel::Left;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.location.parent"),
            arguments: BTreeMap::new(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(
            workspace.left.location(),
            &LocalFileProvider::location(&source)
        );

        workspace.focused = FocusedPanel::Right;
        workspace.refresh_collections();
        wait_for_listing(&mut workspace, FocusedPanel::Right);
        let inside = workspace
            .right
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "inside.txt")
            .unwrap();
        workspace.right.set_cursor(inside);
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.archive.create-confirmed"),
            arguments: BTreeMap::from([(
                "name".to_owned(),
                CommandValue::String("created.zip".to_owned()),
            )]),
        });
        let preview = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(preview.contains("operation: Copy"));
        workspace.handle_terminal_event(&mut Keymap::from_toml(KEYMAP).unwrap(), key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        assert!(destination.join("created.zip").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editor_screens_retain_documents_and_restore_closed_positions() {
        let root =
            std::env::temp_dir().join(format!("near-fm-editor-screens-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"one\ntwo\nthree").unwrap();
        fs::write(root.join("b.txt"), b"second").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let position_store = LocalEditorPositionStore::new(root.join("positions.toml"));
        let left = filesystem_collection(provider.as_ref(), &root, "editor.left");
        let right = filesystem_collection(provider.as_ref(), &root, "editor.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider.clone())
            .with_editor_position_store(position_store.clone());
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F4"));
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        assert_eq!(workspace.active_editor().unwrap().title(), "a.txt");
        assert_eq!(workspace.active_editor().unwrap().position().row, 1);

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Tab"));
        assert!(workspace.active_editor.is_none());
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        workspace.handle_terminal_event(&mut keymap, key("F4"));
        assert_eq!(workspace.editors.len(), 2);
        assert_eq!(workspace.active_editor().unwrap().title(), "b.txt");

        workspace.handle_terminal_event(&mut keymap, key("F12"));
        let screens = workspace.snapshot(&theme, &keymap, 110, 32).join("\n");
        assert!(screens.contains("Panels"));
        assert!(screens.contains("Editor: a.txt"));
        assert!(screens.contains("Editor: b.txt"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(workspace.active_editor.is_none());

        workspace.handle_terminal_event(&mut keymap, key("F12"));
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert_eq!(workspace.active_editor().unwrap().title(), "a.txt");

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Tab"));
        assert_eq!(workspace.active_editor().unwrap().title(), "b.txt");
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+Tab"));
        assert_eq!(workspace.active_editor().unwrap().title(), "a.txt");
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        assert_eq!(workspace.editors.len(), 1);
        assert!(workspace.active_editor.is_none());

        workspace.handle_terminal_event(&mut keymap, key("Up"));
        workspace.handle_terminal_event(&mut keymap, key("F4"));
        assert_eq!(workspace.active_editor().unwrap().title(), "a.txt");
        assert_eq!(workspace.active_editor().unwrap().position().row, 1);

        let left = filesystem_collection(provider.as_ref(), &root, "editor.restart-left");
        let right = filesystem_collection(provider.as_ref(), &root, "editor.restart-right");
        let mut restarted = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_editor_position_store(position_store);
        restarted.handle_terminal_event(&mut keymap, key("F4"));
        assert_eq!(restarted.active_editor().unwrap().title(), "a.txt");
        assert_eq!(restarted.active_editor().unwrap().position().row, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editor_save_as_and_external_change_choices_run_end_to_end() {
        let root =
            std::env::temp_dir().join(format!("near-fm-editor-save-as-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("document.txt"), b"original\ntext").unwrap();
        let saved = root.join("saved.txt");
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "save-as.left");
        let right = filesystem_collection(provider.as_ref(), &root, "save-as.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F4"));
        for character in "local ".chars() {
            workspace.handle_terminal_event(&mut keymap, key(&character.to_string()));
        }
        workspace.dispatch(&CommandInvocation {
            id: "near.editor.save-as-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "provider".to_owned(),
                    CommandValue::String("near.local-fs".to_owned()),
                ),
                (
                    "location".to_owned(),
                    CommandValue::String(LocalFileProvider::location(&saved).as_str().to_owned()),
                ),
                (
                    "encoding".to_owned(),
                    CommandValue::String("UTF-16LE".to_owned()),
                ),
                ("bom".to_owned(), CommandValue::String("yes".to_owned())),
                ("eol".to_owned(), CommandValue::String("CRLF".to_owned())),
                ("replace".to_owned(), CommandValue::String("yes".to_owned())),
                ("lossy".to_owned(), CommandValue::String("no".to_owned())),
            ]),
        });
        let bytes = fs::read(&saved).unwrap();
        assert!(bytes.starts_with(&[0xff, 0xfe]));
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16(&units).unwrap(),
            "local original\r\ntext"
        );

        fs::write(&saved, b"external\ntext").unwrap();
        workspace.handle_terminal_event(&mut keymap, key("F2"));
        let conflict = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(conflict.contains("Resource Changed Externally"));

        workspace.dispatch(&CommandInvocation {
            id: "near.editor.external-compare".into(),
            arguments: BTreeMap::new(),
        });
        let comparison = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(comparison.contains("-external"));
        assert!(comparison.contains("+local original"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));

        workspace.dispatch(&CommandInvocation {
            id: "near.editor.external-change".into(),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation {
            id: "near.editor.external-reload".into(),
            arguments: BTreeMap::new(),
        });
        let reloaded = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(reloaded.contains("external"));

        workspace.handle_terminal_event(&mut keymap, key("End"));
        for character in "-local".chars() {
            workspace.handle_terminal_event(&mut keymap, key(&character.to_string()));
        }
        fs::write(&saved, b"changed elsewhere").unwrap();
        workspace.handle_terminal_event(&mut keymap, key("F2"));
        workspace.dispatch(&CommandInvocation {
            id: "near.editor.external-keep-local".into(),
            arguments: BTreeMap::new(),
        });
        assert_eq!(fs::read_to_string(&saved).unwrap(), "external-local\ntext");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editor_save_as_refuses_lossy_output_until_confirmed() {
        let root =
            std::env::temp_dir().join(format!("near-fm-editor-lossy-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("document.txt"), "snowman ☃").unwrap();
        let target = root.join("latin1.txt");
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "lossy.left");
        let right = filesystem_collection(provider.as_ref(), &root, "lossy.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.handle_terminal_event(&mut keymap, key("F4"));
        let invocation = |lossy: &str| CommandInvocation {
            id: "near.editor.save-as-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "provider".to_owned(),
                    CommandValue::String("near.local-fs".to_owned()),
                ),
                (
                    "location".to_owned(),
                    CommandValue::String(LocalFileProvider::location(&target).as_str().to_owned()),
                ),
                (
                    "encoding".to_owned(),
                    CommandValue::String("Latin-1".to_owned()),
                ),
                ("bom".to_owned(), CommandValue::String("no".to_owned())),
                ("eol".to_owned(), CommandValue::String("LF".to_owned())),
                ("replace".to_owned(), CommandValue::String("yes".to_owned())),
                ("lossy".to_owned(), CommandValue::String(lossy.to_owned())),
            ]),
        };

        workspace.dispatch(&invocation("no"));
        assert!(!target.exists());
        assert!(workspace.status.contains("Allow lossy"));
        workspace.dispatch(&invocation("yes"));
        assert_eq!(fs::read(&target).unwrap(), b"snowman ?");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dirty_editor_sessions_block_application_quit() {
        let root =
            std::env::temp_dir().join(format!("near-fm-dirty-editor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("document.txt"), b"content").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "dirty.left");
        let right = filesystem_collection(provider.as_ref(), &root, "dirty.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F4"));
        workspace.handle_terminal_event(&mut keymap, key("x"));
        workspace.dispatch(&CommandInvocation {
            id: "near.app.quit".into(),
            arguments: BTreeMap::new(),
        });

        assert!(!workspace.should_quit());
        let Some(Overlay::Message { title, body }) = &workspace.overlay else {
            panic!("dirty editors should produce a quit-blocking message");
        };
        assert_eq!(title, "Unsaved Editors");
        assert!(body.contains("document.txt"));
        assert_eq!(
            fs::read_to_string(root.join("document.txt")).unwrap(),
            "content"
        );

        let action = workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+Q"));
        assert!(matches!(
            action,
            WorkspaceAction::Command(ref command)
                if command.id.as_str() == "near.app.force-quit"
        ));
        assert!(workspace.should_quit());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn emergency_quit_bypasses_overlays_and_missing_keymap_bindings() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(
            r#"
            [[context]]
            id = "global"
            "#,
        )
        .unwrap();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.show"),
            arguments: BTreeMap::new(),
        });

        let action = workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+Q"));

        assert!(matches!(
            action,
            WorkspaceAction::Command(ref command)
                if command.id.as_str() == "near.app.force-quit"
        ));
        assert!(workspace.should_quit());
    }

    #[test]
    fn temporary_panels_add_references_remove_without_deleting_and_isolate_slots() {
        let mut workspace = FarWorkspace::demo();
        let source = workspace.left.current().unwrap().resource.clone();

        workspace.focused = FocusedPanel::Right;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(2))]),
        });
        assert_eq!(
            workspace.active_contexts()[0].as_str(),
            "workspace.temporary-panel"
        );
        assert_eq!(workspace.right.location().as_str(), "temporary://slots/2");

        workspace.focused = FocusedPanel::Left;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.copy-to-peer"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.temporary_panels[&2].hits.len(), 1);
        assert_eq!(workspace.temporary_panels[&2].hits[0].source, source);
        assert_eq!(workspace.left.current().unwrap().resource, source);
        assert!(workspace.status.contains("source resources unchanged"));

        workspace.focused = FocusedPanel::Right;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.remove"),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.temporary_panels[&2].hits.is_empty());
        assert_eq!(workspace.left.current().unwrap().resource, source);
        assert!(workspace.status.contains("source resources unchanged"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(3))]),
        });
        assert_eq!(workspace.right.location().as_str(), "temporary://slots/3");
        assert!(workspace.temporary_panels[&3].hits.is_empty());
        assert!(workspace.temporary_panels[&2].hits.is_empty());
    }

    #[test]
    fn temporary_panels_persist_preview_target_side_and_never_move_source_files() {
        let root = std::env::temp_dir().join(format!(
            "near-fm-persistent-temporary-panel-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source.txt");
        fs::write(&source_path, b"source remains authoritative").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let store = TestStateDocumentStore::default();
        let left = filesystem_collection(provider.as_ref(), &root, "temporary.persist.left");
        let source_index = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "source.txt")
            .unwrap();
        let mut workspace = FarWorkspace::new(
            left.with_cursor(source_index),
            filesystem_collection(provider.as_ref(), &root, "temporary.persist.right"),
        )
        .with_provider(provider.clone())
        .with_state_document_store(store.clone());
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([
                ("slot".to_owned(), CommandValue::Integer(3)),
                (
                    "target".to_owned(),
                    CommandValue::String("right".to_owned()),
                ),
            ]),
        });
        assert_eq!(workspace.focused, FocusedPanel::Left);
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.copy-to-peer"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.temporary_panels[&3].hits.len(), 1);
        assert_eq!(
            fs::read(&source_path).unwrap(),
            b"source remains authoritative"
        );
        drop(workspace);

        let mut restored = FarWorkspace::new(
            filesystem_collection(provider.as_ref(), &root, "temporary.restore.left"),
            filesystem_collection(provider.as_ref(), &root, "temporary.restore.right"),
        )
        .with_provider(provider)
        .with_state_document_store(store.clone());
        assert_eq!(restored.temporary_panels[&3].hits.len(), 1);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        restored.handle_terminal_event(&mut keymap, key("Alt+F2"));
        let locations = restored.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(locations.contains("[T]emporary"), "{locations}");
        assert!(locations.contains("1 persisted reference"), "{locations}");
        restored.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.list"),
            arguments: BTreeMap::from([(
                "target".to_owned(),
                CommandValue::String("right".to_owned()),
            )]),
        });
        let slots = restored.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(slots.contains("[3]  [ ] 1 reference(s)"), "{slots}");
        assert!(slots.contains("source.txt"), "{slots}");
        restored.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([
                ("slot".to_owned(), CommandValue::Integer(3)),
                (
                    "target".to_owned(),
                    CommandValue::String("right".to_owned()),
                ),
            ]),
        });
        restored.focused = FocusedPanel::Right;
        restored.handle_terminal_event(&mut keymap, key("Shift+F7"));
        assert!(restored.temporary_panels[&3].hits.is_empty());
        assert!(restored.status.contains("source resources unchanged"));
        assert_eq!(
            fs::read(&source_path).unwrap(),
            b"source remains authoritative"
        );

        let reloaded = FarWorkspace::demo().with_state_document_store(store);
        assert!(reloaded.temporary_panels[&3].hits.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn temporary_panel_bindings_switch_slots_and_override_f7() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Alt+Shift+4"));
        assert_eq!(workspace.left.location().as_str(), "temporary://slots/4");
        assert_eq!(
            workspace.active_contexts()[0].as_str(),
            "workspace.temporary-panel"
        );
        let rendered = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(rendered.contains("temporary://slots/4"));
        assert!(rendered.contains("Remove reference"));

        workspace.handle_terminal_event(&mut keymap, key("F7"));
        assert!(
            workspace
                .status
                .contains("No temporary-panel references selected")
        );
        assert!(workspace.overlay.is_none());

        workspace.handle_terminal_event(&mut keymap, key("Alt+Shift+F12"));
        assert!(matches!(workspace.overlay, Some(Overlay::Menu(_))));
    }

    #[test]
    fn temporary_panel_lists_round_trip_provider_identity_and_refresh_stale_resources() {
        let root = std::env::temp_dir().join(format!(
            "near-fm-temporary-panel-list-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source.txt");
        fs::write(&source_path, b"temporary panel source").unwrap();
        let list_path = root.join("panel.temp");
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "temporary.left");
        let source_index = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "source.txt")
            .unwrap();
        let left = left.with_cursor(source_index);
        let right = filesystem_collection(provider.as_ref(), &root, "temporary.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);

        workspace.focused = FocusedPanel::Right;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(2))]),
        });
        workspace.focused = FocusedPanel::Left;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.copy-to-peer"),
            arguments: BTreeMap::new(),
        });
        let original = workspace.temporary_panels[&2].hits[0].source.clone();

        workspace.focused = FocusedPanel::Right;
        workspace.export_temporary_panel(&CommandInvocation {
            id: CommandId::from("near.temp-panel.export-confirmed"),
            arguments: BTreeMap::from([(
                "path".to_owned(),
                CommandValue::String(list_path.to_string_lossy().into_owned()),
            )]),
        });
        assert!(
            fs::read_to_string(&list_path)
                .unwrap()
                .contains(&original.to_string())
        );

        workspace.temporary_panels.get_mut(&2).unwrap().hits.clear();
        workspace.replace_temporary_panel_surface(FocusedPanel::Right, 2);
        workspace.import_temporary_panel(&CommandInvocation {
            id: CommandId::from("near.temp-panel.import-confirmed"),
            arguments: BTreeMap::from([
                (
                    "path".to_owned(),
                    CommandValue::String(list_path.to_string_lossy().into_owned()),
                ),
                (
                    "mode".to_owned(),
                    CommandValue::String("replace".to_owned()),
                ),
            ]),
        });
        assert_eq!(workspace.temporary_panels[&2].hits[0].source, original);

        workspace.reveal_temporary_panel_resource();
        for _ in 0..100 {
            workspace.poll_background_tasks();
            if workspace.right.location() == &LocalFileProvider::location(&root)
                && workspace
                    .right
                    .current()
                    .is_some_and(|entry| entry.resource == original)
                && !workspace
                    .pending_reveal_targets
                    .contains_key(&FocusedPanel::Right)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(workspace.right.current().unwrap().resource, original);
        assert!(
            !workspace
                .pending_reveal_targets
                .contains_key(&FocusedPanel::Right)
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(2))]),
        });

        fs::remove_file(&source_path).unwrap();
        workspace.dispatch(&CommandInvocation::new("near.panel.refresh"));
        let temporary = &workspace.temporary_panels[&2];
        assert_eq!(temporary.hits.len(), 1);
        assert_eq!(temporary.stale_count(), 1);
        assert!(workspace.status.contains("1 stale reference"));
        assert!(workspace.right.current().unwrap().details.contains("Stale"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn temporary_panel_prefix_selects_slot_modes_and_safe_mode_denies_reference_changes() {
        let mut workspace = FarWorkspace::demo();
        let source = workspace.left.current().unwrap().resource.clone();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.focused = FocusedPanel::Right;
        workspace.dispatch_temporary_panel_prefix("+4 +safe");
        assert_eq!(workspace.right.location().as_str(), "temporary://slots/4");
        assert!(workspace.temporary_panels[&4].safe_mode);
        assert!(workspace.status.contains("safe mode"));
        workspace.show_temporary_panels(&CommandInvocation {
            id: CommandId::from("near.temp-panel.list"),
            arguments: BTreeMap::new(),
        });
        let rendered = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(rendered.contains("Temporary Panels"));
        assert!(rendered.contains("reference(s) R"));
        workspace.overlay = None;

        workspace.focused = FocusedPanel::Left;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.copy-to-peer"),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.temporary_panels[&4].hits.is_empty());
        assert_eq!(workspace.left.current().unwrap().resource, source);
        assert!(workspace.status.contains("adding references is disabled"));

        workspace.focused = FocusedPanel::Right;
        workspace.dispatch_temporary_panel_prefix("+4 -safe");
        assert!(!workspace.temporary_panels[&4].safe_mode);
    }

    #[test]
    fn temporary_panel_any_mode_keeps_lines_and_enter_copies_text_to_command_line() {
        let root = std::env::temp_dir().join(format!(
            "near-fm-temporary-panel-any-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let list_path = root.join("commands.temp");
        fs::write(
            &list_path,
            "printf 'first line'\nsftp://example.invalid/path\n",
        )
        .unwrap();
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch_temporary_panel_prefix(&format!(
            "+6 +any +replace \"{}\"",
            list_path.display()
        ));
        let temporary = &workspace.temporary_panels[&6];
        assert!(temporary.allow_arbitrary);
        assert_eq!(temporary.hits.len(), 2);
        assert!(
            temporary.hits[0]
                .metadata
                .extensions
                .contains_key("near.temporary-panel.arbitrary-text")
        );
        let rendered = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(rendered.contains("printf 'first line'"));

        workspace.open_current();
        assert_eq!(workspace.command_line.buffer(), "printf 'first line'");
        assert!(workspace.status.contains("Copied Temporary Panel text"));
        assert!(workspace.refresh_temporary_panel(FocusedPanel::Left));
        assert_eq!(workspace.temporary_panels[&6].stale_count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn temporary_panel_export_binding_opens_the_list_dialog() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.dispatch_temporary_panel_prefix("+1");

        workspace.handle_terminal_event(&mut keymap, key("Alt+Shift+F2"));

        let Some(Overlay::Surface(surface)) = workspace.overlay.as_ref() else {
            panic!("Alt+Shift+F2 should open the Temporary Panel export dialog");
        };
        assert_eq!(surface.id().as_str(), "near-fm.temporary-panel-export");
    }

    #[test]
    fn temporary_panel_command_output_is_captured_asynchronously_with_prefix_modes() {
        let mut workspace =
            FarWorkspace::demo().with_command_line_executor(TemporaryPanelCommandExecutor);

        workspace.dispatch_temporary_panel_prefix("+7+any+replace<emit-lines");
        assert_eq!(workspace.left.location().as_str(), "temporary://slots/7");
        assert!(workspace.status.contains("running <emit-lines"));
        assert!(workspace.temporary_panels[&7].hits.is_empty());

        for _ in 0..100 {
            workspace.poll_background_tasks();
            if workspace.status.contains("command exited") {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(workspace.status.contains("added 2, rejected 0"));
        assert_eq!(workspace.temporary_panels[&7].hits.len(), 2);
        assert_eq!(
            workspace.left.current().unwrap().metadata.name,
            "first from emit-lines"
        );
        let task = workspace.task_records.values().next().unwrap();
        assert_eq!(task.state, TaskState::Completed);
        workspace.dispatch(&CommandInvocation::new("near.demo.tasks"));
        let rendered = workspace
            .snapshot(
                &SemanticTheme::from_toml(THEME).unwrap(),
                &Keymap::from_toml(KEYMAP).unwrap(),
                100,
                28,
            )
            .join("\n");
        assert!(rendered.contains("Temporary-panel command"));
        workspace.open_current();
        assert_eq!(workspace.command_line.buffer(), "first from emit-lines");
    }

    #[test]
    fn temporary_panel_menu_mode_renders_labels_and_routes_paths_or_command_text() {
        let root = std::env::temp_dir().join(format!(
            "near-fm-temporary-panel-menu-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("destination")).unwrap();
        let list_path = root.join("shortcuts.temp");
        fs::write(
            &list_path,
            format!(
                "|&Destination|{}\n|-|\n|&Command|printf menu-action\n",
                root.join("destination").display()
            ),
        )
        .unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "temporary.menu.left");
        let right = filesystem_collection(provider.as_ref(), &root, "temporary.menu.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch_temporary_panel_prefix(&format!("+menu\"{}\"", list_path.display()));
        let rendered = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(rendered.contains("shortcuts"));
        assert!(rendered.contains("[D]estination"));
        assert!(rendered.contains("[C]ommand"));
        assert!(rendered.contains("────────"));

        workspace.activate_temporary_panel_menu_item(&CommandInvocation {
            id: CommandId::from("near.temp-panel.menu-select"),
            arguments: BTreeMap::from([(
                "text".to_owned(),
                CommandValue::String("printf menu-action".to_owned()),
            )]),
        });
        assert_eq!(workspace.command_line.buffer(), "printf menu-action");

        workspace.activate_temporary_panel_menu_item(&CommandInvocation {
            id: CommandId::from("near.temp-panel.menu-select"),
            arguments: BTreeMap::from([(
                "text".to_owned(),
                CommandValue::String(root.join("destination").to_string_lossy().into_owned()),
            )]),
        });
        for _ in 0..100 {
            workspace.poll_background_tasks();
            if workspace.left.location() == &LocalFileProvider::location(&root.join("destination"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            workspace.left.location(),
            &LocalFileProvider::location(&root.join("destination"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn temporary_panel_full_mode_uses_the_complete_panel_viewport_and_restores_dual_panes() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch_temporary_panel_prefix("+9+full");
        let full = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(workspace.temporary_panels[&9].full_screen);
        assert!(full.contains("temporary://slots/9"));
        assert!(!full.contains("Home [Unsorted"));
        assert!(workspace.status.contains("full-screen mode"));

        workspace.dispatch_temporary_panel_prefix("+9-full");
        let dual = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
        assert!(!workspace.temporary_panels[&9].full_screen);
        assert!(dual.contains("Home [Unsorted"));
    }

    #[test]
    fn full_screen_temporary_panel_mouse_routes_the_whole_viewport_to_the_focused_panel() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.focused = FocusedPanel::Left;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(6))]),
        });
        workspace.focused = FocusedPanel::Right;
        workspace.right.toggle_selection();
        workspace.right.move_cursor(1);
        workspace.right.toggle_selection();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.copy-to-peer"),
            arguments: BTreeMap::new(),
        });
        workspace.focused = FocusedPanel::Left;
        workspace.temporary_panels.get_mut(&6).unwrap().full_screen = true;
        workspace.replace_temporary_panel_surface(FocusedPanel::Left, 6);
        let right_cursor = workspace.right.cursor();
        workspace.snapshot(&theme, &keymap, 100, 28);

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 75, 2),
        );

        assert_eq!(workspace.focused, FocusedPanel::Left);
        assert_eq!(workspace.left.cursor(), 1);
        assert_eq!(workspace.right.cursor(), right_cursor);
    }

    #[test]
    fn full_screen_temporary_panel_hit_area_follows_hidden_footer_rows() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.temp-panel.open"),
            arguments: BTreeMap::from([("slot".to_owned(), CommandValue::Integer(6))]),
        });
        workspace.temporary_panels.get_mut(&6).unwrap().full_screen = true;
        let entries = (0..30)
            .map(|index| {
                CollectionEntry::new(
                    ResourceRef {
                        provider: ProviderId::from("near.demo"),
                        location: Location::new(format!("demo://full/{index}")),
                    },
                    ResourceMetadata {
                        name: format!("item-{index:02}"),
                        kind: ResourceKind::File,
                        ..ResourceMetadata::default()
                    },
                    "file",
                )
            })
            .collect();
        workspace
            .left
            .replace(Location::new("temporary://slots/6"), entries);
        workspace.settings.interface.show_status_line = false;
        workspace.settings.interface.show_keybar = false;
        workspace.snapshot(&theme, &keymap, 100, 28);

        workspace.handle_terminal_event(
            &mut keymap,
            mouse(MouseEventKind::Down(MouseButton::Left), 75, 25),
        );

        assert_eq!(workspace.focused, FocusedPanel::Left);
        assert_eq!(workspace.left.cursor(), 24);
    }

    #[test]
    fn temporary_panel_safe_mode_denies_import_and_command_capture_mutation() {
        let mut workspace = FarWorkspace::demo();
        workspace.dispatch_temporary_panel_prefix("+4+safe");
        let before = workspace.temporary_panels[&4].clone();

        let import = workspace.ingest_temporary_panel_text(
            FocusedPanel::Left,
            4,
            "arbitrary command text",
            true,
            true,
        );
        assert!(import.unwrap_err().contains("safe mode"));
        assert_eq!(workspace.temporary_panels[&4].hits, before.hits);
        assert_eq!(
            workspace.temporary_panels[&4].allow_arbitrary,
            before.allow_arbitrary
        );

        workspace.finish_temporary_panel_command(
            0,
            FocusedPanel::Left,
            4,
            (true, true),
            "printf unsafe".to_owned(),
            Ok(CommandLineOutput {
                stdout: "another line".to_owned(),
                stderr: String::new(),
                exit_code: Some(0),
            }),
        );
        assert!(workspace.status.contains("safe mode"));
        assert_eq!(workspace.temporary_panels[&4].hits, before.hits);
    }

    #[test]
    fn temporary_panel_arbitrary_append_allocates_unique_synthetic_resources() {
        let mut workspace = FarWorkspace::demo();
        workspace.dispatch_temporary_panel_prefix("+4+any");
        assert_eq!(
            workspace
                .ingest_temporary_panel_text(FocusedPanel::Left, 4, "first\nsecond", false, true)
                .unwrap(),
            (2, 0)
        );
        assert_eq!(
            workspace
                .ingest_temporary_panel_text(FocusedPanel::Left, 4, "third\nfourth", false, true)
                .unwrap(),
            (2, 0)
        );
        let temporary = &workspace.temporary_panels[&4];
        assert_eq!(temporary.hits.len(), 4);
        assert_eq!(
            temporary
                .hits
                .iter()
                .map(|hit| hit.source.location.as_str())
                .collect::<Vec<_>>(),
            vec![
                "temporary-text://4/0",
                "temporary-text://4/1",
                "temporary-text://4/2",
                "temporary-text://4/3",
            ]
        );
    }

    #[test]
    fn recursive_search_streams_actionable_source_resources_without_blocking_input() {
        let root = std::env::temp_dir().join(format!("near-fm-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("first.txt"), b"needle at root").unwrap();
        fs::write(root.join("nested/second.txt"), b"another needle").unwrap();
        fs::write(root.join("nested/no-match.txt"), b"other content").unwrap();
        fs::write(root.join("third.md"), b"needle in markdown").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "search.left");
        let right = filesystem_collection(provider.as_ref(), &root, "search.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_external_tool_resolver(LocalExternalToolResolver::macos_text_editor());

        let started = Instant::now();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.confirmed"),
            arguments: BTreeMap::from([
                ("name".to_owned(), CommandValue::String("*.txt".to_owned())),
                (
                    "content".to_owned(),
                    CommandValue::String("needle".to_owned()),
                ),
            ]),
        });
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(
            workspace
                .focused_panel()
                .location()
                .as_str()
                .starts_with("search://")
        );
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.collection.move"),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
        });
        wait_for_search(&mut workspace, FocusedPanel::Left);

        assert_eq!(workspace.focused_panel().entries().len(), 2);
        assert!(
            workspace.focused_panel().entries().iter().all(|entry| entry
                .resource
                .provider
                .as_str()
                == "near.local-fs")
        );
        let source = workspace
            .focused_panel()
            .current()
            .unwrap()
            .resource
            .clone();
        let expected_parent = LocalFileProvider
            .parent(&source.location)
            .expect("search result should have a local parent");
        assert_eq!(workspace.canonical_targets(), vec![source.clone()]);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.view"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.viewer");
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.overlay.cancel"),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.edit-external"),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.take_external_invocation().is_some());

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.keep-panel"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.saved_search_panels.len(), 1);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.reveal"),
            arguments: BTreeMap::new(),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(
            !workspace
                .focused_panel()
                .location()
                .as_str()
                .starts_with("search://")
        );
        assert_eq!(workspace.focused_panel().location(), &expected_parent);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.open-panel"),
            arguments: BTreeMap::from([("session".to_owned(), CommandValue::Integer(1))]),
        });
        assert_eq!(workspace.focused_panel().entries().len(), 2);
        assert!(
            workspace
                .focused_panel()
                .location()
                .as_str()
                .starts_with("search://")
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.confirmed"),
            arguments: BTreeMap::from([
                ("name".to_owned(), CommandValue::String("*.md".to_owned())),
                (
                    "content".to_owned(),
                    CommandValue::String("needle".to_owned()),
                ),
                ("mode".to_owned(), CommandValue::String("append".to_owned())),
            ]),
        });
        wait_for_search(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.focused_panel().entries().len(), 3);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.confirmed"),
            arguments: BTreeMap::from([
                ("name".to_owned(), CommandValue::String("*.md".to_owned())),
                (
                    "content".to_owned(),
                    CommandValue::String("needle".to_owned()),
                ),
                ("mode".to_owned(), CommandValue::String("append".to_owned())),
            ]),
        });
        wait_for_search(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.focused_panel().entries().len(), 3);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.confirmed"),
            arguments: BTreeMap::from([
                ("name".to_owned(), CommandValue::String("first*".to_owned())),
                (
                    "content".to_owned(),
                    CommandValue::String("needle".to_owned()),
                ),
                ("mode".to_owned(), CommandValue::String("refine".to_owned())),
            ]),
        });
        wait_for_search(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.focused_panel().entries().len(), 1);
        assert_eq!(
            workspace.focused_panel().entries()[0].metadata.name,
            "first.txt"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn advanced_search_dialog_composes_regex_encoding_size_date_and_attributes() {
        let root =
            std::env::temp_dir().join(format!("near-fm-advanced-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let encoded = "Café ticket 2048"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        fs::write(
            root.join("report-2026.txt"),
            [vec![0xff, 0xfe], encoded].concat(),
        )
        .unwrap();
        fs::write(root.join("report-old.txt"), b"Cafe ticket 5").unwrap();
        fs::write(root.join("notes.md"), b"Cafe ticket 2048").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "advanced-search.left");
        let right = filesystem_collection(provider.as_ref(), &root, "advanced-search.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);

        workspace.dispatch(&CommandInvocation {
            id: "near.search.confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "name".to_owned(),
                    CommandValue::String(r"^report-\d{4}\.txt$".to_owned()),
                ),
                (
                    "name_mode".to_owned(),
                    CommandValue::String("regex".to_owned()),
                ),
                (
                    "content".to_owned(),
                    CommandValue::String(r"Caf. ticket \d+".to_owned()),
                ),
                (
                    "content_mode".to_owned(),
                    CommandValue::String("regex".to_owned()),
                ),
                (
                    "encoding".to_owned(),
                    CommandValue::String("utf16le".to_owned()),
                ),
                (
                    "case_sensitive".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
                ("kinds".to_owned(), CommandValue::String("file".to_owned())),
                (
                    "minimum_size".to_owned(),
                    CommandValue::String("16B".to_owned()),
                ),
                (
                    "maximum_size".to_owned(),
                    CommandValue::String("1K".to_owned()),
                ),
                (
                    "modified_after".to_owned(),
                    CommandValue::String("2020-01-01".to_owned()),
                ),
                (
                    "modified_before".to_owned(),
                    CommandValue::String("2030-01-01".to_owned()),
                ),
                (
                    "readonly".to_owned(),
                    CommandValue::String("any".to_owned()),
                ),
                (
                    "executable".to_owned(),
                    CommandValue::String("no".to_owned()),
                ),
                (
                    "hidden".to_owned(),
                    CommandValue::String("include".to_owned()),
                ),
                (
                    "ignore".to_owned(),
                    CommandValue::String("common".to_owned()),
                ),
            ]),
        });
        wait_for_search(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.focused_panel().entries().len(), 1);
        assert_eq!(
            workspace.focused_panel().entries()[0].metadata.name,
            "report-2026.txt"
        );

        workspace.dispatch(&CommandInvocation {
            id: "near.search.confirmed".into(),
            arguments: BTreeMap::from([
                ("name".to_owned(), CommandValue::String("(".to_owned())),
                (
                    "name_mode".to_owned(),
                    CommandValue::String("regex".to_owned()),
                ),
            ]),
        });
        assert!(workspace.status.contains("invalid search field name"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scoped_search_includes_archives_and_surfaces_stream_capability_diagnostics() {
        let root =
            std::env::temp_dir().join(format!("near-fm-scoped-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("selected-root")).unwrap();
        fs::write(root.join("outside.txt"), b"outside").unwrap();
        let archive_path = root.join("fixture.zip");
        let mut archive = ZipWriter::new(fs::File::create(&archive_path).unwrap());
        archive
            .start_file("inside.txt", SimpleFileOptions::default())
            .unwrap();
        use std::io::Write as _;
        archive.write_all(b"inside").unwrap();
        archive.finish().unwrap();

        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "scoped-search.left");
        let right = filesystem_collection(provider.as_ref(), &root, "scoped-search.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_provider(Arc::new(ZipArchiveProvider));
        let archive_resource = workspace
            .left
            .entries()
            .iter()
            .find(|entry| entry.metadata.name == "fixture.zip")
            .unwrap()
            .resource
            .clone();
        let selected_directory = workspace
            .left
            .entries()
            .iter()
            .find(|entry| entry.metadata.name == "selected-root")
            .unwrap()
            .resource
            .clone();
        workspace
            .left
            .restore_selection(&[selected_directory.clone()]);
        let selected_roots = workspace
            .search_roots(super::SearchScope::SelectedRoots)
            .unwrap();
        assert_eq!(selected_roots.len(), 1);
        assert_eq!(selected_roots[0].location, selected_directory.location);
        workspace.left.restore_selection(&[archive_resource]);
        let archive_roots = workspace
            .search_roots(super::SearchScope::Archives)
            .unwrap();
        assert_eq!(archive_roots.len(), 1);
        assert_eq!(archive_roots[0].provider, ProviderId::from("near.archive"));
        let provider_roots = workspace
            .search_roots(super::SearchScope::Providers)
            .unwrap();
        assert!(
            provider_roots
                .iter()
                .any(|root| root.provider == ProviderId::from("near.local-fs"))
        );
        workspace.left.restore_selection(&[]);
        workspace.dispatch(&CommandInvocation {
            id: "near.search.confirmed".into(),
            arguments: BTreeMap::from([
                ("name".to_owned(), CommandValue::String("*.txt".to_owned())),
                (
                    "archives".to_owned(),
                    CommandValue::String("include".to_owned()),
                ),
                (
                    "streams".to_owned(),
                    CommandValue::String("include".to_owned()),
                ),
            ]),
        });
        wait_for_search(&mut workspace, FocusedPanel::Left);
        let mut names = workspace
            .focused_panel()
            .entries()
            .iter()
            .map(|entry| entry.metadata.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["inside.txt", "outside.txt"]);
        let state = workspace.searches.get(&FocusedPanel::Left).unwrap();
        assert!(state.diagnostics.iter().any(|diagnostic| {
            diagnostic.capability == "resource.streams"
                && diagnostic.provider == ProviderId::from("near.local-fs")
        }));
        assert!(workspace.status.contains("capability diagnostics"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quick_view_tracks_the_cursor_in_the_peer_panel() {
        let root = std::env::temp_dir().join(format!("near-fm-quick-view-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"first-quick-content").unwrap();
        fs::write(root.join("b.txt"), b"second-quick-content").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "quick.left");
        let right = filesystem_collection(provider.as_ref(), &root, "quick.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: "near.panel.toggle-quick-view".into(),
            arguments: BTreeMap::new(),
        });
        while workspace.quick_view_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        let first = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(first.contains("first-quick-content"));
        assert!(
            workspace
                .semantic_snapshot(&theme, &keymap, 100, 30)
                .role_lines()
                .join("\n")
                .contains("viewer.text")
        );

        workspace.dispatch(&CommandInvocation {
            id: "near.collection.move".into(),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
        });
        while workspace.quick_view_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        let second = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(second.contains("second-quick-content"));
        assert!(!second.contains("first-quick-content"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quick_view_summarizes_directories_and_exposes_viewer_navigation_and_search() {
        let root =
            std::env::temp_dir().join(format!("near-fm-quick-control-{}", std::process::id()));
        let folder = root.join("folder");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("child.txt"), b"child").unwrap();
        let content = (0..120)
            .map(|line| {
                if line == 90 {
                    format!("line {line} needle\n")
                } else {
                    format!("line {line}\n")
                }
            })
            .collect::<String>();
        fs::write(root.join("document.txt"), content).unwrap();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &root, "quick-control.left");
        let folder_index = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "folder")
            .unwrap();
        left.set_cursor(folder_index);
        let right = filesystem_collection(provider.as_ref(), &root, "quick-control.right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Q"));
        while workspace.quick_view_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        let directory = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(directory.contains("Directory summary"));
        assert!(directory.contains("child.txt"));
        assert!(directory.contains("Visible items:"));

        let document_index = workspace
            .left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "document.txt")
            .unwrap();
        workspace.left.set_cursor(document_index);
        workspace.refresh_quick_view();
        while workspace.quick_view_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+Q"));
        assert!(workspace.quick_view_interactive);
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.viewer");
        workspace.handle_terminal_event(&mut keymap, key("PageDown"));
        assert!(workspace.quick_view.as_ref().unwrap().offset() > 0);
        workspace.handle_terminal_event(&mut keymap, key("F7"));
        for character in ["n", "e", "e", "d", "l", "e"] {
            workspace.handle_terminal_event(&mut keymap, key(character));
        }
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        let searched = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(searched.contains("needle"));
        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        assert!(!workspace.quick_view_interactive);
        assert_eq!(workspace.active_contexts()[0].as_str(), "workspace.panel");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tree_information_and_quick_view_are_independent_panel_types() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+T"));
        assert!(workspace.overlay.is_none());
        assert_eq!(workspace.left_panel_type, PanelType::Tree);
        assert_eq!(workspace.right_panel_type, PanelType::File);
        let tree = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(tree.contains("Tree —"), "{tree}");
        assert!(tree.contains("/Users/alex/Projects/Near"), "{tree}");
        assert!(tree.contains("crates"));

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+L"));
        assert_eq!(workspace.left_panel_type, PanelType::Information);
        let information = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(information.contains("Information —"), "{information}");
        assert!(
            information.contains("/Users/alex/Projects/Near"),
            "{information}"
        );
        assert!(information.contains("Location"));
        assert!(information.contains("Entries"));
        assert!(information.contains("crates"));

        workspace.handle_terminal_event(&mut keymap, key("Tab"));
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+T"));
        assert_eq!(workspace.left_panel_type, PanelType::Information);
        assert_eq!(workspace.right_panel_type, PanelType::Tree);
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+T"));
        assert_eq!(workspace.right_panel_type, PanelType::File);

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Q"));
        assert_eq!(workspace.left_panel_type, PanelType::QuickView);
        assert_eq!(workspace.right_panel_type, PanelType::File);
        assert_eq!(workspace.focused, FocusedPanel::Right);
        workspace.handle_terminal_event(&mut keymap, key("Tab"));
        assert_eq!(workspace.focused, FocusedPanel::Right);
        assert_eq!(workspace.status, "Quick view remains the passive panel");
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Q"));
        assert_eq!(workspace.left_panel_type, PanelType::Information);
        assert!(workspace.quick_view.is_none());
    }

    struct DelayedViewerProvider;

    struct LocationMenuProvider;

    impl ResourceProvider for LocationMenuProvider {
        fn id(&self) -> ProviderId {
            ProviderId::from("near.locations")
        }

        fn schemes(&self) -> &[&str] {
            &["loc"]
        }

        fn locations(&self) -> Vec<ProviderLocation> {
            vec![
                ProviderLocation {
                    location: Location::new("loc://alpha"),
                    label: "Alpha root".to_owned(),
                    detail: "A provider root".to_owned(),
                },
                ProviderLocation {
                    location: Location::new("loc://beta"),
                    label: "Beta root".to_owned(),
                    detail: "B provider root".to_owned(),
                },
            ]
        }

        fn list<'a>(
            &'a self,
            location: &'a Location,
            request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            let location = location.clone();
            Box::pin(async move {
                Ok(ListPage {
                    generation: request.generation,
                    entries: vec![ResourceEntry {
                        resource: ResourceRef {
                            provider: ProviderId::from("near.locations"),
                            location: Location::new(format!("{}/item.txt", location.as_str())),
                        },
                        metadata: ResourceMetadata {
                            name: "item.txt".to_owned(),
                            kind: ResourceKind::File,
                            ..ResourceMetadata::default()
                        },
                        details: location.as_str().to_owned(),
                    }],
                    continuation: None,
                    complete: true,
                })
            })
        }

        fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            let name = resource
                .location
                .as_str()
                .rsplit('/')
                .next()
                .unwrap_or("item")
                .to_owned();
            Box::pin(async move {
                Ok(ResourceMetadata {
                    name,
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                })
            })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            _request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async { Err(ProviderError::Unsupported("open".to_owned())) })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            CapabilitySet::default()
        }
    }

    #[test]
    fn location_menus_show_provider_metadata_and_navigate_exact_panels() {
        let provider: Arc<dyn ResourceProvider> = Arc::new(LocationMenuProvider);
        let mut workspace = FarWorkspace::new(
            CollectionSurface::new(
                "locations.left",
                "workspace.panel",
                "Left",
                Location::new("loc://left-start"),
                Vec::new(),
            ),
            CollectionSurface::new(
                "locations.right",
                "workspace.panel",
                "Right",
                Location::new("loc://right-start"),
                Vec::new(),
            ),
        )
        .with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Tab"));
        assert_eq!(workspace.focused, FocusedPanel::Right);
        workspace.handle_terminal_event(&mut keymap, key("Alt+F1"));
        let left_menu = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(left_menu.contains("Left Panel Locations"));
        assert!(left_menu.contains("Alpha root"));
        assert!(left_menu.contains("near.locations"), "{left_menu}");
        assert!(left_menu.contains("A provider root"), "{left_menu}");
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.left.location().as_str(), "loc://beta");
        assert_eq!(workspace.right.location().as_str(), "loc://right-start");
        assert_eq!(workspace.focused, FocusedPanel::Right);
        assert_eq!(workspace.left.entries()[0].metadata.name, "item.txt");

        workspace.handle_terminal_event(&mut keymap, key("Alt+F2"));
        let right_menu = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(right_menu.contains("Right Panel Locations"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        wait_for_listing(&mut workspace, FocusedPanel::Right);
        assert_eq!(workspace.right.location().as_str(), "loc://alpha");
    }

    impl ResourceProvider for DelayedViewerProvider {
        fn id(&self) -> ProviderId {
            "near.delayed-viewer".into()
        }

        fn schemes(&self) -> &[&str] {
            &["delay"]
        }

        fn list<'a>(
            &'a self,
            _location: &'a Location,
            _request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            Box::pin(async { Err(ProviderError::Unsupported("list".to_owned())) })
        }

        fn stat<'a>(&'a self, _resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            Box::pin(async { Ok(ResourceMetadata::default()) })
        }

        fn open<'a>(
            &'a self,
            resource: &'a ResourceRef,
            _request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            let location = resource.location.as_str().to_owned();
            Box::pin(async move {
                if location.ends_with("slow") {
                    std::thread::sleep(Duration::from_millis(100));
                    Ok(ResourceStream {
                        offset: 0,
                        bytes: b"stale-slow-content".to_vec(),
                        total_size: Some(18),
                        complete: true,
                    })
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                    Ok(ResourceStream {
                        offset: 0,
                        bytes: b"current-fast-content".to_vec(),
                        total_size: Some(20),
                        complete: true,
                    })
                }
            })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            CapabilitySet::default()
        }
    }

    #[test]
    fn delayed_quick_view_remains_responsive_and_rejects_stale_completion() {
        let provider = Arc::new(DelayedViewerProvider);
        let entries = [("slow", "delay:///slow"), ("fast", "delay:///fast")]
            .into_iter()
            .map(|(name, location)| CollectionEntry {
                resource: ResourceRef {
                    provider: provider.id(),
                    location: Location::new(location),
                },
                metadata: ResourceMetadata {
                    name: name.to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                },
                details: "file".to_owned(),
                selected: false,
            })
            .collect::<Vec<_>>();
        let left = CollectionSurface::new(
            "delayed.left",
            "workspace.panel",
            "Delayed",
            Location::new("delay:///"),
            entries.clone(),
        );
        let right = CollectionSurface::new(
            "delayed.right",
            "workspace.panel",
            "Peer",
            Location::new("delay:///"),
            entries,
        );
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.dispatch(&CommandInvocation {
            id: "near.panel.toggle-quick-view".into(),
            arguments: BTreeMap::new(),
        });

        let started = Instant::now();
        workspace.dispatch(&CommandInvocation {
            id: "near.collection.move".into(),
            arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
        });
        assert!(started.elapsed() < Duration::from_millis(20));

        let deadline = Instant::now() + Duration::from_secs(1);
        while workspace.quick_view_task.is_some() && Instant::now() < deadline {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        let current = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(current.contains("current-fast-content"));
        std::thread::sleep(Duration::from_millis(120));
        workspace.poll_background_tasks();
        let after_stale = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(after_stale.contains("current-fast-content"));
        assert!(!after_stale.contains("stale-slow-content"));
    }

    struct ScriptedListingProvider;

    impl ScriptedListingProvider {
        fn entry(&self, location: &str, name: &str) -> ResourceEntry {
            ResourceEntry {
                resource: ResourceRef {
                    provider: self.id(),
                    location: Location::new(location),
                },
                metadata: ResourceMetadata {
                    name: name.to_owned(),
                    kind: ResourceKind::File,
                    ..ResourceMetadata::default()
                },
                details: "pending metadata".to_owned(),
            }
        }
    }

    impl ResourceProvider for ScriptedListingProvider {
        fn id(&self) -> ProviderId {
            "near.scripted-listing".into()
        }

        fn schemes(&self) -> &[&str] {
            &["scripted"]
        }

        fn list<'a>(
            &'a self,
            location: &'a Location,
            request: ListRequest,
        ) -> ProviderFuture<'a, ListPage> {
            let location = location.as_str().to_owned();
            Box::pin(async move {
                let page = match (location.as_str(), request.continuation.as_deref()) {
                    ("scripted:///slow", None) => {
                        std::thread::sleep(Duration::from_millis(100));
                        ListPage {
                            generation: request.generation,
                            entries: vec![self.entry("scripted:///slow/stale", "stale")],
                            continuation: None,
                            complete: true,
                        }
                    }
                    ("scripted:///fast", None) => {
                        std::thread::sleep(Duration::from_millis(5));
                        ListPage {
                            generation: request.generation,
                            entries: vec![self.entry("scripted:///fast/current", "current")],
                            continuation: None,
                            complete: true,
                        }
                    }
                    ("scripted:///paged", None) => {
                        std::thread::sleep(Duration::from_millis(5));
                        ListPage {
                            generation: request.generation,
                            entries: vec![self.entry("scripted:///paged/first", "first")],
                            continuation: Some("second".to_owned()),
                            complete: false,
                        }
                    }
                    ("scripted:///paged", Some("second")) => {
                        std::thread::sleep(Duration::from_millis(80));
                        ListPage {
                            generation: request.generation,
                            entries: vec![self.entry("scripted:///paged/second", "second")],
                            continuation: None,
                            complete: true,
                        }
                    }
                    ("scripted:///partial", None) => ListPage {
                        generation: request.generation,
                        entries: vec![self.entry("scripted:///partial/kept", "kept")],
                        continuation: Some("failure".to_owned()),
                        complete: false,
                    },
                    ("scripted:///partial", Some("failure")) => {
                        std::thread::sleep(Duration::from_millis(40));
                        return Err(ProviderError::Failed("page two unavailable".to_owned()));
                    }
                    ("scripted:///broken", None) => ListPage {
                        generation: request.generation,
                        entries: vec![self.entry("scripted:///broken/item", "broken")],
                        continuation: None,
                        complete: true,
                    },
                    _ => {
                        return Err(ProviderError::NotFound(ResourceRef {
                            provider: self.id(),
                            location: Location::new(location),
                        }));
                    }
                };
                Ok(page)
            })
        }

        fn stat<'a>(&'a self, resource: &'a ResourceRef) -> ProviderFuture<'a, ResourceMetadata> {
            let resource = resource.clone();
            Box::pin(async move {
                if resource.location.as_str().contains("/broken/") {
                    return Err(ProviderError::Failed("metadata unavailable".to_owned()));
                }
                if resource.location.as_str().contains("/paged/") {
                    std::thread::sleep(Duration::from_millis(120));
                }
                let name = resource
                    .location
                    .as_str()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                Ok(ResourceMetadata {
                    name,
                    kind: ResourceKind::File,
                    size: Some(42),
                    ..ResourceMetadata::default()
                })
            })
        }

        fn open<'a>(
            &'a self,
            _resource: &'a ResourceRef,
            _request: OpenRequest,
        ) -> ProviderFuture<'a, ResourceStream> {
            Box::pin(async { Err(ProviderError::Unsupported("open".to_owned())) })
        }

        fn capabilities(&self, _resource: &ResourceRef) -> CapabilitySet {
            CapabilitySet::default()
        }
    }

    #[test]
    fn asynchronous_listing_rejects_a_stale_generation() {
        let provider: Arc<dyn ResourceProvider> = Arc::new(ScriptedListingProvider);
        let mut workspace = FarWorkspace::demo().with_provider(Arc::clone(&provider));
        workspace.start_listing(
            FocusedPanel::Left,
            &provider,
            &Location::new("scripted:///slow"),
        );
        std::thread::sleep(Duration::from_millis(10));

        let started = Instant::now();
        workspace.start_listing(
            FocusedPanel::Left,
            &provider,
            &Location::new("scripted:///fast"),
        );
        assert!(started.elapsed() < Duration::from_millis(20));
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(
            workspace.left.location(),
            &Location::new("scripted:///fast")
        );
        assert_eq!(workspace.left.entries()[0].metadata.name, "current");

        std::thread::sleep(Duration::from_millis(120));
        workspace.poll_background_tasks();
        assert_eq!(
            workspace.left.location(),
            &Location::new("scripted:///fast")
        );
        assert_eq!(workspace.left.entries()[0].metadata.name, "current");
    }

    #[test]
    fn paged_listing_renders_before_continuation_and_metadata_hydration() {
        let provider: Arc<dyn ResourceProvider> = Arc::new(ScriptedListingProvider);
        let mut workspace = FarWorkspace::demo().with_provider(Arc::clone(&provider));
        workspace.start_listing(
            FocusedPanel::Left,
            &provider,
            &Location::new("scripted:///paged"),
        );

        let deadline = Instant::now() + Duration::from_secs(1);
        while (workspace.left.location() != &Location::new("scripted:///paged")
            || workspace.left.entries().is_empty())
            && Instant::now() < deadline
        {
            workspace.poll_background_tasks();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(workspace.left.entries().len(), 1);
        assert_eq!(workspace.left.entries()[0].metadata.name, "first");
        assert_eq!(workspace.left.entries()[0].metadata.size, None);
        assert!(
            workspace
                .listing_state(FocusedPanel::Left)
                .is_some_and(|state| !state.tasks.is_empty())
        );

        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.left.entries().len(), 2);
        assert!(
            workspace
                .left
                .entries()
                .iter()
                .all(|entry| entry.metadata.size == Some(42))
        );
        let export = workspace.diagnostic_export();
        assert!(export.events.iter().any(|event| {
            event.domain == DiagnosticDomain::Provider && event.phase == DiagnosticPhase::Completed
        }));
    }

    #[test]
    fn listing_and_metadata_failures_preserve_usable_partial_results() {
        let provider: Arc<dyn ResourceProvider> = Arc::new(ScriptedListingProvider);
        let mut workspace = FarWorkspace::demo().with_provider(Arc::clone(&provider));
        workspace.start_listing(
            FocusedPanel::Left,
            &provider,
            &Location::new("scripted:///partial"),
        );
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(workspace.left.entries().len(), 1);
        assert_eq!(workspace.left.entries()[0].metadata.name, "kept");
        assert!(workspace.status.contains("Partial listing failure"));

        workspace.start_listing(
            FocusedPanel::Left,
            &provider,
            &Location::new("scripted:///broken"),
        );
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        let broken = &workspace.left.entries()[0];
        assert_eq!(broken.metadata.name, "broken");
        assert_eq!(broken.details, "metadata error");
        assert_eq!(
            broken.metadata.field_errors.get("stat").map(String::as_str),
            Some("provider failed: metadata unavailable")
        );
        assert!(workspace.status.contains("1 metadata failures"));
    }

    #[test]
    fn far_copy_command_previews_executes_and_refreshes_provider_panels() {
        let root = std::env::temp_dir().join(format!("near-fm-copy-{}", std::process::id()));
        let source = root.join("source");
        let destination = root.join("destination");
        let trash = root.join("Trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("copy.txt"), b"copy-content").unwrap();
        let provider = Arc::new(LocalFileProvider);

        let left = filesystem_collection(provider.as_ref(), &source, "source");
        let right = filesystem_collection(provider.as_ref(), &destination, "destination");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                trash,
                OperationJournal::memory(),
            ));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F5"));
        assert_eq!(
            workspace.active_contexts()[0].as_str(),
            "surface.operation-preview"
        );
        let preview = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(preview.contains("Operation Preview"));
        assert!(preview.contains("operation: Copy"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        wait_for_listing(&mut workspace, FocusedPanel::Right);

        assert_eq!(
            fs::read(destination.join("copy.txt")).unwrap(),
            b"copy-content"
        );
        assert!(
            workspace
                .right
                .entries()
                .iter()
                .any(|entry| entry.metadata.name == "copy.txt")
        );
        assert!(
            workspace
                .task_records
                .values()
                .any(|task| task.state == TaskState::Completed
                    && task.message.contains("1 completed"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn permanent_delete_and_wipe_require_two_step_high_impact_confirmation() {
        let root = std::env::temp_dir().join(format!("near-fm-delete-{}", std::process::id()));
        let peer = root.join("peer");
        let trash = root.join("Trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&peer).unwrap();
        fs::write(root.join("delete.txt"), b"delete-content").unwrap();
        fs::write(root.join("wipe.bin"), vec![0x5a_u8; 96 * 1024]).unwrap();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &root, "delete.source");
        let delete_index = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "delete.txt")
            .unwrap();
        left.set_cursor(delete_index);
        let right = filesystem_collection(provider.as_ref(), &peer, "delete.peer");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                trash,
                OperationJournal::memory(),
            ));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Shift+Delete"));
        let delete_preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(delete_preview.contains("operation: Delete"));
        assert!(delete_preview.contains("Arm irreversible operation"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(root.join("delete.txt").exists());
        let armed = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(armed.contains("CONFIRM irreversible operation"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(!root.join("delete.txt").exists());

        let wipe_index = workspace
            .left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "wipe.bin")
            .unwrap();
        workspace.left.set_cursor(wipe_index);
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+Delete"));
        let wipe_dialog = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(wipe_dialog.contains("SSD/COW recovery is not guaranteed"));
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.wipe-confirmed"),
            arguments: BTreeMap::from([(
                "passes".to_owned(),
                CommandValue::String("2".to_owned()),
            )]),
        });
        let wipe_preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(wipe_preview.contains("operation: Wipe"));
        assert!(wipe_preview.contains("passes"));
        assert!(wipe_preview.contains('2'));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(root.join("wipe.bin").exists());
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(!root.join("wipe.bin").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_dialog_previews_and_executes_single_and_template_multi_rename() {
        let root = std::env::temp_dir().join(format!("near-fm-rename-{}", std::process::id()));
        let source = root.join("source");
        let peer = root.join("peer");
        let trash = root.join("Trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&peer).unwrap();
        fs::write(source.join("alpha.txt"), b"alpha").unwrap();
        fs::write(source.join("beta.md"), b"beta").unwrap();
        fs::write(source.join("first.txt"), b"old target").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &source, "rename.source");
        let right = filesystem_collection(provider.as_ref(), &peer, "rename.peer");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                trash,
                OperationJournal::memory(),
            ));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.rename".into(),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.dialog");
        let dialog = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(dialog.contains("Rename"));
        assert!(dialog.contains("alpha.txt"));
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.rename-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "template".to_owned(),
                    CommandValue::String("first.txt".to_owned()),
                ),
                ("start".to_owned(), CommandValue::String("1".to_owned())),
            ]),
        });
        assert_eq!(
            workspace.active_contexts()[0].as_str(),
            "surface.operation-preview"
        );
        let preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(preview.contains("operation: Rename"));
        assert!(preview.contains("conflicts: 1"));
        assert!(preview.contains("recovery: Backup"));
        workspace.handle_terminal_event(&mut keymap, key("r"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(source.join("first.txt").exists());
        assert_eq!(fs::read(source.join("first.txt")).unwrap(), b"alpha");
        assert!(!source.join("alpha.txt").exists());

        assert_eq!(
            workspace
                .left
                .select_by_masks("first.txt;beta.md", "", true),
            2
        );
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.rename-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "template".to_owned(),
                    CommandValue::String("{index}_{stem}{dotext}".to_owned()),
                ),
                ("start".to_owned(), CommandValue::String("7".to_owned())),
            ]),
        });
        let preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(preview.contains("operation: Rename"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert!(source.join("7_beta.md").exists());
        assert!(source.join("8_first.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multi_rename_rejects_duplicate_and_unsafe_targets_before_planning() {
        let mut workspace = FarWorkspace::demo();
        assert_eq!(workspace.left.select_by_masks("crates;docs", "", true), 2);
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.rename-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "template".to_owned(),
                    CommandValue::String("same".to_owned()),
                ),
                ("start".to_owned(), CommandValue::String("1".to_owned())),
            ]),
        });
        assert!(workspace.status.contains("maps both"));
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.rename-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "template".to_owned(),
                    CommandValue::String("../escape".to_owned()),
                ),
                ("start".to_owned(), CommandValue::String("1".to_owned())),
            ]),
        });
        assert!(workspace.status.contains("path separator"));
    }

    #[cfg(unix)]
    #[test]
    fn link_dialog_creates_typed_links_and_rejects_unsupported_sources_before_execution() {
        let root = std::env::temp_dir().join(format!("near-fm-link-{}", std::process::id()));
        let peer = root.join("peer");
        let trash = root.join("Trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&peer).unwrap();
        fs::create_dir_all(root.join("folder")).unwrap();
        fs::write(root.join("target.txt"), b"target").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &root, "link.source");
        let target_index = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "target.txt")
            .unwrap();
        left.set_cursor(target_index);
        let right = filesystem_collection(provider.as_ref(), &peer, "link.peer");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                trash,
                OperationJournal::memory(),
            ));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Alt+F6"));
        let dialog = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(dialog.contains("Create Link"));
        assert!(dialog.contains("hard | symbolic | junction"));
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.link-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "name".to_owned(),
                    CommandValue::String("shortcut".to_owned()),
                ),
                (
                    "type".to_owned(),
                    CommandValue::String("symbolic".to_owned()),
                ),
            ]),
        });
        let preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(preview.contains("operation: SymbolicLink"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        let link = workspace
            .left
            .entries()
            .iter()
            .find(|entry| entry.metadata.name == "shortcut")
            .unwrap();
        assert_eq!(link.metadata.kind, ResourceKind::Symlink);
        assert_eq!(
            LocalFileProvider::path(link.metadata.link_target.as_ref().unwrap()).unwrap(),
            root.join("target.txt")
        );

        let folder_index = workspace
            .left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "folder")
            .unwrap();
        workspace.left.set_cursor(folder_index);
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.link-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "name".to_owned(),
                    CommandValue::String("folder-hard".to_owned()),
                ),
                ("type".to_owned(), CommandValue::String("hard".to_owned())),
            ]),
        });
        assert!(
            workspace
                .status
                .contains("hard links require a regular file")
        );
        assert!(workspace.overlay.is_none());

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.link-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "name".to_owned(),
                    CommandValue::String("folder-junction".to_owned()),
                ),
                (
                    "type".to_owned(),
                    CommandValue::String("junction".to_owned()),
                ),
            ]),
        });
        let preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(preview.contains("operation: SymbolicLink"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn attributes_dialog_previews_and_applies_recursive_metadata_updates() {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        let root = std::env::temp_dir().join(format!("near-fm-attributes-{}", std::process::id()));
        let peer = root.join("peer");
        let folder = root.join("folder");
        let child = folder.join("child.txt");
        let trash = root.join("Trash");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&peer).unwrap();
        fs::create_dir_all(&folder).unwrap();
        fs::write(&child, b"child").unwrap();
        let child_metadata = fs::metadata(&child).unwrap();
        let owner = child_metadata.uid().to_string();
        let group = child_metadata.gid().to_string();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &root, "attributes.source");
        let folder_index = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "folder")
            .unwrap();
        left.set_cursor(folder_index);
        let right = filesystem_collection(provider.as_ref(), &peer, "attributes.peer");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(LocalOperationService::new(
                trash,
                OperationJournal::memory(),
            ));
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+A"));
        let dialog = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(dialog.contains("Attributes, Ownership, and Timestamps"));
        assert!(dialog.contains("Apply recursively"));
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.attributes-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "readonly".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
                (
                    "unix_mode".to_owned(),
                    CommandValue::String("755".to_owned()),
                ),
                ("owner".to_owned(), CommandValue::String(owner)),
                ("group".to_owned(), CommandValue::String(group)),
                (
                    "modified".to_owned(),
                    CommandValue::String("1000".to_owned()),
                ),
                (
                    "accessed".to_owned(),
                    CommandValue::String("2000".to_owned()),
                ),
                (
                    "recursive".to_owned(),
                    CommandValue::String("yes".to_owned()),
                ),
            ]),
        });
        let preview = workspace.snapshot(&theme, &keymap, 120, 30).join("\n");
        assert!(preview.contains("operation: SetAttributes"));
        assert!(preview.contains("sources: 2"));
        assert!(preview.contains("recovery: JournalOnly"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);

        assert_eq!(
            fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
            0o555
        );
        assert_eq!(
            fs::metadata(&child).unwrap().permissions().mode() & 0o777,
            0o555
        );
        assert_eq!(
            fs::metadata(&child)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            1_000
        );
        assert_eq!(
            fs::metadata(&child)
                .unwrap()
                .accessed()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            2_000
        );
        let folder_entry = workspace
            .left
            .entries()
            .iter()
            .find(|entry| entry.metadata.name == "folder")
            .unwrap();
        assert!(folder_entry.metadata.permissions.as_ref().unwrap().readonly);

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.attributes-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "readonly".to_owned(),
                    CommandValue::String("keep".to_owned()),
                ),
                (
                    "unix_mode".to_owned(),
                    CommandValue::String("999".to_owned()),
                ),
                ("owner".to_owned(), CommandValue::String(String::new())),
                ("group".to_owned(), CommandValue::String(String::new())),
                ("modified".to_owned(), CommandValue::String(String::new())),
                ("accessed".to_owned(), CommandValue::String(String::new())),
                (
                    "recursive".to_owned(),
                    CommandValue::String("no".to_owned()),
                ),
            ]),
        });
        assert!(workspace.status.contains("octal"));
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&child, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    struct SlowOperationService {
        item: Option<PlannedItem>,
        started: Arc<AtomicBool>,
    }

    struct CapturingOperationService {
        intents: Arc<Mutex<Vec<OperationIntent>>>,
    }

    #[derive(Default)]
    struct PartialLocalOperationService {
        items: Vec<PlannedItem>,
    }

    impl OperationService for CapturingOperationService {
        fn plan(
            &mut self,
            intent: OperationIntent,
            _generation: ListingGeneration,
        ) -> Result<OperationPlan, String> {
            self.intents.lock().unwrap().push(intent);
            Err("captured operation intent".to_owned())
        }

        fn execute(
            &mut self,
            _plan: &OperationId,
            _authorization: ExecutionAuthorization,
            _cancellation: &CancellationToken,
            _conflict: ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            Err("execution is not used by this test".to_owned())
        }
    }

    #[test]
    fn completed_trash_exposes_exact_restore_intent() {
        let intents = Arc::new(Mutex::new(Vec::new()));
        let mut workspace =
            FarWorkspace::demo().with_operation_service(CapturingOperationService {
                intents: Arc::clone(&intents),
            });
        let original = ResourceRef {
            provider: "near.local-fs".into(),
            location: Location::new("file:///tmp/original/report.txt"),
        };
        let trashed = Location::new("file:///Users/test/.Trash/report 2.txt");
        let plan = OperationId::from("trash-plan");
        workspace.operation_contexts.insert(
            77,
            ElevatedRetry {
                plan: plan.clone(),
                authorization: ExecutionAuthorization {
                    context_generation: workspace.generation,
                    confirmed: true,
                    high_impact_confirmed: false,
                },
                conflict: ConflictDecision {
                    action: ConflictAction::Rename,
                    scope: DecisionScope::Remaining,
                },
                elevated: false,
            },
        );
        workspace.finish_operation_task(
            77,
            Ok(ExecutionSummary {
                plan,
                kind: OperationKind::Trash,
                items: vec![ItemOutcome {
                    item: PlannedItem {
                        source: Some(original.clone()),
                        target: trashed.clone(),
                        conflict_expected: false,
                        recursive: false,
                        parameters: BTreeMap::new(),
                    },
                    status: ItemStatus::Completed,
                }],
                cancelled: false,
            }),
        );

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.restore-last-trash".into(),
            arguments: BTreeMap::new(),
        });

        let captured = intents.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(matches!(
            &captured[0],
            OperationIntent::Restore { items }
                if items == &vec![(
                    ResourceRef {
                        provider: original.provider,
                        location: trashed,
                    },
                    original.location,
                )]
        ));
    }

    impl OperationService for PartialLocalOperationService {
        fn plan(
            &mut self,
            intent: OperationIntent,
            generation: ListingGeneration,
        ) -> Result<OperationPlan, String> {
            let OperationIntent::Trash { sources } = intent else {
                return Err("expected trash intent".to_owned());
            };
            self.items = sources
                .into_iter()
                .map(|source| PlannedItem {
                    target: source.location.clone(),
                    source: Some(source),
                    conflict_expected: false,
                    recursive: false,
                    parameters: BTreeMap::new(),
                })
                .collect();
            OperationPlanner::default()
                .plan(PlanRequest {
                    kind: OperationKind::Trash,
                    items: self.items.clone(),
                    destination: None,
                    policies: PlanPolicies::default(),
                    safety: SafetyClass::Destructive,
                    context_generation: generation,
                    high_impact: false,
                })
                .map_err(|error| error.to_string())
        }

        fn execute(
            &mut self,
            plan: &OperationId,
            _authorization: ExecutionAuthorization,
            _cancellation: &CancellationToken,
            _conflict: ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            let mut outcomes = Vec::new();
            for (index, item) in self.items.iter().cloned().enumerate() {
                let status = if index == 0 {
                    let source = item.source.as_ref().ok_or("missing source")?;
                    fs::remove_file(LocalFileProvider::path(&source.location).map_err(
                        |error| format!("cannot map local source for partial operation: {error}"),
                    )?)
                    .map_err(|error| error.to_string())?;
                    ItemStatus::Completed
                } else {
                    ItemStatus::Failed("simulated partial failure".to_owned())
                };
                outcomes.push(ItemOutcome { item, status });
            }
            Ok(ExecutionSummary {
                plan: plan.clone(),
                kind: OperationKind::Trash,
                items: outcomes,
                cancelled: false,
            })
        }
    }

    struct PermissionOperationService {
        item: PlannedItem,
        elevated: Arc<Mutex<Vec<OperationId>>>,
    }

    impl OperationService for PermissionOperationService {
        fn plan(
            &mut self,
            _intent: OperationIntent,
            _generation: ListingGeneration,
        ) -> Result<OperationPlan, String> {
            Err("planning is not used by this test".to_owned())
        }

        fn execute(
            &mut self,
            plan: &OperationId,
            _authorization: ExecutionAuthorization,
            _cancellation: &CancellationToken,
            _conflict: ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            Ok(ExecutionSummary {
                plan: plan.clone(),
                kind: OperationKind::Copy,
                items: vec![ItemOutcome {
                    item: self.item.clone(),
                    status: ItemStatus::Failed("permission denied".to_owned()),
                }],
                cancelled: false,
            })
        }

        fn execute_elevated(
            &mut self,
            plan: &OperationId,
            _authorization: ExecutionAuthorization,
            _conflict: ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            self.elevated.lock().unwrap().push(plan.clone());
            Ok(ExecutionSummary {
                plan: plan.clone(),
                kind: OperationKind::Copy,
                items: vec![ItemOutcome {
                    item: self.item.clone(),
                    status: ItemStatus::Completed,
                }],
                cancelled: false,
            })
        }
    }

    #[test]
    fn permission_failure_offers_exact_plan_elevation_retry() {
        let plan = OperationId::from("permission-plan");
        let item = PlannedItem {
            source: None,
            target: Location::new("file:///protected"),
            conflict_expected: false,
            recursive: false,
            parameters: BTreeMap::new(),
        };
        let elevated = Arc::new(Mutex::new(Vec::new()));
        let mut workspace =
            FarWorkspace::demo().with_operation_service(PermissionOperationService {
                item,
                elevated: Arc::clone(&elevated),
            });
        workspace.start_operation(plan.clone(), ConflictAction::Skip, true, false);
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        assert!(workspace.elevated_retry.is_some());
        assert!(workspace.status.contains("retry-elevated"));
        workspace.dispatch(&CommandInvocation {
            id: "near.operation.retry-elevated".into(),
            arguments: BTreeMap::new(),
        });
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        assert_eq!(elevated.lock().unwrap().as_slice(), &[plan]);
        assert!(workspace.elevated_retry.is_none());
    }

    impl OperationService for SlowOperationService {
        fn plan(
            &mut self,
            intent: OperationIntent,
            generation: ListingGeneration,
        ) -> Result<OperationPlan, String> {
            let (source, destination) = match intent {
                OperationIntent::CopyTo {
                    mut sources,
                    destination,
                } => (sources.pop().ok_or("missing source")?, destination),
                _ => return Err("unexpected intent".to_owned()),
            };
            let item = PlannedItem {
                source: Some(source),
                target: destination,
                conflict_expected: false,
                recursive: false,
                parameters: BTreeMap::new(),
            };
            self.item = Some(item.clone());
            OperationPlanner::default()
                .plan(PlanRequest {
                    kind: OperationKind::Copy,
                    items: vec![item],
                    destination: None,
                    policies: PlanPolicies::default(),
                    safety: near_core::SafetyClass::Confirmable,
                    context_generation: generation,
                    high_impact: false,
                })
                .map_err(|error| error.to_string())
        }

        fn execute(
            &mut self,
            plan: &near_core::OperationId,
            _authorization: ExecutionAuthorization,
            cancellation: &CancellationToken,
            _conflict: ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            self.started.store(true, Ordering::Release);
            for _ in 0..100 {
                if cancellation.is_cancelled() {
                    return Ok(ExecutionSummary {
                        plan: plan.clone(),
                        kind: OperationKind::Copy,
                        items: vec![ItemOutcome {
                            item: self.item.clone().ok_or("missing item")?,
                            status: ItemStatus::Pending,
                        }],
                        cancelled: true,
                    });
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(ExecutionSummary {
                plan: plan.clone(),
                kind: OperationKind::Copy,
                items: vec![ItemOutcome {
                    item: self.item.clone().ok_or("missing item")?,
                    status: ItemStatus::Completed,
                }],
                cancelled: false,
            })
        }
    }

    #[test]
    fn operation_execution_remains_responsive_and_cancellable() {
        let started_execution = Arc::new(AtomicBool::new(false));
        let mut workspace = FarWorkspace::demo().with_operation_service(SlowOperationService {
            item: None,
            started: Arc::clone(&started_execution),
        });
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.handle_terminal_event(&mut keymap, key("F5"));
        let started = Instant::now();
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(started.elapsed() < Duration::from_millis(20));
        let task = workspace.operation_task.as_ref().unwrap().id().0;
        while !started_execution.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        workspace.dispatch(&CommandInvocation {
            id: "near.task.cancel".into(),
            arguments: BTreeMap::from([(
                "task".to_owned(),
                CommandValue::String(task.to_string()),
            )]),
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while workspace.operation_task.is_some() && Instant::now() < deadline {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        let record = workspace.task_records.get(&task).unwrap();
        assert_eq!(record.state, TaskState::Cancelled);
        assert!(record.message.contains("1 pending"));
        let export = workspace.diagnostic_export();
        let task_event = export
            .events
            .iter()
            .find(|event| event.domain == DiagnosticDomain::Task && event.parent.is_some())
            .unwrap();
        assert!(export.events.iter().any(|event| {
            event.domain == DiagnosticDomain::Command
                && Some(event.correlation) == task_event.parent
        }));
        assert!(export.events.iter().any(|event| {
            event.domain == DiagnosticDomain::Operation
                && matches!(
                    event.phase,
                    DiagnosticPhase::Completed | DiagnosticPhase::Cancelled
                )
        }));
        assert_eq!(export.near_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn protected_filesystem_root_shows_blocking_denial_before_operation_planning() {
        let provider = Arc::new(LocalFileProvider);
        let resource = LocalFileProvider::resource_for_path(std::path::Path::new("/"));
        let metadata = block_on_provider(provider.stat(&resource)).unwrap();
        let left = CollectionSurface::new(
            "protected.left",
            "workspace.panel",
            "Protected root",
            LocalFileProvider::location(std::path::Path::new("/Volumes")),
            vec![CollectionEntry::new(
                resource.clone(),
                metadata,
                "filesystem root",
            )],
        );
        let right = demo_collection(
            "protected.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let intents = Arc::new(Mutex::new(Vec::new()));
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(CapturingOperationService {
                intents: Arc::clone(&intents),
            });
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        for command in [
            "near.resource.trash",
            "near.resource.delete",
            "near.resource.wipe",
        ] {
            workspace.dispatch(&CommandInvocation {
                id: CommandId::from(command),
                arguments: BTreeMap::new(),
            });
            assert!(intents.lock().unwrap().is_empty());
            let denial = workspace.snapshot(&theme, &keymap, 100, 28).join("\n");
            assert!(
                denial.contains("did not create an operation plan"),
                "{denial}"
            );
            assert!(denial.contains("filesystem root"), "{denial}");
            workspace.dispatch(&CommandInvocation {
                id: CommandId::from("near.overlay.cancel"),
                arguments: BTreeMap::new(),
            });
        }
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires the disposable mounted filesystem harness"]
    fn mounted_volume_delete_workflow_shows_denial_before_operation_planning() {
        let mount_root = std::env::var_os("NEAR_TEST_MOUNT_ROOT")
            .map(std::path::PathBuf::from)
            .expect("NEAR_TEST_MOUNT_ROOT must name the disposable mounted volume");
        let provider = Arc::new(LocalFileProvider);
        let resource = LocalFileProvider::resource_for_path(&mount_root);
        let metadata = block_on_provider(provider.stat(&resource)).unwrap();
        let left = CollectionSurface::new(
            "mount.left",
            "workspace.panel",
            "Mounted volume",
            LocalFileProvider::location(mount_root.parent().unwrap()),
            vec![CollectionEntry::new(
                resource,
                metadata,
                "mounted volume root",
            )],
        );
        let right = demo_collection(
            "mount.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let intents = Arc::new(Mutex::new(Vec::new()));
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(CapturingOperationService {
                intents: Arc::clone(&intents),
            });
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.delete"),
            arguments: BTreeMap::new(),
        });

        assert!(intents.lock().unwrap().is_empty());
        let denial = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(denial.contains("Cannot Delete permanently"), "{denial}");
        assert!(denial.contains("mounted volume"), "{denial}");
        assert!(denial.contains("Hotplug devices"), "{denial}");
        assert!(mount_root.join("near-mount-sentinel.txt").is_file());
    }

    #[test]
    fn operation_target_conformance_shift_function_keys_ignore_selection() {
        let left = demo_collection(
            "targets.left",
            "Targets",
            "/targets",
            vec![
                CollectionItem::file("alpha.txt", "file"),
                CollectionItem::file("beta.txt", "file"),
                CollectionItem::file("gamma.txt", "file"),
            ],
            1,
        );
        let right = demo_collection(
            "targets.right",
            "Peer",
            "/peer",
            vec![CollectionItem::file("peer.txt", "file")],
            0,
        );
        let intents = Arc::new(Mutex::new(Vec::new()));
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(Arc::new(PermissiveDemoProvider))
            .with_operation_service(CapturingOperationService {
                intents: Arc::clone(&intents),
            });
        workspace.left.set_cursor(0);
        workspace.left.toggle_selection();
        workspace.left.set_cursor(2);
        workspace.left.toggle_selection();
        workspace.left.set_cursor(1);
        let current = workspace.left.current().unwrap().resource.clone();
        let selected = workspace.left.selected_resources();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F5"));
        workspace.handle_terminal_event(&mut keymap, key("Shift+F5"));
        workspace.handle_terminal_event(&mut keymap, key("Shift+F6"));
        workspace.handle_terminal_event(&mut keymap, key("Shift+F8"));

        let intents = intents.lock().unwrap();
        assert!(matches!(
            &intents[0],
            OperationIntent::CopyTo { sources, .. } if sources == &selected
        ));
        assert!(matches!(
            &intents[1],
            OperationIntent::CopyTo { sources, .. } if sources == &vec![current.clone()]
        ));
        assert!(matches!(
            &intents[2],
            OperationIntent::MoveTo { sources, .. } if sources == &vec![current.clone()]
        ));
        assert!(matches!(
            &intents[3],
            OperationIntent::Trash { sources } if sources == &vec![current]
        ));
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let semantic = workspace.semantic_snapshot(&theme, &keymap, 100, 30);
        assert!(
            semantic
                .role_lines()
                .join("\n")
                .contains("panel.item.selected")
        );
        assert_eq!(workspace.left.selected_resources(), selected);
    }

    #[test]
    fn operation_target_conformance_partial_failure_retains_failed_selection() {
        let fixture =
            std::env::temp_dir().join(format!("near-ui-partial-operation-{}", std::process::id()));
        let _ = fs::remove_dir_all(&fixture);
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("alpha.txt"), b"alpha").unwrap();
        fs::write(fixture.join("beta.txt"), b"beta").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &fixture, "partial.left");
        let right = filesystem_collection(provider.as_ref(), &fixture, "partial.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_operation_service(PartialLocalOperationService::default());
        workspace.left.set_cursor(0);
        workspace.left.toggle_selection();
        workspace.left.set_cursor(1);
        workspace.left.toggle_selection();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F8"));
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let preview = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(preview.contains("alpha.txt"));
        assert!(preview.contains("beta.txt"));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        while workspace.operation_task.is_some() {
            workspace.poll_background_tasks();
            std::thread::yield_now();
        }
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        wait_for_listing(&mut workspace, FocusedPanel::Right);

        assert!(!fixture.join("alpha.txt").exists());
        assert!(fixture.join("beta.txt").exists());
        assert_eq!(workspace.left.selected_resources().len(), 1);
        assert_eq!(
            workspace
                .left
                .entries()
                .iter()
                .find(|entry| entry.selected)
                .unwrap()
                .metadata
                .name,
            "beta.txt"
        );
        assert!(
            workspace
                .semantic_snapshot(&theme, &keymap, 100, 30)
                .role_lines()
                .join("\n")
                .contains("panel.item.selected")
        );
        assert!(workspace.task_records.values().any(|record| {
            record.state == TaskState::Failed && record.message.contains("1 failed")
        }));
        let Some(Overlay::Message { body, .. }) = &workspace.overlay else {
            panic!()
        };
        assert!(body.contains("beta.txt"));
        assert!(body.contains("simulated partial failure"));
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn external_edit_request_preserves_workspace_selection_and_focus() {
        let fixture = std::env::temp_dir().join(format!("near-ui-external-{}", std::process::id()));
        let _ = fs::remove_dir_all(&fixture);
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("one.txt"), b"one").unwrap();
        fs::write(fixture.join("two.txt"), b"two").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &fixture, "near-fm.left");
        let right = filesystem_collection(provider.as_ref(), &fixture, "near-fm.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_embedded_pty(false)
            .with_external_tool_resolver(LocalExternalToolResolver::new(
                "true",
                std::iter::empty::<&str>(),
            ));
        workspace.focused_panel_mut().move_cursor(1);
        workspace.focused_panel_mut().toggle_selection();
        let resource = workspace
            .focused_panel()
            .current()
            .unwrap()
            .resource
            .clone();
        let selected = workspace.canonical_targets();

        workspace.dispatch(&CommandInvocation {
            id: "near.terminal.open".into(),
            arguments: BTreeMap::new(),
        });
        let Some(Overlay::Message { body, .. }) = &workspace.overlay else {
            panic!("disabled embedded PTY should report its fallback");
        };
        assert!(body.contains("Embedded PTY support is disabled"));
        workspace.dispatch(&CommandInvocation {
            id: "near.overlay.cancel".into(),
            arguments: BTreeMap::new(),
        });

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.edit-external".into(),
            arguments: BTreeMap::new(),
        });
        let invocation = workspace.take_external_invocation().unwrap();

        assert_eq!(
            invocation.arguments.last(),
            Some(
                &LocalFileProvider::path(&resource.location)
                    .unwrap()
                    .into_os_string()
            )
        );
        assert_eq!(
            workspace.focused_panel().current().unwrap().resource,
            resource
        );
        assert_eq!(workspace.canonical_targets(), selected);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn viewer_and_editor_open_policies_route_to_external_tools() {
        let fixture = std::env::temp_dir().join(format!("near-ui-policy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&fixture);
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("one.txt"), b"one").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &fixture, "policy.left");
        let right = filesystem_collection(provider.as_ref(), &fixture, "policy.right");
        let mut editor_settings = EditorSettings::default();
        editor_settings.open_policy = ResourceOpenPolicy::External;
        let mut viewer_settings = ViewerSettings::default();
        viewer_settings.open_policy = ResourceOpenPolicy::External;
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_editor_settings(editor_settings)
            .with_viewer_settings(viewer_settings)
            .with_external_tool_resolver(LocalExternalToolResolver::new(
                "true",
                std::iter::empty::<&str>(),
            ));

        workspace.dispatch(&CommandInvocation {
            id: "near.resource.view".into(),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.take_external_invocation().is_some());
        assert!(workspace.overlay.is_none());
        workspace.dispatch(&CommandInvocation {
            id: "near.resource.edit".into(),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.take_external_invocation().is_some());
        assert!(workspace.editors.is_empty());
        workspace.settings.editor.open_policy = ResourceOpenPolicy::Internal;
        workspace.settings.editor.tab_size = 3;
        workspace.settings.editor.expand_tabs = true;
        workspace.dispatch(&CommandInvocation::new("near.resource.edit"));
        workspace.dispatch(&CommandInvocation::new("near.editor.insert-tab"));
        workspace.dispatch(&CommandInvocation::new("near.editor.save"));
        assert_eq!(fs::read(fixture.join("one.txt")).unwrap(), b"   one");
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn enter_uses_open_association_while_f3_remains_the_viewer() {
        let fixture =
            std::env::temp_dir().join(format!("near-ui-enter-open-{}", std::process::id()));
        let _ = fs::remove_dir_all(&fixture);
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("document.txt"), b"viewer-content").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &fixture, "enter-open.left");
        let right = filesystem_collection(provider.as_ref(), &fixture, "enter-open.right");
        let document = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "document.txt")
            .unwrap();
        left.set_cursor(document);
        let resolver = LocalExternalToolResolver::from_toml(
            r#"
schema = 1

[[handlers]]
id = "default-open"
actions = ["open"]
[handlers.predicate]
schema_version = 1
hidden = "include"
ignore = "none"
[handlers.invocation]
mode = "argv"
program = "open-with-default-application"
arguments = [{ value = "native-path" }]
"#,
        )
        .unwrap();
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_external_tool_resolver(resolver);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let theme = SemanticTheme::from_toml(THEME).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        let invocation = workspace.take_external_invocation().unwrap();
        assert_eq!(
            invocation.program.to_string_lossy(),
            "open-with-default-application"
        );
        assert!(workspace.overlay.is_none());

        workspace.handle_terminal_event(&mut keymap, key("F3"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.viewer");
        assert!(
            workspace
                .snapshot(&theme, &keymap, 100, 30)
                .join("\n")
                .contains("viewer-content")
        );
        assert!(workspace.take_external_invocation().is_none());
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn enter_without_an_open_handler_is_denied_instead_of_falling_back_to_viewer() {
        let fixture =
            std::env::temp_dir().join(format!("near-ui-enter-denial-{}", std::process::id()));
        let _ = fs::remove_dir_all(&fixture);
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("document.txt"), b"must-not-open-in-viewer").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let mut left = filesystem_collection(provider.as_ref(), &fixture, "enter-denial.left");
        let right = filesystem_collection(provider.as_ref(), &fixture, "enter-denial.right");
        let document = left
            .entries()
            .iter()
            .position(|entry| entry.metadata.name == "document.txt")
            .unwrap();
        left.set_cursor(document);
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Enter"));

        assert_eq!(workspace.active_contexts()[0].as_str(), "workspace.panel");
        assert_eq!(workspace.status, "No external tool resolver is configured");
        assert!(workspace.overlay.is_none());
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn configured_user_menu_entries_activate_through_the_menu_route() {
        let root = std::env::temp_dir().join(format!("near-user-menu-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("document.txt"), b"content").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "user-menu.left");
        let right = filesystem_collection(provider.as_ref(), &root, "user-menu.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_user_menus(UserMenuCatalog::from_toml(USER_MENU).unwrap());
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: "near.user-menu.global".into(),
            arguments: BTreeMap::new(),
        });
        let Some(Overlay::Menu(menu)) = &workspace.overlay else {
            panic!("global user menu did not open");
        };
        assert_eq!(menu.title(), "Global User Menu");
        assert_eq!(menu.items().len(), 1);
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        let global = workspace.take_external_invocation().unwrap();
        assert_eq!(global.program.to_string_lossy(), "/usr/bin/printf");

        workspace.dispatch(&CommandInvocation {
            id: "near.user-menu.local".into(),
            arguments: BTreeMap::new(),
        });
        let Some(Overlay::Menu(menu)) = &workspace.overlay else {
            panic!("local user menu did not open");
        };
        assert_eq!(menu.title(), "Local User Menu");
        assert_eq!(menu.items().len(), 1);
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        let local = workspace.take_external_invocation().unwrap();
        assert_eq!(local.program.to_string_lossy(), "/bin/cat");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_association_menu_exposes_ordered_execute_alternatives_and_named_selection() {
        let root = std::env::temp_dir().join(format!("near-association-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("run.sh"), b"#!/bin/sh\n").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "association.left");
        let right = filesystem_collection(provider.as_ref(), &root, "association.right");
        let resolver = LocalExternalToolResolver::from_toml(
            r#"
schema = 1

[[handlers]]
id = "primary"
actions = ["execute"]
[handlers.predicate]
schema_version = 1
name = { match = "glob", value = "*.sh" }
hidden = "include"
ignore = "none"
[handlers.invocation]
mode = "argv"
program = "primary-runner"
arguments = [{ value = "native-path" }]

[[handlers]]
id = "alternative"
actions = ["execute"]
[handlers.predicate]
schema_version = 1
name = { match = "glob", value = "*.sh" }
hidden = "include"
ignore = "none"
[handlers.invocation]
mode = "argv"
program = "alternative-runner"
arguments = [{ value = "native-path" }]
"#,
        )
        .unwrap();
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_external_tool_resolver(resolver);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.associations"),
            arguments: BTreeMap::new(),
        });
        let menu = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(menu.contains("Execute — primary"));
        assert!(menu.contains("Execute — alternative"));
        assert!(menu.contains("structured argv"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.association-run"),
            arguments: BTreeMap::from([
                (
                    "action".to_owned(),
                    CommandValue::String("execute".to_owned()),
                ),
                (
                    "handler".to_owned(),
                    CommandValue::String("alternative".to_owned()),
                ),
            ]),
        });
        let invocation = workspace.take_external_invocation().unwrap();
        assert_eq!(
            invocation.program,
            std::ffi::OsString::from("alternative-runner")
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.handler.diagnostics"),
            arguments: BTreeMap::new(),
        });
        let Some(Overlay::Message {
            body: diagnostics, ..
        }) = &workspace.overlay
        else {
            panic!("handler diagnostics should open a message");
        };
        assert!(diagnostics.contains("Execute"));
        assert!(diagnostics.contains("1. primary [structured argv]"));
        assert!(diagnostics.contains("2. alternative [structured argv]"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn semantic_macro_replay_survives_key_rebinding_and_is_inspectable() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let initial = workspace.focused_panel().cursor();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+."));
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+."));
        assert_eq!(workspace.focused_panel().cursor(), initial + 1);
        assert_eq!(workspace.last_macro.as_ref().unwrap().steps.len(), 1);

        keymap
            .reload_from_toml(&KEYMAP.replace("on = \"Down\"", "on = \"Ctrl+N\""))
            .unwrap();
        workspace.handle_terminal_event(&mut keymap, key("Down"));
        assert_eq!(workspace.focused_panel().cursor(), initial + 1);
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Shift+."));
        assert_eq!(workspace.focused_panel().cursor(), initial + 2);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.macro.show-last"),
            arguments: BTreeMap::new(),
        });
        let Some(Overlay::Message { body, .. }) = &workspace.overlay else {
            panic!("macro inspection should open a message overlay");
        };
        assert!(body.contains("schema = 2"));
        assert!(body.contains("near.collection.move"));
    }

    #[test]
    fn untrusted_macro_cannot_replay_destructive_commands() {
        let mut workspace = FarWorkspace::demo();
        workspace.last_macro = Some(SemanticMacro {
            id: "test.destructive".to_owned(),
            title: "Destructive".to_owned(),
            binding: None,
            trust: MacroTrust::Untrusted,
            when: MacroCondition::default(),
            steps: vec![MacroStep {
                invocation: CommandInvocation {
                    id: CommandId::from("near.resource.trash"),
                    arguments: BTreeMap::new(),
                },
                when: MacroCondition::default(),
            }],
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.macro.play-last"),
            arguments: BTreeMap::new(),
        });
        assert!(workspace.status.contains("is not authorized"));
        assert!(workspace.overlay.is_none());
    }

    #[test]
    fn macro_manager_edits_binds_diagnoses_replays_deletes_and_persists() {
        let store = TestMacroStore::default();
        let saved = store.0.clone();
        let semantic_macro = SemanticMacro {
            id: "test.move".to_owned(),
            title: "Move down".to_owned(),
            binding: None,
            trust: MacroTrust::Untrusted,
            when: MacroCondition {
                required_contexts: vec![ContextId::from("workspace.panel")],
                ..MacroCondition::default()
            },
            steps: vec![MacroStep {
                invocation: CommandInvocation {
                    id: "near.collection.move".into(),
                    arguments: BTreeMap::from([("rows".to_owned(), CommandValue::Integer(1))]),
                },
                when: MacroCondition::default(),
            }],
        };
        let mut workspace = FarWorkspace::demo()
            .with_macros([semantic_macro])
            .with_macro_store(store);
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: "near.macro.manage".into(),
            arguments: BTreeMap::new(),
        });
        let manager = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(manager.contains("Macro Manager"));
        assert!(manager.contains("Move down"));

        workspace.dispatch(&CommandInvocation {
            id: "near.macro.edit-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "id".to_owned(),
                    CommandValue::String("test.move".to_owned()),
                ),
                (
                    "title".to_owned(),
                    CommandValue::String("Navigate down".to_owned()),
                ),
                (
                    "trust".to_owned(),
                    CommandValue::String("untrusted".to_owned()),
                ),
                (
                    "contexts".to_owned(),
                    CommandValue::String("workspace.panel".to_owned()),
                ),
                (
                    "capabilities".to_owned(),
                    CommandValue::String(String::new()),
                ),
                (
                    "current".to_owned(),
                    CommandValue::String("present".to_owned()),
                ),
                (
                    "peer".to_owned(),
                    CommandValue::String("present".to_owned()),
                ),
            ]),
        });
        assert_eq!(workspace.macro_catalog["test.move"].title, "Navigate down");

        workspace.dispatch(&CommandInvocation {
            id: "near.macro.bind-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "id".to_owned(),
                    CommandValue::String("test.move".to_owned()),
                ),
                (
                    "binding".to_owned(),
                    CommandValue::String("Ctrl+Alt+M".to_owned()),
                ),
            ]),
        });
        assert_eq!(
            workspace.macro_catalog["test.move"].binding.as_deref(),
            Some("Ctrl+Alt+m")
        );
        let initial = workspace.focused_panel().cursor();
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+M"));
        assert_eq!(workspace.focused_panel().cursor(), initial + 1);

        workspace.dispatch(&macro_invocation("near.macro.diagnose", "test.move"));
        let Some(Overlay::Message { body, .. }) = &workspace.overlay else {
            panic!("macro diagnostics should be visible");
        };
        assert!(body.contains("Available now: true"));
        assert!(body.contains("near.collection.move"));
        assert!(body.contains("authorized=true"));

        workspace.dispatch(&CommandInvocation {
            id: "near.macro.delete-confirmed".into(),
            arguments: BTreeMap::from([
                (
                    "id".to_owned(),
                    CommandValue::String("test.move".to_owned()),
                ),
                ("confirm".to_owned(), CommandValue::String("yes".to_owned())),
            ]),
        });
        assert!(workspace.macro_catalog.is_empty());
        assert!(saved.lock().unwrap().as_ref().unwrap().macros.is_empty());
    }

    #[test]
    fn surface_gallery_routes_filtering_navigation_and_text_input() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F9"));
        workspace.handle_terminal_event(&mut keymap, key("Right"));
        workspace.handle_terminal_event(&mut keymap, key("Right"));
        for character in ["c", "o", "m", "m"] {
            workspace.handle_terminal_event(&mut keymap, key(&format!("Alt+{character}")));
        }
        let menu = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(menu.contains("[C]ommand history"));
        assert!(!menu.contains("Inspector"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.overlay.cancel"),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.demo.terminal"),
            arguments: BTreeMap::new(),
        });
        workspace.handle_terminal_event(&mut keymap, key("x"));
        assert_eq!(workspace.status, "Terminal input: \"x\"");
    }

    #[test]
    fn user_screen_retains_output_and_restores_the_previous_full_screen_surface() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let mut terminal = TerminalSurface::new("test.user-screen", "User Screen", 100);
        terminal.append_output("retained shell output");
        workspace.install_test_terminal("User Screen", Box::new(terminal));
        workspace.overlay = Some(Overlay::Surface(Box::new(ViewerSurface::text(
            "test.viewer",
            "Viewer",
            "retained viewer screen",
        ))));

        let action = workspace.handle_terminal_event(&mut keymap, key("Ctrl+O"));
        assert!(
            matches!(action, WorkspaceAction::Command(ref invocation) if invocation.id.as_str() == "near.terminal.open"),
            "{action:?}"
        );
        assert!(workspace.terminal_is_full_screen(), "{}", workspace.status);
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.terminal");
        let terminal_frame = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(terminal_frame.contains("retained shell output"));
        assert!(!terminal_frame.contains("retained viewer screen"));

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+O"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.viewer");
        let viewer_frame = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(viewer_frame.contains("retained viewer screen"));
        assert!(!workspace.terminals.is_empty());
    }

    #[test]
    fn user_screen_participates_in_screen_list_and_cycle_order() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.install_test_terminal(
            "User Screen",
            Box::new(TerminalSurface::new("test.user-screen", "User Screen", 100)),
        );

        workspace.handle_terminal_event(&mut keymap, key("F12"));
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let screens = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(screens.contains("[1] Panels"));
        assert!(screens.contains("[2] Terminal: User Screen"));
        assert!(screens.contains("User Screen"));
        assert!(screens.contains("retained terminal"));

        workspace.handle_terminal_event(&mut keymap, key("2"));
        assert!(workspace.terminal_is_full_screen());
        workspace.handle_terminal_event(&mut keymap, key("F12"));

        workspace.dispatch(&CommandInvocation {
            id: "near.overlay.cancel".into(),
            arguments: BTreeMap::new(),
        });
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Tab"));
        assert!(!workspace.terminal_is_full_screen());
        assert!(workspace.active_editor.is_none());
    }

    #[test]
    fn terminal_tabs_cycle_inside_a_zoomable_peer_pane() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        let mut first = TerminalSurface::new("test.agent-one", "Agent One", 100);
        first.append_output("agent-one-output");
        workspace.install_test_terminal("Agent One", Box::new(first));
        let mut second = TerminalSurface::new("test.agent-two", "Agent Two", 100);
        second.append_output("agent-two-output");
        workspace.install_test_terminal("Agent Two", Box::new(second));

        workspace.place_terminal_in_pane(PaneSlot::Second);
        assert_eq!(workspace.terminal_pane(), Some(FocusedPanel::Right));
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.terminal");
        let pane = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(pane.contains("Agent One"));
        assert!(pane.contains("Agent Two"));
        assert!(pane.contains("agent-two-output"));
        assert!(pane.contains("Macintosh HD"));
        assert!(pane.contains("C-A-N New"));
        assert!(pane.contains("C-A-P Peer"));

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+PageUp"));
        let previous = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(previous.contains("agent-one-output"));
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+P"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "workspace.panel");
        workspace.handle_terminal_event(&mut keymap, key("Tab"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.terminal");

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+O"));
        assert!(workspace.terminal_is_full_screen());
        let zoomed = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(!zoomed.contains("Macintosh HD"));
        assert!(zoomed.contains("C-A-N New"));
        assert!(!zoomed.contains("C-A-P Peer"));
        let focused = workspace.focused;
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+Alt+P"));
        assert_eq!(workspace.focused, focused);
        assert!(workspace.status.starts_with("No peer pane is visible"));
        let denied = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(denied.contains("No peer pane is visible"));
        workspace.handle_terminal_event(&mut keymap, key("Ctrl+O"));
        assert_eq!(workspace.terminal_pane(), Some(FocusedPanel::Right));
        let restored = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(restored.contains("Macintosh HD"));
        assert!(restored.contains("agent-one-output"));
    }

    #[test]
    fn create_directory_uses_dialog_surface_and_returns_values() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("F7"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.dialog");
        workspace.handle_terminal_event(&mut keymap, TerminalEvent::Paste("Artifacts".to_owned()));
        let dialog = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(dialog.contains("Artifacts"));

        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert_eq!(workspace.active_contexts()[0].as_str(), "workspace.panel");
        assert_eq!(workspace.status, "No operation service is configured");
    }

    #[test]
    fn options_categories_filter_settings_and_escape_returns_to_parent_menu() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.menu.options"),
            arguments: BTreeMap::new(),
        });
        workspace.handle_terminal_event(&mut keymap, key("Enter"));

        let system = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(system.contains("System Settings"), "{system}");
        assert!(system.contains("Show status line"), "{system}");
        assert!(!system.contains("Open policy"), "{system}");

        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        let options = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(options.contains("Options Menu"), "{options}");
        assert!(options.contains("[S]ystem settings"), "{options}");
    }

    #[test]
    fn enter_edits_non_boolean_settings_and_escape_returns_to_filtered_catalog() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.show"),
            arguments: BTreeMap::from([(
                "category".to_owned(),
                CommandValue::String("Viewer".to_owned()),
            )]),
        });

        workspace.handle_terminal_event(&mut keymap, TerminalEvent::Paste("open policy".into()));
        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        let editor = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(editor.contains("Edit viewer.open_policy"), "{editor}");

        workspace.handle_terminal_event(&mut keymap, key("Esc"));
        let viewer = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(viewer.contains("Viewer Settings"), "{viewer}");
        assert!(viewer.contains("Find: open policy_"), "{viewer}");
        assert!(!viewer.contains("Editor Settings"), "{viewer}");
    }

    #[test]
    fn advanced_settings_are_hidden_toggleable_and_searchable_through_workspace_input() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.show"),
            arguments: BTreeMap::new(),
        });

        let hidden = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(!hidden.contains("Prefer physical keys"), "{hidden}");
        assert!(hidden.contains("F6 show advanced"), "{hidden}");

        workspace.handle_terminal_event(&mut keymap, key("F6"));
        let shown = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(shown.contains("Prefer physical keys"), "{shown}");
        assert!(shown.contains("F6 hide advanced"), "{shown}");

        workspace.handle_terminal_event(&mut keymap, key("F6"));
        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Paste("physical keys".to_owned()),
        );
        let searched = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(searched.contains("Prefer physical keys"), "{searched}");
        assert!(searched.contains("Find: physical keys_"), "{searched}");
    }

    #[test]
    fn startup_panel_setting_is_restart_scoped_and_only_new_workspaces_apply_it() {
        let mut workspace = FarWorkspace::demo();
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        assert_eq!(workspace.focused, FocusedPanel::Left);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.apply-candidate"),
            arguments: BTreeMap::from([
                (
                    "id".to_owned(),
                    CommandValue::String("interface.startup_panel".to_owned()),
                ),
                ("value".to_owned(), CommandValue::String("right".to_owned())),
            ]),
        });
        assert_eq!(workspace.focused, FocusedPanel::Left);
        assert_eq!(
            workspace.settings.interface.startup_panel,
            crate::StartupPanel::Right
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.show"),
            arguments: BTreeMap::from([(
                "category".to_owned(),
                CommandValue::String("Interface".to_owned()),
            )]),
        });
        workspace.handle_terminal_event(
            &mut keymap,
            TerminalEvent::Paste("startup panel".to_owned()),
        );
        let rendered = workspace.snapshot(&theme, &keymap, 110, 30).join("\n");
        assert!(rendered.contains("Startup panel"), "{rendered}");
        assert!(rendered.contains("scope=Restart"), "{rendered}");

        let restarted = FarWorkspace::demo().with_interface_settings(workspace.settings.interface);
        assert_eq!(restarted.focused, FocusedPanel::Right);
    }

    #[test]
    fn escape_closes_settings_even_without_a_keymap_binding() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(
            r#"
            [[context]]
            id = "global"
            "#,
        )
        .unwrap();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.show"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.active_contexts()[0].as_str(), "surface.settings");

        let action = workspace.handle_terminal_event(&mut keymap, key("Esc"));

        assert!(matches!(
            action,
            WorkspaceAction::Command(ref command)
                if command.id.as_str() == "near.overlay.cancel"
        ));
        assert_eq!(workspace.active_contexts()[0].as_str(), "workspace.panel");
    }

    #[test]
    fn typed_settings_are_searchable_editable_validated_and_actionable() {
        let mut workspace = FarWorkspace::demo();
        struct FailingStore;
        impl crate::SettingsDocumentStore for FailingStore {
            fn load(&self, _: &str) -> Result<Option<String>, String> {
                Ok(None)
            }
            fn persist(&self, _: &str, _: &str) -> Result<(), String> {
                Err("disk full".to_owned())
            }
        }
        workspace.settings.viewer.open_policy = ResourceOpenPolicy::Association;
        workspace.settings.store = Some(Arc::new(FailingStore));
        workspace.dispatch(&CommandInvocation {
            id: "near.settings.apply-candidate".into(),
            arguments: BTreeMap::from([
                (
                    "id".into(),
                    CommandValue::String("viewer.open_policy".into()),
                ),
                ("value".into(), CommandValue::String("internal".into())),
            ]),
        });
        let policy = workspace.settings.viewer.open_policy;
        assert_eq!(policy, ResourceOpenPolicy::Association);
        assert!(workspace.status.contains("cannot persist viewer.toml"));
    }

    #[test]
    fn descriptions_render_edit_and_open_folder_description_files() {
        let root =
            std::env::temp_dir().join(format!("near-description-workspace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("item.txt"), "body").unwrap();
        fs::write(root.join("descript.ion"), "item.txt Initial catalog text\n").unwrap();
        fs::write(root.join("README.md"), "Folder documentation").unwrap();
        let provider = Arc::new(DescribedLocalFileProvider::new(
            DescriptionSettings::default(),
        ));
        let left = filesystem_collection(provider.as_ref(), &root, "left");
        let right = filesystem_collection(provider.as_ref(), &root, "right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);
        let wide = PanelModeCatalog::from_toml(include_str!("../../../specs/panel-modes.toml"))
            .unwrap()
            .mode("wide")
            .unwrap()
            .clone();
        workspace.focused_panel_mut().set_view_mode(wide);
        let frame = workspace
            .snapshot(
                &SemanticTheme::from_toml(THEME).unwrap(),
                &Keymap::from_toml(KEYMAP).unwrap(),
                120,
                30,
            )
            .join("\n");
        assert!(frame.contains("Initial catalog"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.resource.description-confirmed"),
            arguments: BTreeMap::from([(
                "description".to_owned(),
                CommandValue::String("Updated catalog text".to_owned()),
            )]),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while workspace.status.starts_with("Updating descriptions") && Instant::now() < deadline {
            workspace.poll_background_tasks();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            fs::read_to_string(root.join("descript.ion"))
                .unwrap()
                .contains("Updated catalog text")
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.folder-description.view"),
            arguments: BTreeMap::new(),
        });
        let folder_frame = workspace
            .snapshot(
                &SemanticTheme::from_toml(THEME).unwrap(),
                &Keymap::from_toml(KEYMAP).unwrap(),
                100,
                30,
            )
            .join("\n");
        assert!(folder_frame.contains("Folder documentation"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registered_provider_prefix_navigates_before_shell_execution() {
        let root =
            std::env::temp_dir().join(format!("near-prefix-provider-{}", std::process::id()));
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "left");
        let right = filesystem_collection(provider.as_ref(), &root, "right");
        let mut workspace = FarWorkspace::new(left, right).with_provider(provider);

        workspace
            .command_line
            .set_buffer(format!("file:{}", child.display()));
        workspace.submit_command_line();

        assert_eq!(
            workspace
                .listing_state(FocusedPanel::Left)
                .unwrap()
                .location,
            LocalFileProvider::location(&child)
        );
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        assert_eq!(
            workspace.focused_panel().location(),
            &LocalFileProvider::location(&child)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registered_extension_prefix_receives_exact_argument_text() {
        let mut workspace = FarWorkspace::demo()
            .try_with_extension(Arc::new(TestExtension))
            .unwrap();
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.command-prefixes.show"),
            arguments: BTreeMap::new(),
        });
        let frame = workspace
            .snapshot(
                &SemanticTheme::from_toml(THEME).unwrap(),
                &Keymap::from_toml(KEYMAP).unwrap(),
                100,
                30,
            )
            .join("\n");
        assert!(frame.contains("hello:"));
        assert!(frame.contains("extension test.extension"));
        workspace.overlay = None;
        workspace.command_line.set_buffer("hello:spaced value");
        workspace.submit_command_line();
        assert_eq!(workspace.status, "Hello spaced value");
        assert_eq!(
            workspace.command_line.entries()[0].command,
            "hello:spaced value"
        );
    }

    #[test]
    fn removable_device_disconnect_is_capability_gated_and_audited() {
        let device = ResourceRef {
            provider: ProviderId::from("test.devices"),
            location: Location::new("test-device://usb"),
        };
        let left = CollectionSurface::new(
            "devices",
            "workspace.panel",
            "Devices",
            Location::new("test-device://attached"),
            vec![CollectionEntry {
                resource: device,
                metadata: ResourceMetadata {
                    name: "Test USB".to_owned(),
                    kind: ResourceKind::Virtual,
                    extensions: BTreeMap::from([(
                        "near.device.id".to_owned(),
                        MetadataValue::String("platform-usb-7".to_owned()),
                    )]),
                    ..ResourceMetadata::default()
                },
                details: "/dev/test-usb".to_owned(),
                selected: false,
            }],
        );
        let service = RecordingDeviceService::default();
        let calls = Arc::clone(&service.0);
        let peer = CollectionSurface::new(
            "peer",
            "workspace.panel",
            "Peer",
            Location::new("file:///peer"),
            Vec::new(),
        );
        let mut workspace = FarWorkspace::new(left, peer)
            .with_provider(Arc::new(TestDeviceProvider))
            .with_removable_device_service(Arc::new(service));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.device.disconnect"),
            arguments: BTreeMap::new(),
        });

        assert_eq!(*calls.lock().unwrap(), ["platform-usb-7"]);
        assert!(workspace.configuration_diagnostics.contains("status=0"));
        let export = workspace.diagnostic_export();
        let event = export
            .events
            .iter()
            .find(|event| {
                event.domain == DiagnosticDomain::Provider
                    && event.phase == DiagnosticPhase::Completed
                    && event.name == "device.disconnect"
            })
            .unwrap();
        assert_eq!(event.fields.get("device").unwrap(), "platform-usb-7");
        assert!(
            event
                .fields
                .get("audit")
                .unwrap()
                .contains("executable=test-device")
        );
        assert!(
            export
                .capabilities
                .iter()
                .any(|capability| capability == "device.disconnect")
        );
    }

    #[test]
    fn removable_device_disconnect_rejects_resources_without_capability() {
        let left = CollectionSurface::new(
            "devices",
            "workspace.panel",
            "Devices",
            Location::new("test-device://attached"),
            vec![CollectionEntry {
                resource: ResourceRef {
                    provider: ProviderId::from("test.devices"),
                    location: Location::new("test-device://fixed"),
                },
                metadata: ResourceMetadata {
                    name: "Fixed disk".to_owned(),
                    kind: ResourceKind::Virtual,
                    extensions: BTreeMap::from([(
                        "near.device.id".to_owned(),
                        MetadataValue::String("fixed-0".to_owned()),
                    )]),
                    ..ResourceMetadata::default()
                },
                details: "fixed".to_owned(),
                selected: false,
            }],
        );
        let service = RecordingDeviceService::default();
        let calls = Arc::clone(&service.0);
        let peer = CollectionSurface::new(
            "peer",
            "workspace.panel",
            "Peer",
            Location::new("file:///peer"),
            Vec::new(),
        );
        let mut workspace = FarWorkspace::new(left, peer)
            .with_provider(Arc::new(TestDeviceProvider))
            .with_removable_device_service(Arc::new(service));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.device.disconnect"),
            arguments: BTreeMap::new(),
        });

        assert!(calls.lock().unwrap().is_empty());
        assert!(workspace.status.contains("not a disconnectable device"));
    }

    #[test]
    fn unregistered_drive_like_prefix_falls_through_to_the_shell() {
        let executor = RecordingCommandExecutor::default();
        let commands = Arc::clone(&executor.commands);
        let mut workspace = FarWorkspace::demo()
            .with_embedded_pty(false)
            .with_command_line_executor(executor);
        workspace.command_line.set_buffer("C:work");
        workspace.submit_command_line();
        wait_for_command_line(&mut workspace);
        assert_eq!(*commands.lock().unwrap(), ["C:work"]);
    }

    #[test]
    fn extension_commands_share_registry_palette_dispatch_and_diagnostics() {
        let mut workspace = FarWorkspace::demo()
            .try_with_extension(Arc::new(TestExtension))
            .unwrap();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+P"));
        let Some(Overlay::CommandPalette { entries, .. }) = &workspace.overlay else {
            panic!("command palette should be open");
        };
        assert!(entries.iter().any(|entry| entry.title == "Extension Hello"));
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("test.extension.hello"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.status, "Hello from extension");
        assert!(
            workspace
                .configuration_diagnostics
                .contains("info: invoked")
        );
        let export = workspace.diagnostic_export();
        let plugin = export
            .events
            .iter()
            .find(|event| event.domain == DiagnosticDomain::Plugin)
            .unwrap();
        assert!(plugin.parent.is_some());
        assert!(export.events.iter().any(|event| {
            event.domain == DiagnosticDomain::Command && Some(event.correlation) == plugin.parent
        }));

        workspace.overlay = None;
        workspace.handle_terminal_event(&mut keymap, key("F11"));
        let Some(Overlay::Menu(_)) = &workspace.overlay else {
            panic!("extension action menu should be visible");
        };
        let theme = SemanticTheme::from_toml(THEME).unwrap();
        let text = workspace.snapshot(&theme, &keymap, 100, 30).join("\n");
        assert!(text.contains("Say hello"));
        assert!(text.contains("Configure test.extension"));

        let open_settings = workspace
            .extension_settings_open
            .keys()
            .next()
            .unwrap()
            .clone();
        workspace.dispatch(&CommandInvocation {
            id: open_settings,
            arguments: BTreeMap::new(),
        });
        assert!(matches!(workspace.overlay, Some(Overlay::Surface(_))));
        let save_settings = workspace
            .extension_settings_save
            .keys()
            .next()
            .unwrap()
            .clone();
        workspace.dispatch(&CommandInvocation {
            id: save_settings,
            arguments: BTreeMap::from([(
                "greeting".to_owned(),
                CommandValue::String("Near".to_owned()),
            )]),
        });
        assert_eq!(workspace.status, "Saved settings for test.extension");
    }

    #[test]
    fn command_palette_accepts_plain_text_search_and_activates_the_match() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();

        workspace.handle_terminal_event(&mut keymap, key("Ctrl+P"));
        for character in "near.temp-panel.import".chars() {
            workspace.handle_terminal_event(&mut keymap, key(&character.to_string()));
        }

        let Some(Overlay::CommandPalette {
            selected,
            entries,
            search,
        }) = &workspace.overlay
        else {
            panic!("command palette should remain open while filtering");
        };
        assert!(search.is_active());
        assert!(search.matches(["near.temp-panel.import"]));
        assert_eq!(entries[*selected].id.as_str(), "near.temp-panel.import");

        workspace.handle_terminal_event(&mut keymap, key("Enter"));
        assert!(workspace.status.starts_with("Open a temporary panel"));
    }

    #[test]
    fn extension_results_save_reopen_and_report_stale_sources() {
        let root =
            std::env::temp_dir().join(format!("near-fm-extension-panel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("generated.txt");
        fs::write(&source, "generated").unwrap();
        let provider = Arc::new(LocalFileProvider);
        let resource = ResourceRef {
            provider: provider.id(),
            location: LocalFileProvider::location(&source),
        };
        let left = filesystem_collection(provider.as_ref(), &root, "generated.left");
        let right = filesystem_collection(provider.as_ref(), &root, "generated.right");
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .try_with_extension(Arc::new(TestOpenExtension {
                resource: resource.clone(),
            }))
            .unwrap();

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("test.open-extension.results"),
            arguments: BTreeMap::new(),
        });
        assert!(
            workspace
                .focused_panel()
                .location()
                .as_str()
                .starts_with("search://")
        );
        assert_eq!(workspace.canonical_targets(), vec![resource.clone()]);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.keep-panel"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(workspace.saved_extension_panels.len(), 1);

        fs::remove_file(&source).unwrap();
        workspace.refresh_collections();
        for _ in 0..200 {
            workspace.poll_background_tasks();
            if workspace.focused_panel().current().is_some_and(|entry| {
                entry.metadata.extensions.get("near.generated.stale")
                    == Some(&MetadataValue::Boolean(true))
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let stale = workspace.focused_panel().current().unwrap();
        assert_eq!(stale.resource, resource);
        assert_eq!(
            stale.metadata.extensions.get("near.generated.stale"),
            Some(&MetadataValue::Boolean(true))
        );
        assert!(stale.details.contains("stale"));

        let session = workspace.saved_extension_panels[0].session;
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.search.open-panel"),
            arguments: BTreeMap::from([(
                "session".to_owned(),
                CommandValue::Integer(i64::try_from(session).unwrap()),
            )]),
        });
        assert_eq!(workspace.canonical_targets(), vec![resource]);
        assert!(workspace.status.contains("Opened saved generated panel"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn theme_presets_role_edits_commit_and_rollback_atomically() {
        let base = SemanticTheme::from_toml(THEME).unwrap();
        let high_contrast = SemanticTheme::from_toml(HIGH_CONTRAST).unwrap();
        let original = base.resolve("text", TerminalColorDepth::TrueColor);
        let mut workspace =
            FarWorkspace::demo().with_theme_presets(base.clone(), [high_contrast.clone()]);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.preview"),
            arguments: BTreeMap::from([(
                "name".to_owned(),
                CommandValue::String(high_contrast.name().to_owned()),
            )]),
        });
        assert_eq!(
            workspace.working_theme.as_ref().unwrap().name(),
            high_contrast.name()
        );
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.rollback"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(
            workspace.working_theme.as_ref().unwrap().name(),
            base.name()
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.edit-confirmed"),
            arguments: BTreeMap::from([
                ("role".to_owned(), CommandValue::String("text".to_owned())),
                (
                    "foreground".to_owned(),
                    CommandValue::String("#123456".to_owned()),
                ),
                ("background".to_owned(), CommandValue::String(String::new())),
            ]),
        });
        assert_eq!(
            workspace
                .working_theme
                .as_ref()
                .unwrap()
                .resolve("text", TerminalColorDepth::TrueColor)
                .foreground,
            Some(SemanticColor::Rgb {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
            })
        );
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.rollback"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(
            workspace
                .working_theme
                .as_ref()
                .unwrap()
                .resolve("text", TerminalColorDepth::TrueColor),
            original
        );

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.preview"),
            arguments: BTreeMap::from([(
                "name".to_owned(),
                CommandValue::String(high_contrast.name().to_owned()),
            )]),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.commit"),
            arguments: BTreeMap::new(),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.edit-confirmed"),
            arguments: BTreeMap::from([
                ("role".to_owned(), CommandValue::String("text".to_owned())),
                (
                    "foreground".to_owned(),
                    CommandValue::String("ansi:2".to_owned()),
                ),
                ("background".to_owned(), CommandValue::String(String::new())),
            ]),
        });
        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.theme.rollback"),
            arguments: BTreeMap::new(),
        });
        assert_eq!(
            workspace.working_theme.as_ref().unwrap().name(),
            high_contrast.name()
        );
    }

    #[test]
    fn semantic_snapshots_preserve_roles_across_themes_and_sizes() {
        let mut workspace = FarWorkspace::demo();
        let mut keymap = Keymap::from_toml(KEYMAP).unwrap();
        workspace.handle_terminal_event(&mut keymap, key("Space"));
        let themes = [THEME, TERMINAL_NATIVE, HIGH_CONTRAST]
            .map(|source| SemanticTheme::from_toml(source).unwrap());

        for (width, height) in [(80, 24), (120, 30), (180, 40)] {
            let snapshots = themes
                .iter()
                .map(|theme| workspace.semantic_snapshot(theme, &keymap, width, height))
                .collect::<Vec<_>>();
            assert_eq!(snapshots[0].role_lines(), snapshots[1].role_lines());
            assert_eq!(snapshots[0].role_lines(), snapshots[2].role_lines());
            let roles = snapshots[0].role_lines().join("\n");
            assert!(roles.contains("panel.item.selected.focused"));
            assert!(roles.contains("panel.border.focused"));
            assert!(roles.contains("keybar.key"));
            for snapshot in &snapshots {
                let screen = snapshot.text_lines().join("\n");
                assert!(screen.contains("Cargo.toml"));
                assert!(screen.contains("Near interaction laboratory"));
            }
        }
    }

    #[test]
    fn saved_filters_toggle_per_panel_and_mark_the_panel_border() {
        let root = std::env::temp_dir().join(format!("near-panel-filters-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.rs"), b"fn main() {}\n").unwrap();
        fs::write(root.join("archive.bin"), vec![0_u8; 2048]).unwrap();
        let provider = Arc::new(LocalFileProvider);
        let left = filesystem_collection(provider.as_ref(), &root, "left");
        let right = filesystem_collection(provider.as_ref(), &root, "right");
        let filters = FilterCatalog::from_toml(
            r#"
schema = 1
[[mask_groups]]
id = "source"
label = "Source"
masks = ["*.rs"]
[[filters]]
id = "source"
label = "Source files"
mode = "include"
mask_group = "source"
"#,
        )
        .unwrap();
        let mut workspace = FarWorkspace::new(left, right)
            .with_provider(provider)
            .with_filters(filters);

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.filters.show"),
            arguments: BTreeMap::new(),
        });
        let menu = workspace
            .snapshot(
                &SemanticTheme::from_toml(THEME).unwrap(),
                &Keymap::from_toml(KEYMAP).unwrap(),
                100,
                30,
            )
            .join("\n");
        assert!(menu.contains("Left Panel Filters"));
        assert!(menu.contains("+ Source files"));
        assert!(menu.contains("mask group: source"));

        workspace.dispatch(&CommandInvocation {
            id: CommandId::from("near.filters.toggle"),
            arguments: BTreeMap::from([(
                "filter".to_owned(),
                CommandValue::String("source".to_owned()),
            )]),
        });
        wait_for_listing(&mut workspace, FocusedPanel::Left);
        wait_for_listing(&mut workspace, FocusedPanel::Right);
        assert!(
            workspace
                .left
                .entries()
                .iter()
                .any(|entry| entry.metadata.name == "main.rs")
        );
        assert!(
            !workspace
                .left
                .entries()
                .iter()
                .any(|entry| entry.metadata.name == "archive.bin")
        );
        assert!(
            workspace
                .right
                .entries()
                .iter()
                .any(|entry| entry.metadata.name == "archive.bin")
        );
        assert!(workspace.left.filter_active());
        assert!(!workspace.right.filter_active());
        let frame = workspace
            .snapshot(
                &SemanticTheme::from_toml(THEME).unwrap(),
                &Keymap::from_toml(KEYMAP).unwrap(),
                100,
                30,
            )
            .join("\n");
        assert!(frame.contains("*] ["));

        fs::remove_dir_all(root).unwrap();
    }
}
