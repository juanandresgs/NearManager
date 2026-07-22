# M1 Streaming Viewer Evidence

Date: 2026-06-23

## Implemented Slice

- `ViewerSurface` opens provider resources through bounded `OpenRequest` ranges.
- A 64 KiB document window is shared by text and hex modes.
- Absolute byte offsets drive navigation, status, bookmarks, search, and hex rows.
- UTF-8-lossy and Latin-1 decoding, wrapping, streaming search, and numbered bookmark commands are implemented.
- F3 uses the streamed viewer rather than a truncated copied string.
- F2 and F4 toggle wrap and hex projections over the same bounded bytes, while F8 changes the explicit text decoding.
- Alt+F8 resolves absolute decimal and hexadecimal offsets, signed relative offsets, percentages, and one-based line positions without buffering the complete resource.
- F7, Shift+F7, and Alt+F7 provide bidirectional text or hex-byte search; Alt+Left and Alt+Right traverse significant positions, and Alt/Ctrl+0–9 set and jump to bookmarks.
- Provider-scoped offsets, bookmarks, and significant navigation history restore from a versioned local state document.
- Shift and Alt+Shift movement create stream and rectangular selections; Ctrl+C and Ctrl+Insert copy through the injected platform clipboard contract.
- Ctrl+Q renders quick view in the peer panel while the file panel retains focus and cursor navigation.
- `ViewerRequestTracker` cancels prior tokens and rejects non-current generations.

## Automated Evidence

- A resource eight windows long keeps `buffered_bytes()` at or below 64 KiB before and after a search and mode switch.
- Search finds a match beyond the first window and a match crossing request progression without retaining earlier windows.
- Encoding, bookmark, offset, and true byte-address hex behavior are tested.
- Viewer tests prove stream and rectangular copy semantics, provider-scoped restoration, and the bounded copy limit; a Far workflow test proves state survives close and reopen.
- Request-tracker tests prove beginning or cancelling a request invalidates older tickets.
- A Far workspace test toggles Ctrl+Q behavior, moves the collection cursor, and verifies the peer panel replaces the first file content with the second.

## Requirement Status

- `REQ-VIEW-001` is verified. Bounded streaming, delayed input-responsive quick view, cancellation, and stale-completion rejection are directly tested.
- `FAR-VIEW-002` is verified by command-path tests for every supported go-to syntax and a multi-window line seek that remains within the 64 KiB buffer budget.
- `FAR-VIEW-003` is verified by persistent provider state, bidirectional search, platform clipboard injection, and deterministic stream/rectangular selection workflows.

## Remaining Platform Extraction

- Add the standalone `near-view` application during M2 platform extraction.
