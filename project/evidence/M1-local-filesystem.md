# M1 Local Filesystem Evidence

Date: 2026-06-23

## Implemented Slice

- `near-core` now models portable and extension metadata, stable identity, field-local failures, provider registries, paged listings, cancellation, generations, parent navigation, and bounded offset reads.
- `near-local-fs` implements the object-safe provider protocol for exact-byte `file://` locations.
- `near-fm` starts with live current-directory and home-directory panels without directly importing Ratatui or Crossterm.
- Enter navigates provider-backed directories and packages and resolves the typed Open association for ordinary files. Backspace resolves provider-neutral parent locations, while F3 independently opens bounded provider data in the shared viewer surface.

## Automated Evidence

- Exact path URI and reversible invalid-UTF-8 display escaping round trips are tested.
- Name-first paged listings retain generation IDs and distinguish packages and symlinks before rich metadata hydration.
- Rich stat fixtures verify device/inode identity, Unix permissions, ownership IDs, symlink targets, macOS xattrs, ACL summaries, and field-local command failures.
- Bounded offset reads and cancellation are tested.
- Capability tests prove unsupported write and list operations are absent.
- Provider-neutral classification and mutation eligibility prevent filesystem and mount roots from reaching operation planning. Protected delete attempts render a blocking denial with unmount or eject guidance; the disposable macOS image fixture proves the sentinel survives both provider and full workspace workflows.
- A 100,000-entry collection renders only viewport rows and asserts warm scene-generation p95 below 16 milliseconds.
- A headless Far workflow test navigates into a real fixture directory, returns to its parent, and views real file content through semantic commands.

## Interactive Evidence

`CARGO_HOME=/tmp/near-cargo cargo run -p near-fm` rendered live `file:///Users/turla/Code/NearManager` and `file:///Users/turla` panels in a macOS PTY. F3 displayed the real `Cargo.lock` and `README.md` contents through bounded provider reads. Esc returned to the unchanged workspace and F10 restored cursor, paste mode, and alternate-screen state.

## Requirement Status

- `REQ-FS-001` is verified by exact-byte path, package, symlink, metadata extension, and field-error fixtures.
- `REQ-FS-002` is verified for name-first listing, idle behavior, viewport virtualization, and measured warm render latency.
- `REQ-RES-001` is verified jointly with `project/evidence/M1-resource-identity.md`, including search-result, process, and plugin-item providers.

## Remaining M1 Work

- Enhanced keyboard protocol negotiation and terminal compatibility diagnostics; fail-safe lifecycle and external-tool suspension are documented in `project/evidence/M1-terminal-and-external-tools.md`.
