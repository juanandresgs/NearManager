# Near Reference Providers

`near-reference-providers` supplies production-quality non-filesystem examples that exercise the provider-neutral resource contract without introducing plugin-runtime or search-engine dependencies.

## Process Provider

`ProcessProvider` exposes `proc://local` as a paged collection. Each process resource uses `near.process` plus its PID address, provider-scoped stable identity, portable virtual metadata, typed process extensions, bounded reads, and only read/inspect capabilities.

The command name is display metadata, not identity. A changed command string for the same live PID retains the same `ResourceRef` and `ResourceIdentity`.

## Search Results

`SearchResultsProvider` exposes a `search://sessions/...` collection but deliberately emits each result's original source `ResourceRef`. The collection provider owns pagination and cancellation; view, edit, inspect, and mutation commands continue resolving through the source provider.

Search metadata adds session and source-provider extensions while retaining the source stable ID. The search provider itself advertises no item capabilities because it does not impersonate the source provider.

## Plugin Catalog

`PluginCatalogProvider` exposes `plugin://catalog` and stable plugin resources keyed by plugin ID rather than display name. Catalog items provide versioned extension metadata, bounded descriptor reads, and explicit read/inspect/activate capabilities. Filesystem mutation capabilities are absent.

## Removable Devices

`RemovableDeviceProvider` exposes `device://attached` using a platform service rather than embedding operating-system commands in the provider. Each row retains exact native identity and mount metadata. Only rows currently reported as safely disconnectable advertise `device.disconnect`; the workspace rechecks that capability before calling the service. See [Near Removable-Device Panels](near-device-panels.md).

## Durable Identity

`ResourceIdentity` combines a provider ID with provider-supplied stable identity. `ResourceMetadata::identity_for` never derives durable identity from a display name or location. Providers that cannot supply a stable identity return `None` rather than inventing one.

## Reference Application

`near-demo` renders process and plugin collections side by side through the same `CollectionSurface`, `SurfaceShell`, semantic scene, theme, focus, and peer contracts used elsewhere. Search-result tests use the same collection commands while preserving source-provider identity.
