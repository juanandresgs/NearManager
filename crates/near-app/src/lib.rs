//! Stable application facade for backend-independent Near programs.

use std::time::Duration;

use near_ui::{RunApplicationError, SurfaceApplication, run_surface_application};
use thiserror::Error;

pub use near_config::{
    CONFIG_SCHEMA_VERSION, ConfigEngine, ConfigError, ConfigErrorKind, ConfigLayer,
    ConfigLayerKind, ConfigManager, ConfigMigration, ConfigOrigin, ConfigurationCoordinator,
    CoordinatorError, EditorSettings, EffectiveConfig, NoopSettingsPersistence, ReloadError,
    ResourceOpenPolicy, SettingApplier, SettingApplyScope, SettingCandidate, SettingDescriptor,
    SettingPlatform, SettingPlatformAvailability, SettingProvenance, SettingState, SettingValue,
    SettingValueKind, SettingsPersistence, ViewerEncoding, ViewerSettings,
};
pub use near_core::{
    ActionContext, ArgumentKind, ArgumentSchema, CancellationToken, CapabilityId, CapabilitySet,
    CommandDescriptor, CommandExtension, CommandId, CommandInvocation, CommandValue, ContextId,
    ExtensionEffect, ExtensionReport, ListPage, ListRequest, ListingGeneration, Location,
    MetadataValue, MutationAlternative, MutationDenial, MutationEligibility, MutationKind,
    OpenRequest, ProviderCollectionResolution, ProviderError, ProviderFuture, ProviderId,
    ProviderRegistry, ResourceClassification, ResourceEntry, ResourceIdentity, ResourceKind,
    ResourceMetadata, ResourceProvider, ResourceRef, ResourceStream, SafetyClass,
    StateDocumentStore, SurfaceId, ViewerStateEntry,
};
pub use near_handlers::{
    HANDLER_SCHEMA_VERSION, HandlerDocument, HandlerInvocationTemplate, HandlerRule, HandlerValue,
};
pub use near_macros::{
    MACRO_SCHEMA_VERSION, MacroCondition, MacroContext, MacroDiagnostic, MacroDocument,
    MacroEngine, MacroError, MacroHost, MacroRecorder, MacroStep, MacroStepDiagnostic, MacroStore,
    MacroTrust, PresenceCondition, ReplayPolicy, ReplayReport, SemanticMacro, TomlMacroStore,
};
pub use near_ops::{
    ConflictAction, ConflictPolicy, CrossDeviceBehavior, MetadataPolicy, OperationDecision,
    OperationFailurePresentation, OperationKind, OperationPlan, OperationPresentation,
    RecoveryPolicy, SymlinkPolicy, VerificationPolicy,
};
pub use near_pty::{
    ResolvedShellProfile, SHELL_PROFILE_SCHEMA_VERSION, ShellClosePolicy, ShellMode, ShellProfile,
};
pub use near_runtime::{TaskOutcome, TaskPool, TaskRecord, TaskState, block_on};
pub use near_search::{
    ContentMatch, ContentPredicate, HiddenPolicy, IgnorePolicy, PREDICATE_SCHEMA_VERSION,
    ResourcePredicate, SearchEncoding, SearchError, SearchEvent, SearchHit, SearchProgress,
    SearchRequest, SearchResultsProvider, SearchService, TextPredicate,
};
pub use near_terminal::{Key, KeyKind, KeyStroke, ModifierKey, Modifiers, TerminalEvent};
pub use near_ui::{
    CollectionEntry, CollectionInteractionEffect, CollectionInteractionModel,
    CollectionInteractionMsg, CollectionLookupMode, CollectionStateSnapshot, CollectionSurface,
    CollectionTargetScope, CollectionViewport, DialogField, DialogSurface, DualSurfaceGeometry,
    DualSurfaceLayout, DualSurfaceSide, EditorEncoding, EditorLineEnding, EditorPosition,
    EditorSaveFormat, EditorSaveOutcome, EditorSurface, HelpEntry, HelpLink, HelpSurface,
    HelpTopic, InspectorField, InspectorSurface, KeyBinding, Keymap, KeymapError, ListNavigation,
    MenuItem, MenuSurface, OperationPreviewSurface, PaneSlot, RenderContext, Scene, SceneBorder,
    SceneColor, ScenePrimitive, SceneRect, SceneTextStyle, SemanticSnapshot, SemanticTheme,
    SettingSurfaceEntry, SettingsDocumentStore, SettingsSurface, StartupPanel, Surface,
    SurfaceEvent, SurfaceShell, SurfaceState, TabEntry, TabId, TabRegistry, TaskSurface,
    TerminalInputMode, TerminalSurface, TextAlignment, UpdateContext, UpdateResult, ViewerSurface,
    ZoomablePanePresentation, parse_key_stroke, snapshot_scene, update_collection_interaction,
};

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ApplicationBuildError {
    #[error("application theme is required")]
    MissingTheme,
    #[error("application keymap is required")]
    MissingKeymap,
}

pub struct ApplicationBuilder {
    runtime: SurfaceApplication,
    theme: Option<SemanticTheme>,
    keymap: Option<Keymap>,
}

impl ApplicationBuilder {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        surface: impl Surface + 'static,
    ) -> Self {
        Self {
            runtime: SurfaceApplication::new(id, title, surface),
            theme: None,
            keymap: None,
        }
    }

    #[must_use]
    pub fn theme(mut self, theme: SemanticTheme) -> Self {
        self.theme = Some(theme);
        self
    }

    #[must_use]
    pub fn keymap(mut self, keymap: Keymap) -> Self {
        self.keymap = Some(keymap);
        self
    }

    /// Builds a configured application without selecting a terminal backend.
    ///
    /// # Errors
    ///
    /// Returns an error when a shared theme or keymap was not supplied.
    pub fn build(self) -> Result<Application, ApplicationBuildError> {
        Ok(Application {
            runtime: self.runtime,
            theme: self.theme.ok_or(ApplicationBuildError::MissingTheme)?,
            keymap: self.keymap.ok_or(ApplicationBuildError::MissingKeymap)?,
        })
    }
}

pub struct Application {
    runtime: SurfaceApplication,
    theme: SemanticTheme,
    keymap: Keymap,
}

impl Application {
    /// Runs the application through Near's terminal adapter.
    ///
    /// # Errors
    ///
    /// Returns terminal initialization, rendering, input, signal, or restoration failures.
    pub fn run(self) -> Result<(), RunApplicationError> {
        run_surface_application(self.runtime, &self.theme, self.keymap)
    }

    pub fn dispatch(&mut self, invocation: &CommandInvocation) {
        self.runtime.dispatch(invocation, &self.keymap);
    }

    pub fn handle_terminal_event(&mut self, event: TerminalEvent) {
        self.runtime.handle_terminal_event(&mut self.keymap, event);
    }

    pub fn handle_terminal_event_at(&mut self, event: TerminalEvent, now: Duration) {
        self.runtime
            .handle_terminal_event_at(&mut self.keymap, event, now);
    }

    /// Sends a normalized key specification through the configured keymap.
    ///
    /// # Errors
    ///
    /// Returns a keymap error when `key` is not a valid Near key specification.
    pub fn handle_key(&mut self, key: &str) -> Result<(), KeymapError> {
        self.handle_terminal_event(TerminalEvent::Key(parse_key_stroke(key)?));
        Ok(())
    }

    /// Sends a normalized key at deterministic manual time.
    ///
    /// # Errors
    ///
    /// Returns a keymap error when `key` is not a valid Near key specification.
    pub fn handle_key_at(&mut self, key: &str, now: Duration) -> Result<(), KeymapError> {
        self.handle_terminal_event_at(TerminalEvent::Key(parse_key_stroke(key)?), now);
        Ok(())
    }

    pub fn paste(&mut self, text: impl Into<String>) {
        self.handle_terminal_event(TerminalEvent::Paste(text.into()));
    }

    pub fn paste_at(&mut self, text: impl Into<String>, now: Duration) {
        self.handle_terminal_event_at(TerminalEvent::Paste(text.into()), now);
    }

    pub fn handle_keymap_timeout_at(&mut self, now: Duration) {
        self.runtime.handle_keymap_timeout_at(&mut self.keymap, now);
    }

    pub fn snapshot(&self, width: u16, height: u16) -> SemanticSnapshot {
        self.runtime.snapshot(&self.theme, width, height)
    }

    pub fn runtime(&self) -> &SurfaceApplication {
        &self.runtime
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        ApplicationBuildError, ApplicationBuilder, CollectionEntry, CollectionInteractionModel,
        CollectionInteractionMsg, CollectionStateSnapshot, CollectionSurface,
        CollectionTargetScope, CommandId, CommandInvocation, CommandValue, ConfigLayerKind,
        DualSurfaceGeometry, DualSurfaceLayout, DualSurfaceSide, Keymap, ListNavigation, Location,
        ProviderId, ResourceMetadata, ResourceRef, SemanticTheme, SettingApplyScope,
        SettingDescriptor, SettingPlatformAvailability, SettingProvenance, SettingState,
        SettingSurfaceEntry, SettingValue, SettingsSurface, ViewerSurface,
        update_collection_interaction,
    };

    const KEYMAP: &str = include_str!("../../../specs/keymap.toml");
    const THEME: &str = include_str!("../../../specs/theme.toml");

    #[test]
    fn builder_requires_shared_configuration_and_never_exposes_a_backend() {
        let error = ApplicationBuilder::new(
            "test.app",
            "Test",
            ViewerSurface::text("viewer", "Viewer", "content"),
        )
        .build()
        .err()
        .unwrap();
        assert_eq!(error, ApplicationBuildError::MissingTheme);

        let application = ApplicationBuilder::new(
            "test.app",
            "Test",
            ViewerSurface::text("viewer", "Viewer", "content"),
        )
        .theme(SemanticTheme::from_toml(THEME).unwrap())
        .keymap(Keymap::from_toml(KEYMAP).unwrap())
        .build()
        .unwrap();
        assert!(
            application
                .snapshot(80, 24)
                .text_lines()
                .join("\n")
                .contains("content")
        );
    }

    #[test]
    fn public_collection_viewport_pages_without_file_manager_internals() {
        let entries = (0..64)
            .map(|index| {
                CollectionEntry::new(
                    ResourceRef {
                        provider: ProviderId::from("proof.records"),
                        location: Location::new(format!("proof://records/{index}")),
                    },
                    ResourceMetadata {
                        name: format!("record-{index:02}"),
                        ..ResourceMetadata::default()
                    },
                    "record",
                )
            })
            .collect();
        let surface = CollectionSurface::new(
            "proof.records",
            "proof.collection",
            "Records",
            Location::new("proof://records"),
            entries,
        );
        let mut application = ApplicationBuilder::new("proof.app", "Proof", surface)
            .theme(SemanticTheme::from_toml(THEME).unwrap())
            .keymap(Keymap::from_toml(KEYMAP).unwrap())
            .build()
            .unwrap();

        application.snapshot(80, 12);
        application.dispatch(&CommandInvocation {
            id: CommandId::from("near.collection.page"),
            arguments: BTreeMap::from([("pages".to_owned(), CommandValue::Integer(1))]),
        });
        let snapshot = application.snapshot(80, 12).text_lines().join("\n");
        assert!(snapshot.contains("record-10"));
        assert!(!snapshot.contains("record-00"));
    }

    #[test]
    fn public_collection_state_survives_replacement() {
        let entries = ["alpha", "beta", "gamma"]
            .into_iter()
            .map(|name| {
                CollectionEntry::new(
                    ResourceRef {
                        provider: ProviderId::from("proof.records"),
                        location: Location::new(format!("proof://records/{name}")),
                    },
                    ResourceMetadata {
                        name: name.to_owned(),
                        ..ResourceMetadata::default()
                    },
                    "record",
                )
            })
            .collect();
        let mut surface = CollectionSurface::new(
            "proof.records",
            "proof.collection",
            "Records",
            Location::new("proof://records"),
            entries,
        );
        surface.set_cursor(1);
        surface.toggle_selection();
        let state: CollectionStateSnapshot = surface.state_snapshot();
        surface.replace(
            Location::new("proof://records"),
            vec![CollectionEntry::new(
                ResourceRef {
                    provider: ProviderId::from("proof.records"),
                    location: Location::new("proof://records/beta"),
                },
                ResourceMetadata {
                    name: "beta".to_owned(),
                    ..ResourceMetadata::default()
                },
                "record",
            )],
        );
        surface.restore_state(&state);
        assert_eq!(surface.current().unwrap().metadata.name, "beta");
        assert_eq!(surface.selected_resources().len(), 1);
    }

    #[test]
    fn public_collection_can_focus_an_exact_provider_resource() {
        let target = ResourceRef {
            provider: ProviderId::from("proof.records"),
            location: Location::new("proof://records/gamma"),
        };
        let mut surface = CollectionSurface::new(
            "proof.records",
            "proof.collection",
            "Records",
            Location::new("proof://records"),
            ["alpha", "beta", "gamma"]
                .into_iter()
                .map(|name| {
                    CollectionEntry::new(
                        ResourceRef {
                            provider: ProviderId::from("proof.records"),
                            location: Location::new(format!("proof://records/{name}")),
                        },
                        ResourceMetadata {
                            name: name.to_owned(),
                            ..ResourceMetadata::default()
                        },
                        "record",
                    )
                })
                .collect(),
        );

        assert!(surface.focus_resource(&target));
        assert_eq!(surface.current().unwrap().resource, target);
    }

    #[test]
    fn public_settings_surface_hides_advanced_entries_until_explicitly_requested() {
        let setting = |id: &str, title: &str, advanced: bool| SettingSurfaceEntry {
            descriptor: SettingDescriptor {
                id: id.to_owned(),
                document: "proof.toml".to_owned(),
                path: id.to_owned(),
                category: "Proof".to_owned(),
                title: title.to_owned(),
                description: "Proof application setting".to_owned(),
                advanced,
                value_kind: SettingValue::Boolean(false).kind(),
                default_value: SettingValue::Boolean(false),
                apply_scope: SettingApplyScope::Live,
                apply_order: 0,
                availability: SettingPlatformAvailability::All,
            },
            state: SettingState {
                value: SettingValue::Boolean(false),
                provenance: SettingProvenance {
                    layer: ConfigLayerKind::BuiltIn,
                    source: "proof built-in".to_owned(),
                },
            },
        };
        let surface = SettingsSurface::new(
            "proof.settings",
            "Proof Settings",
            vec![
                setting("proof.normal", "Normal setting", false),
                setting("proof.advanced", "Advanced setting", true),
            ],
        );
        let mut application = ApplicationBuilder::new("proof.app", "Proof", surface)
            .theme(SemanticTheme::from_toml(THEME).unwrap())
            .keymap(Keymap::from_toml(KEYMAP).unwrap())
            .build()
            .unwrap();

        let hidden = application.snapshot(100, 12).text_lines().join("\n");
        assert!(hidden.contains("Normal setting"));
        assert!(!hidden.contains("Advanced setting"));
        application.dispatch(&CommandInvocation {
            id: CommandId::from("near.settings.toggle-advanced"),
            arguments: BTreeMap::new(),
        });
        let shown = application.snapshot(100, 12).text_lines().join("\n");
        assert!(shown.contains("Advanced setting"));
    }

    #[test]
    fn public_dual_surface_layout_keeps_resize_and_hit_testing_consistent() {
        let mut layout = DualSurfaceLayout::default();
        layout.resize_columns(100, 10, 8);
        layout.resize_rows(30, -5, 5);
        let geometry: DualSurfaceGeometry = layout.geometry(100, 30, 8, 5);
        assert_eq!((geometry.first_width, geometry.second_width), (60, 40));
        assert_eq!(geometry.pane_height, 25);
        assert_eq!(layout.side_at(59, 100, 8), Some(DualSurfaceSide::First));
        assert_eq!(layout.side_at(60, 100, 8), Some(DualSurfaceSide::Second));
    }

    #[test]
    fn public_list_and_collection_target_contracts_are_application_neutral() {
        let visible = [2, 4, 6, 8, 10];
        let mut navigation = ListNavigation::default();
        navigation.set_cursor(2);
        navigation.window(&visible, 2);
        navigation.page(&visible, 1);
        assert_eq!(navigation.cursor(), 6);

        let mut collection = CollectionSurface::new(
            "proof.targets",
            "proof.collection",
            "Targets",
            Location::new("proof://targets"),
            ["alpha", "beta", "gamma"]
                .into_iter()
                .map(|name| {
                    CollectionEntry::new(
                        ResourceRef {
                            provider: ProviderId::from("proof.records"),
                            location: Location::new(format!("proof://targets/{name}")),
                        },
                        ResourceMetadata {
                            name: name.to_owned(),
                            ..ResourceMetadata::default()
                        },
                        "record",
                    )
                })
                .collect(),
        );
        collection.set_cursor(0);
        collection.toggle_selection();
        collection.set_cursor(1);
        assert_eq!(
            collection
                .target_resources(CollectionTargetScope::SelectionOrCurrent)
                .len(),
            1
        );
        assert!(
            collection.target_resources(CollectionTargetScope::CurrentOnly)[0]
                .location
                .as_str()
                .ends_with("/beta")
        );
    }

    #[test]
    fn public_interaction_kernel_supports_non_file_manager_marked_lists() {
        let mut model = CollectionInteractionModel::new(5, 0, [], [true; 5], 0, 3);
        update_collection_interaction(
            &mut model,
            CollectionInteractionMsg::ToggleCurrentAndMove(1),
        );
        update_collection_interaction(&mut model, CollectionInteractionMsg::Move(2));
        update_collection_interaction(
            &mut model,
            CollectionInteractionMsg::ToggleCurrentAndMove(1),
        );
        assert_eq!(model.cursor(), 4);
        assert_eq!(model.selected().iter().copied().collect::<Vec<_>>(), [0, 3]);
    }
}
