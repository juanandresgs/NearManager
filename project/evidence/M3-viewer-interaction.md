# M3 Viewer Interaction Evidence

Date: 2026-06-23

## Implemented Slice

- Viewer positions, numbered bookmarks, and significant navigation history use provider-and-location identity and persist in versioned `viewer-state.toml` storage.
- Shift+cursor creates a stream block and Alt+Shift+cursor creates a rectangular block without abandoning bounded provider reads.
- Ctrl+C and Ctrl+Insert copy selected text through the backend-independent `Clipboard` contract. The reference application injects native macOS, Windows, Wayland, or X11 clipboard commands without shell interpolation.
- Ctrl+U clears the active viewer selection, and status text reports the selection mode and cursor byte/column position.
- Provider-backed copy requests are limited to 8 MiB and return visible diagnostics when a selection exceeds the bound or clipboard access is unavailable.

## Automated Evidence

- Viewer unit tests copy exact stream and rectangular blocks into a recording clipboard.
- Viewer unit tests export and restore provider-scoped offsets, bookmarks, and navigation history.
- Local filesystem tests round-trip the versioned viewer-state document.
- The Far workspace workflow opens a real provider resource, copies a selection, saves viewer state on close, recreates the workspace, and verifies restored offset and bookmark state.

## Requirement Status

- `REQ-VIEW-001` remains verified with the expanded persistence and clipboard acceptance criteria.
- `FAR-VIEW-003` is verified.
