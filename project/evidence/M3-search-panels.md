# M3 Search Panel Evidence

Date: 2026-06-23

## Implemented Slice

- Search results remain exact source resources, so internal view, external edit, canonical operation targeting, and reveal use the original provider.
- The search dialog supports replace, append, and refine modes. Append deduplicates by source identity; refine evaluates only the current result collection.
- A result collection can be kept as a session-persistent generated panel and reopened later in either focused side.
- Kept panels share the live result provider, so background batches remain available even when the visible panel navigates elsewhere.

## Automated Evidence

- `near-search` tests prove duplicate-safe appends, snapshots, replacement, and exact source references.
- The Far workspace workflow proves non-blocking search, view/edit routing, reveal, keeping and reopening a generated result panel, duplicate-safe repeated append, and refinement of the existing result set.
- Shipped keymap validation covers `Alt+F7`, `Ctrl+Alt+F7`, and `Ctrl+Shift+Alt+F7` as configurable bindings.

## Requirement Status

- `FAR-SEARCH-004` is verified for actionable results, session-persistent generated panels, and repeated append/refine workflows. Far Temporary Panel parity is tracked separately by `FAR-EXT-004`.
