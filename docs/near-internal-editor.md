# Near Internal Editor

Near's internal editor is a full-screen `Surface` backed by the same provider-neutral resource contract as panels and the viewer. `F4` opens it for a file whose provider advertises `resource.write`; `Alt+F4` invokes external handlers through `near.resource.edit-external`.

## Implemented Baseline

- UTF-8, UTF-16LE, UTF-16BE, and Latin-1 loading with BOM detection and CRLF/CR normalization in memory.
- Provider-backed save through `ResourceProvider::write`; the local filesystem provider checks the opened resource version, stages replacement content, and honors cancellation.
- Character, line, page, home, and end navigation with a visible line/column cursor.
- Text insertion, newline, indentation, backspace, delete, terminal paste, bounded undo, and redo.
- Literal and Rust-regex search with validated diagnostics and repeat-next behavior.
- Staged replace prompts, capture expansion (`$1` and `${name}`), replace-all, and optional upper/lower/title style preservation.
- A full-screen navigable Find All result list that activates exact source rows and columns.
- Shift+cursor stream selection and Alt+Shift+cursor rectangular selection, including column-preserving cut, copy, and padded paste.
- Current-line copy when no block is active, explicit block clearing, and editor-local clipboard operations.
- Versioned layered `editor.toml` settings captured per newly opened editor. Tab width controls
  display stops; `expand_tabs = true` inserts spaces to the next stop, while `false` preserves a
  literal tab byte. Reloaded defaults do not silently rewrite an open editing session.
- Dirty-state marking and a two-step discard guard; `Ctrl+S` clears dirty state only after provider success.
- `F2` saves in the current format; `Shift+F2` or `Ctrl+Shift+S` opens provider-neutral Save As with explicit provider location, encoding, BOM, EOL, replacement, and lossy-conversion choices.
- Latin-1 conversion refuses unrepresentable characters until the user explicitly confirms replacement with `?`.
- A provider version conflict opens reload, line-comparison, and keep-local overwrite choices without discarding either version implicitly.
- Multiple retained editor sessions, `F12` screen selection, and `Ctrl+Tab` / `Ctrl+Shift+Tab` cycling through panels and documents.
- Provider-scoped cursor and viewport positions persisted through `EditorPositionStore` and restored after restart.
- Application quit is rejected while any retained editor is dirty, with every unsaved document named.
- A dedicated `surface.editor` keymap context, command registration, generated key-bar hints, and full-screen semantic rendering.

## Provider Contract

`WriteRequest` contains exact replacement bytes, the expected size/modified version, and a cancellation token. Providers that cannot safely replace a resource retain the default `Unsupported` implementation. The UI checks `resource.write` before opening the editor and never converts provider locations to native paths. Normal save carries the opened version. Save As requires explicit create/replace approval and writes a selected provider resource without assuming a native path. A version mismatch returns `ProviderError::Conflict` and leaves external content untouched until reload, compare, or keep-local is chosen.

## Remaining Far-Parity Work

Remaining advanced editor work includes viewer participation in the shared screen list, platform system-clipboard adapters for editor cut/copy/paste, block append/indent/move commands, and multiline search options.
