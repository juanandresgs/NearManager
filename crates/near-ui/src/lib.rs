//! Semantic interaction and reusable terminal surfaces for Near applications.

mod application;
mod collection;
mod command;
mod command_history;
mod command_line;
mod confirmation;
mod dialog;
mod editor;
mod filter;
mod folder_history;
mod help;
mod highlighting;
mod inspector;
mod interaction_kernel;
mod interface_settings;
mod keymap;
mod layout;
mod list_navigation;
mod menu;
mod operation_preview;
mod panel_mode;
mod render_loop;
mod resource_history;
mod scene;
mod scene_renderer;
mod selection_search;
mod semantic;
mod settings_surface;
mod shell;
mod surface;
mod tab_registry;
mod task;
mod terminal_surface;
mod theme;
mod tree;
mod viewer;
mod workspace;
mod workspace_diagnostics;
mod workspace_runtime;
mod workspace_settings;

#[cfg(test)]
mod catalog_tests;

pub use application::{RunApplicationError, SurfaceApplication, run_surface_application};
pub use collection::{
    CollectionEntry, CollectionLookupMode, CollectionStateSnapshot, CollectionSurface,
    CollectionTargetScope, CollectionViewport, ComparisonSelection, FolderComparisonPolicy,
    FolderComparisonResult, SortMode, SortState, compare_folders,
};
pub use command::{Command, CommandCheckError, CommandRegistry, CommandRegistryError};
pub use command_history::CommandHistorySurface;
pub use confirmation::{ConfirmationPolicy, ConfirmationPolicyError};
pub use dialog::{DialogField, DialogSurface};
pub use editor::{
    EditorEncoding, EditorLineEnding, EditorPosition, EditorSaveFormat, EditorSaveOutcome,
    EditorSurface,
};
pub use filter::{FilterCatalog, FilterError, FilterMode, NamedMaskGroup, SavedFilter};
pub use folder_history::FolderHistorySurface;
pub use help::{HelpEntry, HelpLink, HelpSurface, HelpTopic};
pub use highlighting::{FileDecoration, HighlightingCatalog, HighlightingError};
pub use inspector::{InspectorField, InspectorSurface};
pub use interaction_kernel::{
    CollectionInteractionEffect, CollectionInteractionModel, CollectionInteractionMsg,
    update_collection_interaction,
};
pub use interface_settings::{InterfaceSettings, InterfaceSettingsError, StartupPanel};
pub use keymap::{
    BindingConflict, BindingOrigin, KeyBinding, Keymap, KeymapError, KeymapSettings, ResolveResult,
    format_key_sequence, format_key_stroke, parse_key_stroke,
};
pub use layout::{DualSurfaceGeometry, DualSurfaceLayout, DualSurfaceSide};
pub use list_navigation::ListNavigation;
pub use menu::{MenuItem, MenuSurface};
pub use near_config::{EditorSettings, ResourceOpenPolicy, ViewerEncoding, ViewerSettings};
#[cfg(feature = "embedded-pty")]
pub use near_pty::PtySessionHandle as EmbeddedTerminalSession;
pub use operation_preview::OperationPreviewSurface;
pub use panel_mode::{
    ColumnAlignment, PanelColumn, PanelColumnKind, PanelModeCatalog, PanelModeDefaults,
    PanelModeError, PanelModesDocument, PanelViewMode,
};
pub use resource_history::{HistorySettings, ResourceHistoryKind, ResourceHistorySurface};
pub use scene::{
    Scene, SceneBorder, SceneColor, ScenePrimitive, SceneRect, SceneTextStyle, TextAlignment,
};
pub use scene_renderer::snapshot_scene;
pub use semantic::{SemanticCell, SemanticSnapshot};
pub use settings_surface::{SettingSurfaceEntry, SettingsDocumentStore, SettingsSurface};
pub use shell::SurfaceShell;
pub use surface::{
    RenderContext, Surface, SurfaceEvent, SurfacePresentation, SurfaceState, UpdateContext,
    UpdateResult,
};
pub use tab_registry::{PaneSlot, TabEntry, TabId, TabRegistry, ZoomablePanePresentation};
pub use task::{TaskRecord, TaskState, TaskSurface};
#[cfg(feature = "embedded-pty")]
pub use terminal_surface::{EmbeddedTerminalDockSurface, EmbeddedTerminalSurface};
pub use terminal_surface::{TerminalInputMode, TerminalSurface};
pub use theme::{
    ResolvedRoleStyle, SemanticColor, SemanticModifier, SemanticTheme, TerminalColorDepth,
    ThemeDensity, ThemeError, ThemeGlyphs, format_semantic_color, parse_semantic_color,
};
pub use tree::{TreeNode, TreeSurface};
pub use viewer::{ViewerLoadTicket, ViewerRequestTracker, ViewerSurface};
pub use workspace::{CollectionItem, FarWorkspace, RunWorkspaceError, WorkspaceAction};
pub use workspace_runtime::{run_workspace, run_workspace_at_depth};
