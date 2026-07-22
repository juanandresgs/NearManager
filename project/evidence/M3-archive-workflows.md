# M3 Archive Workflow Evidence

- `ResourceProvider::mount` and `ProviderRegistry::mount` provide a generic foreign-resource-to-collection contract.
- `ResourceProvider::create_container` and container capabilities make archive creation format-driven rather than extension logic in the workspace.
- `ZipArchiveProvider` supplies paged collection listing, parent navigation, metadata, identity, bounded reads, and traversal filtering.
- `ArchiveOperationService` wraps the local operation service and assigns provider-namespaced immutable plan IDs.
- Unit tests browse nested ZIP entries, omit traversal names, read bounded content, extract with rename conflicts, add and replace entries, and reject unsupported creation formats.
- A workspace test opens a ZIP panel, extracts through `OperationPreviewSurface`, and creates a ZIP through the registered command.
