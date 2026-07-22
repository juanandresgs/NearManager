//! Backend-independent domain contracts for Near applications.

mod apply_command;
mod clipboard;
mod command;
mod command_line;
mod device;
mod diagnostic;
mod extension;
mod external;
mod id;
mod navigation;
mod provider;
mod resource;
mod state;

pub use apply_command::{
    ApplyCommandMode, ApplyCommandTarget, ApplyCommandTemplate, ApplyCommandTemplateError,
};
pub use clipboard::Clipboard;
pub use command::{
    ActionContext, ArgumentKind, ArgumentSchema, Availability, CommandDescriptor,
    CommandInvocation, CommandValue, SafetyClass,
};
pub use command_line::{
    CommandHistoryEntry, CommandHistoryStore, CommandLineArgumentResolver, CommandLineExecutor,
    CommandLineOutput, CommandPrefixDescriptor, ExtensionCommandPrefix,
};
pub use device::{DeviceDisconnectReport, RemovableDevice, RemovableDeviceService};
pub use diagnostic::{
    CorrelationId, DiagnosticDomain, DiagnosticEvent, DiagnosticExport, DiagnosticJournal,
    DiagnosticPhase,
};
pub use extension::{
    CommandExtension, ExtensionEffect, ExtensionHelpTopic, ExtensionMenuItem, ExtensionReport,
    ExtensionSetting,
};
pub use external::{
    ExternalAction, ExternalInvocation, ExternalInvocationMode, ExternalResolution,
    ExternalToolResolver,
};
pub use id::{CapabilityId, CommandId, ContextId, OperationId, ProviderId, RoleId, SurfaceId};
pub use navigation::{
    EditorPositionEntry, EditorPositionStore, FolderLocationEntry, FolderNavigationState,
    FolderNavigationStore, ResourceHistoryEntry, ResourceHistoryState, ResourceHistoryStore,
    ViewerStateEntry, ViewerStateStore,
};
pub use provider::{
    CancellationToken, ListPage, ListRequest, ListingGeneration, OpenRequest,
    ProviderCollectionResolution, ProviderError, ProviderFuture, ProviderLocation,
    ProviderRegistry, ProviderRegistryError, ResourceEntry, ResourceProvider, ResourceStream,
    ResourceVersion, WriteRequest,
};
pub use resource::{
    CapabilitySet, Location, MetadataValue, MutationAlternative, MutationDenial,
    MutationEligibility, MutationKind, OwnerSummary, PermissionSummary, RESOURCE_DESCRIPTION_KEY,
    ResourceClassification, ResourceIdentity, ResourceKind, ResourceMetadata, ResourceRef,
};
pub use state::StateDocumentStore;
