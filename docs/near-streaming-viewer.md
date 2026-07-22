# Near Streaming Viewer

`ViewerSurface` is an embeddable, provider-neutral text and hex viewer. It consumes `ResourceProvider::open` ranges rather than local paths or complete files.

## Bounded Document Window

Provider-backed viewers retain one 64 KiB window, its absolute byte offset, optional total size, and completion state. Navigation reloads another bounded range when the requested offset leaves the current window. Static text remains supported for help, diagnostics, and application-owned content.

Text and hex modes share the same document bytes and absolute cursor offset. Switching mode therefore does not duplicate content or invent line-derived offsets. Hex rows include absolute byte offsets, hexadecimal bytes, and a printable ASCII projection.

## Text Behavior

- Automatic BOM/NUL-pattern detection plus explicit UTF-8-lossy, UTF-16LE, UTF-16BE, and Latin-1
  decoding. Encoded newline navigation and text search retain provider byte offsets.
- Character wrapping constrained by the current viewport width.
- Line and page navigation backed by byte offsets.
- Start, end, and explicit offset state.
- Interactive F7 search entry, Shift+F7 continuation, and Alt+F7 reverse search.
- Text search honors the selected encoding; hex mode accepts whitespace-separated byte pairs.
- Streaming forward and reverse search retain bounded windows and match across window boundaries.
- Alt+F8 accepts absolute decimal or hexadecimal offsets, signed relative offsets, percentages, and one-based line positions such as `L120` or `line:120`.
- Alt+Left and Alt+Right traverse significant viewer positions without disturbing the document stream.
- Ten numbered bookmark set and jump commands are exposed through Alt+0–9 and Ctrl+0–9.
- Provider-scoped offsets, bookmarks, and significant-position history are saved in `viewer-state.toml` and restored when the same resource is reopened.
- Shift+cursor extends a stream selection; Alt+Shift+cursor extends a rectangular selection. Ctrl+C or Ctrl+Insert copies through the platform clipboard, and Ctrl+U clears the block.
- Copy reads are separately bounded to 8 MiB and report an explicit diagnostic rather than silently loading an unbounded resource.

## Far Integration

F3 opens the focused provider resource in the streamed viewer. F2 toggles wrapping, F4 toggles text and hex, F7 starts search, Shift+F7 and Alt+F7 move between matches, F8 cycles encoding, and Alt+F8 opens go-to-position.

Per-resource persistence has an explicit master switch and independent position/history, bookmark,
resolved-encoding, and wrap/hex controls. Disabled fields fall back to the current global defaults;
the viewer does not write any state when the master switch is off.

Ctrl+Q toggles quick view. The focused collection remains interactive while the opposite panel renders the streamed viewer. Cursor movement creates a new `ViewerLoadTicket`, cancels the previous token, and accepts a result only when its generation remains current.

Ctrl+Shift+Q enters quick-view control mode. The preview receives the standard viewer context, including line and page navigation, Home/End, wrap and hex toggles, F7 search, next/previous search, go-to, encoding changes, bookmarks, and navigation history. The quick-view panel receives focused styling while controls are active. Esc or Ctrl+Shift+Q returns input to the file panel without closing the preview.

Directories and provider packages use a bounded asynchronous list request instead of a byte stream. The preview reports location, visible item count, directory/package/file/link counts, known size, metadata-error count, continuation state, and the first page of names. Providers may still replace this generic summary with richer previews through their own resource representation.

## Runtime Behavior

Quick-view reads and directory summaries run on the bounded `near-runtime` worker pool. Cursor movement cancels the previous task without blocking input, while generation checks prevent a provider that ignores or races cancellation from rendering a late stale result. Full-view continuation reads currently occur during viewer commands; initial F3 and all cursor-driven quick-view loads remain bounded in memory, and the quick-view acceptance path is fully asynchronous.

## Verification

Tests prove a multi-window resource never buffers more than 64 KiB, text and hex share the same window, forward and reverse search work in text and hex modes, every go-to syntax resolves deterministically, line seeking remains bounded, stream and rectangular selections copy exact text, provider-scoped bookmarks and navigation restore after reopen, and a delayed 100 ms quick-view result cannot block or replace a newer 5 ms cursor result.
