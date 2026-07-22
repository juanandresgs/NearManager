# M3 Quick View Evidence

Date: 2026-06-23

## Implemented Slice

- Ctrl+Q keeps quick view passive while the source panel navigates.
- Ctrl+Shift+Q gives the preview the complete viewer context without changing source identity.
- Standard viewer navigation, search, wrap, hex, encoding, go-to, bookmarks, and history commands operate in quick view.
- Esc or Ctrl+Shift+Q returns to file-panel navigation without closing the preview.
- Directories and packages receive bounded asynchronous summaries with counts, sizes, errors, continuation state, and names.
- File and directory preview tasks share generation tickets, cancellation, and stale-completion rejection.

## Automated Evidence

- `workspace::tests::quick_view_summarizes_directories_and_exposes_viewer_navigation_and_search` proves directory summaries, control-mode context, page navigation, F7 search, and return to the file panel.
- `workspace::tests::quick_view_tracks_the_cursor_in_the_peer_panel` proves passive cursor-driven preview replacement.
- `workspace::tests::delayed_quick_view_remains_responsive_and_rejects_stale_completion` proves a slower cancelled result cannot replace the current preview.
- Viewer unit tests cover text/hex navigation, forward/reverse search, go-to, encodings, bookmarks, and bounded windows reused by control mode.

## Requirement Status

`FAR-VIEW-004` is verified by full viewer controls, provider-backed directory summaries, and stale-safe asynchronous loading.
