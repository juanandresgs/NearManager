# Near Local Filesystem Provider

`near-local-fs` is the macOS-first implementation of the provider-neutral `ResourceProvider` contract.

## Identity and Paths

- Resource locations are `file://` URIs produced from exact Unix path bytes.
- Bytes outside URI-safe ASCII are percent-encoded and decode without lossy Unicode conversion.
- Display names use reversible `\\xNN` escaping when a filename is not valid UTF-8.
- Local stable identity uses the filesystem device and inode rather than the display path.

## Listing and Responsiveness

- Directory listing reads names and file types without hydrating size, timestamps, ownership, xattrs, or other rich metadata.
- Listings preserve generation IDs, support page continuations, and honor cancellation.
- The workspace schedules pages and per-page metadata hydration on the bounded task runtime; first-page names become visible without waiting for later pages or rich `stat` calls.
- `CollectionSurface` renders only visible viewport rows; its 100,000-entry warm render p95 is tested below 16 milliseconds.
- Hydration workers exit after their finite page batches, so no polling worker remains active while idle.

## macOS Metadata

Rich `stat` requests report portable size, timestamps, stable identity, Unix mode, ownership IDs, hidden state, resource kind, and symlink target. Extension fields expose raw macOS xattr summaries, Finder tags, ACL output, and quarantine bytes. Failures are attached to individual metadata field IDs instead of failing the complete resource.

Package directory extensions such as `.app`, `.bundle`, `.framework`, `.kext`, `.pkg`, and `.plugin` receive the distinct `ResourceKind::Package` classification.

## Streams and Capabilities

Provider reads are offset-based, bounded to four MiB per request, report total size when available, and honor cancellation. Capability sets explicitly distinguish listable directories, readable files, writable files, inspection, rename, Trash, and directory creation.

Quick-view reads, operation execution, paged directory listing, and metadata hydration use the bounded `near-runtime` worker pool. Every collection completion is checked against its panel location and generation before it can update visible state.
