# M3 Description Evidence

Date: 2026-06-23

## Implemented Slice

- `ResourceProvider` exposes optional description mutation and folder-description lookup while `near.description` remains provider-neutral metadata.
- The configured local provider reads ordered sidecar names, parses quoted filenames, hides catalogs by policy, and enriches listing/stat metadata.
- Description panel columns render catalog text rather than generic kind labels.
- Ctrl+Z and F9 commands edit selected/current descriptions and view or create/edit configured folder-description files through the internal viewer/editor.
- UTF-8, UTF-8 BOM, and Latin-1 behavior is explicit; an existing BOM takes decoding precedence.
- Local copy, move, rename, trash, and delete execution synchronizes top-level description entries.

## Automated Evidence

- Provider tests prove BOM precedence, quoted names, catalog hiding, editing, configurable folder filenames, BOM creation, and copy/rename synchronization.
- Workspace tests prove description-column rendering, asynchronous edits, refresh, and folder-description viewing.
- Layered configuration tests include the shipped `descriptions.toml` document.

## Requirement Status

`FAR-OPS-007` is verified by configurable display/edit workflows, folder-description access, explicit encoding, and operation-aware catalog maintenance.
