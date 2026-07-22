# Near Archive Providers

Near treats archives as resource providers and operation backends, not as file-manager-specific dialogs. `ResourceProvider::mount` lets a registered provider claim a foreign resource and return a collection location. The initial implementation, `near-archive::ZipArchiveProvider`, claims local `.zip` resources and exposes them below `archive://zip/...`.

## Navigation and Identity

Archive locations encode the source `Location` and normalized inner path. Rows preserve the source archive, entry path, compressed size, stable provider-scoped identity, kind, and uncompressed size. Implicit parent directories are synthesized from entry paths. Normal parent navigation stays inside the archive until its root, then crosses back to the source archive's local parent through provider lookup.

Entry reads are bounded and cancellation-aware. ZIP paths must be enclosed relative paths; absolute paths, drive prefixes, and `..` traversal entries are omitted. Entry and archive-wide extraction ceilings prevent unbounded expansion.

## Operations

`ArchiveOperationService` wraps an ordinary fallback `OperationService`. It claims only these cross-provider intents:

- archive resources copied to a local destination are extraction plans;
- local resources copied to an archive destination are create/update plans;
- archive moves are rejected so deletion remains an explicit separate action.

Archive plans use provider-namespaced IDs and the shared immutable `OperationPlan`, preview surface, authorization, conflict resolver, cancellation token, item outcomes, and journal. `Skip`, `Replace`, and `Rename` therefore behave like other Near operations.

Extraction rejects ZIP symlinks and writes each file through a temporary path before rename. Creation and update rebuild a temporary ZIP and replace the archive only after the writer finishes, using a recovery backup where the platform cannot overwrite atomically. Local symlink sources are rejected. Existing entries are retained unless the resolved conflict action replaces them.

## Capabilities

Archive formats advertise container capabilities independently of file extensions:

- local collection locations receive `archive.create` when a registered format can create there;
- mounted ZIP directories receive `archive.update`;
- archive files expose `resource.read` and `resource.copy`;
- archive directories expose `resource.list`, `resource.copy`, and `archive.update`.

The Files menu's **Create archive** command and copy-to-archive availability consult these capabilities. Additional formats can implement the same mount, container-capability, provider, and operation contracts without changing workspace navigation or dialogs.
