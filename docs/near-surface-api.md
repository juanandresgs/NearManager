# Near Surface and Scene API

Near applications compose terminal interfaces without importing Ratatui or Crossterm. Those crates remain private renderer and terminal substrates.

## Public Model

- `Scene`, `SceneRect`, and `ScenePrimitive` describe fills, semantic text, and borders using Near-owned geometry and role identifiers.
- `Surface` owns behavior, contexts, capabilities, resource state, command updates, and scene generation.
- `SurfaceShell` owns focus, optional peer relationships, command routing, action-context derivation, and surface layout.
- `snapshot_scene` renders any public scene through Near's private backend and returns content plus semantic role IDs.
- The reusable catalog includes `CollectionSurface`, `TreeSurface`, `ViewerSurface`, `InspectorSurface`, `TerminalSurface`, `TaskSurface`, `MenuSurface`, `DialogSurface`, and `HelpSurface`.
- Surface updates consume semantic `SurfaceEvent` values and may return a `CommandInvocation` effect for the host to execute.

## Minimal Application Surface

```rust
use near_core::{CapabilitySet, ContextId, SurfaceId};
use near_ui::{
    RenderContext, Scene, SceneRect, Surface, SurfaceEvent, SurfaceState,
    UpdateContext, UpdateResult,
};

struct Records;

impl Surface for Records {
    fn id(&self) -> SurfaceId { SurfaceId::from("example.records") }
    fn contexts(&self) -> Vec<ContextId> { vec![ContextId::from("example.records")] }
    fn capabilities(&self) -> CapabilitySet { CapabilitySet::default() }
    fn state(&self) -> SurfaceState { SurfaceState::default() }
    fn update(&mut self, _: &SurfaceEvent, _: &mut UpdateContext<'_>) -> UpdateResult {
        UpdateResult::ignored()
    }
    fn scene(&self, area: SceneRect, _: &RenderContext<'_>) -> Scene {
        let mut scene = Scene::new();
        scene.border(area, Some(" Records ".to_owned()), "panel.border");
        scene.text(area.inset(1), "one\ntwo", "text");
        scene
    }
}
```

`apps/near-demo` is an executable non-filesystem consumer with only `near-core` and `near-ui` as direct dependencies.

The demo's `ProcessProvider` implements the object-safe `ResourceProvider` protocol for `proc://local`, then converts its provider-neutral `ResourceEntry` values into the same `CollectionSurface` and `near.collection.move` command contract used by file-oriented workspaces.

## Current Scope

The public protocol, renderer adapter, focus/peer shell, and complete M0 surface catalog are implemented. `near-fm` exercises the catalog through its panels, viewer, contextual help, filtered main menu, new-folder dialog, and tree, inspector, task, and terminal gallery surfaces. Unmatched text, paste, focus, and semantic command events route through the same surface protocol.

`TerminalSurface` defines the backend-independent terminal grid, scrollback, cursor, modes, and input-effect contract. It does not claim the process lifecycle, emulation, OSC handling, or fallback behavior required by the later embedded-PTY requirement `REQ-PTY-001`.
