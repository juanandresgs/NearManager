use near_core::{
    ActionContext, Location, PermissionSummary, ResourceKind, ResourceMetadata, ResourceRef,
};
use near_ui::{
    CollectionEntry, CollectionSurface, HighlightingCatalog, RenderContext, ScenePrimitive,
    SceneRect, Surface,
};

#[cfg(target_os = "macos")]
use near_core::{CancellationToken, CommandInvocation, ListingGeneration, OperationId};
#[cfg(target_os = "macos")]
use near_local_fs::LocalFileProvider;
#[cfg(target_os = "macos")]
use near_ops::{
    ConflictDecision, ExecutionAuthorization, ExecutionSummary, OperationIntent, OperationPlan,
    OperationService,
};
#[cfg(target_os = "macos")]
use near_ui::{FarWorkspace, Keymap, SemanticTheme};

fn entry(name: &str, kind: ResourceKind, mode: u32) -> CollectionEntry {
    CollectionEntry::new(
        ResourceRef {
            provider: "test".into(),
            location: Location::new(format!("test:///{name}")),
        },
        ResourceMetadata {
            name: name.to_owned(),
            kind,
            size: (kind == ResourceKind::File).then_some(1),
            permissions: Some(PermissionSummary {
                unix_mode: Some(mode),
                readonly: false,
                executable: mode & 0o111 != 0,
            }),
            ..ResourceMetadata::default()
        },
        name,
    )
}

#[test]
fn executable_directory_keeps_directory_highlighting() {
    let catalog =
        HighlightingCatalog::from_toml(include_str!("../../../specs/highlighting.toml")).unwrap();
    assert_eq!(
        catalog
            .decoration(&entry("folder", ResourceKind::Directory, 0o755).metadata)
            .rule_id
            .as_deref(),
        Some("directories")
    );
}

#[test]
fn directory_kinds_use_far_name_and_size_presentation() {
    let surface = CollectionSurface::new(
        "kinds",
        "workspace.panel",
        "Kinds",
        Location::new("test:///"),
        vec![
            entry("folder", ResourceKind::Directory, 0o755),
            entry("file.txt", ResourceKind::File, 0o644),
            entry("link", ResourceKind::Symlink, 0o777),
            entry("package", ResourceKind::Package, 0o755),
            entry("virtual", ResourceKind::Virtual, 0o644),
        ],
    );
    let action = ActionContext::default();
    let scene = surface.scene(
        SceneRect::new(0, 0, 60, 10),
        &RenderContext {
            focused: false,
            action: &action,
        },
    );
    let rows = scene
        .primitives()
        .iter()
        .filter_map(|primitive| match primitive {
            ScenePrimitive::Text { content, role, .. } => Some((content.as_str(), role.as_str())),
            _ => None,
        });
    let rows = rows.collect::<Vec<_>>();
    assert!(
        rows.iter()
            .any(|(text, role)| text.contains("folder") && *role == "panel.item.directory")
    );
    assert!(rows.iter().any(|(text, _)| text.contains("Folder")));
    assert!(rows.iter().all(|(text, _)| !text.contains("/ folder")));
    assert!(rows.iter().any(|(text, _)| text.contains("file.txt")));
}

#[cfg(target_os = "macos")]
#[test]
fn stale_macos_volume_namespace_entry_is_denied_before_planning() {
    struct PlanningMustNotRun;
    impl OperationService for PlanningMustNotRun {
        fn plan(
            &mut self,
            _intent: OperationIntent,
            _generation: ListingGeneration,
        ) -> Result<OperationPlan, String> {
            panic!("protected volume reached operation planning")
        }

        fn execute(
            &mut self,
            _plan: &OperationId,
            _authorization: ExecutionAuthorization,
            _cancellation: &CancellationToken,
            _conflict: ConflictDecision,
        ) -> Result<ExecutionSummary, String> {
            unreachable!()
        }
    }

    let provider = std::sync::Arc::new(LocalFileProvider);
    let volume =
        LocalFileProvider::resource_for_path(std::path::Path::new("/Volumes/Unmounted\\ Volume"));
    let left = CollectionSurface::new(
        "left",
        "workspace.panel",
        "Volumes",
        Location::new("file:///Volumes"),
        vec![CollectionEntry::new(
            volume,
            ResourceMetadata {
                name: "Unmounted\\ Volume".to_owned(),
                kind: ResourceKind::Directory,
                ..ResourceMetadata::default()
            },
            "stale mount namespace entry",
        )],
    );
    let right = CollectionSurface::new(
        "right",
        "workspace.panel",
        "Peer",
        Location::new("test:///"),
        vec![entry("peer.txt", ResourceKind::File, 0o644)],
    );
    let mut workspace = FarWorkspace::new(left, right)
        .with_provider(provider)
        .with_operation_service(PlanningMustNotRun);
    workspace.dispatch(&CommandInvocation::new("near.resource.trash"));
    let rendered = workspace
        .snapshot(
            &SemanticTheme::from_toml(include_str!("../../../specs/theme.toml")).unwrap(),
            &Keymap::from_toml(include_str!("../../../specs/keymap.toml")).unwrap(),
            100,
            28,
        )
        .join("\n");
    assert!(rendered.contains("Cannot Move to Trash"), "{rendered}");
    assert!(
        rendered.contains("did not create an operation plan"),
        "{rendered}"
    );
    assert!(rendered.contains("unmount or eject"), "{rendered}");
}
