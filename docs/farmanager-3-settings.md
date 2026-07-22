# Far Manager 3 Settings Reference

This document summarizes the user-facing settings in official build `3.0.6703.0`. Settings changed from inside an already-open viewer or editor can apply only to the current session/window; settings changed through the main Options menu establish defaults for future instances.

## Options menu map

| Menu item | Controls |
|---|---|
| System settings | Copy/delete implementation, histories, Windows associations, elevation, sorting, automatic saving. |
| Panel settings | Visibility, selection, sorting behavior, refresh, titles, status, totals, scrollbars. |
| Tree settings | Automatic folder synchronization and tree-cache threshold. |
| Interface settings | Mouse, key/menu bars, progress UI, rendering, title, icon, screen saver. |
| Languages | Interface and help language. |
| Plugins configuration | Each plugin’s own settings. |
| Plugin manager settings | Legacy plugins, discovery, associations, prefixes, result handling. |
| Dialog settings | History, blocks, autocomplete, deletion, outside-click behavior. |
| Menu settings | Mouse behavior outside menus. |
| Command line settings | Blocks, deletion, completion, prompt, home directory. |
| AutoComplete settings | List presentation and completion sources. |
| InfoPanel settings | Information-panel sections and formatting. |
| Groups of file masks | Reusable named mask sets. |
| Confirmations | Safety prompts for destructive, overwrite, history, device, and exit actions. |
| File panel modes | Custom panel and status-line columns. |
| File descriptions | Description filenames and update policy. |
| Folder description files | Folder-description filenames. |
| Viewer settings | Internal/external viewer, display, saved state, encoding. |
| Editor settings | Internal/external editor, tabs, selection, display, saved state, encoding. |
| Code pages | Favorites and available encodings. |
| Colors | UI color groups and themes. |
| Files highlighting and sort groups | Conditional colors, marks, and ordered groups. |
| Save setup | Persist configuration, colors, and screen layout when needed. |

## System settings

### File operations

- **Delete to Recycle Bin**: sends eligible deletions on local fixed disks to the Windows Recycle Bin. `Shift+Del` still bypasses it.
- **Use system copy routine**: uses Windows `CopyFileEx`, which can preserve extended attributes but does not provide Far’s sparse-file-aware copy behavior.
- **Copy files opened for writing**: permits potentially inconsistent snapshots of files being modified by another process.
- **Scan symbolic links**: traverses symbolic links while building trees or calculating directory sizes; can create loops or surprising totals and should be used deliberately.

### Persistence and integration

- Save command history.
- Save folder history.
- Save viewed/edited-file history.
- Use Windows registered file types when no Far association matches.
- Automatically refresh environment variables after global environment changes.
- Automatically save setup and both panels’ current folders.

### Elevation

“Request administrator rights” is a multi-part policy:

- Request for modifications.
- Request for read-only access failures.
- Attempt additional backup/restore-like privileges that bypass normal ACL checks.

The last option is especially powerful and should be enabled only when its security consequences are understood.

### Sorting collation

- **Ordinal**: compare character values directly.
- **Invariant**: locale-independent collation.
- **Linguistic**: culture-specific collation.
- **Treat digits as numbers**: makes `5.txt` sort before `11.txt`.
- **Case sensitive**: incorporates case according to the chosen collation.

## Panel settings

### Content and selection

- Show hidden and system files (`Ctrl+H`).
- Enable file highlighting.
- Allow keypad group operations to select folders as well as files.
- Make right-click select files; when disabled, right-click opens the Explorer context menu.
- Sort folder names by extension rather than by name while in extension sort mode.
- Allow reselecting the current sort mode to reverse it.

### Refresh behavior

- Disable automatic refresh when a directory exceeds a configurable object count; zero means always refresh.
- Enable or disable network-drive auto-refresh separately for performance on slow connections.
- `Ctrl+R` always forces a refresh.

### Panel chrome and metadata

- Column titles.
- Status line.
- Detection of volume mount points versus ordinary junctions; detection can be slow on networks.
- File totals.
- Free-space display.
- Scrollbar.
- Number of background screens.
- Sort-mode indicator letter.
- `..` entry in filesystem roots; activating it opens the drive menu.

## Tree settings

- **Auto change folder**: moving the tree cursor immediately changes the other panel; otherwise `Enter` confirms the change.
- **Minimum number of folders**: threshold for creating the `tree3.far` cache.
- Advanced `far:config` can disable tree functionality completely.

## Interface settings

- Show clock.
- Enable mouse.
- Show the function-key bar (`Ctrl+B`).
- Always show the top menu bar.
- Start a screen saver after inactivity; moving the pointer to the upper-right corner can trigger it.
- Show total copy progress, with the tradeoff that Far may need a pre-scan.
- Show transfer speed, elapsed time, and remaining-time estimates.
- Show total delete progress, also potentially requiring a pre-scan.
- Use `Ctrl+PgUp` at roots to open the drive menu; a third state integrates network share navigation when the Network plugin is present.
- Use Virtual Terminal rendering for ANSI escape sequences, 8/24-bit color, text styles, and modern terminal behavior on Windows 10+.
- Enable experimental fullwidth-aware rendering for East Asian characters.
- Use ClearType-friendly redraw at a possible performance cost.
- Select an embedded console icon and optionally use the red alternative when elevated.
- Add text and variables to the Far window title: `%Ver`, `%Platform`, `%Admin`, `%PID`, and ordinary environment variables.

## Dialog settings

- Keep history in dialog edit controls.
- Keep text selections after cursor movement.
- Make `Del` remove a selected block rather than the character under the cursor.
- Enable automatic completion in history-backed edit controls and combo boxes; with it disabled, `Ctrl+End` can still invoke completion manually.
- Make `Backspace` clear an unchanged prefilled value.
- Permit or suppress closing a dialog by clicking outside it.

History can be disabled for privacy-sensitive fields and workflows.

## Menu settings

For left, right, and middle clicks outside a menu, independently choose:

- Cancel the menu.
- Execute the selected item.
- Do nothing.

## Command-line settings

- Persistent text blocks.
- `Del` removes selected blocks.
- Automatic completion; if disabled, `Ctrl+Space` invokes it manually.
- Custom prompt format.
- Home-directory target for `cd ~`; the default `%FARHOME%` points to the Far program directory.

## AutoComplete settings

Presentation options:

- Show suggestions as a list.
- Make the list modal.
- Append the first match while typing.

Completion sources are advanced three-state settings: enabled, disabled, or manual-only (`Ctrl+Space`). Sources are:

- Filesystem.
- History.
- `PATH` executables.
- Environment variables.

## Confirmations

Far can independently ask before:

- Overwriting during copy.
- Overwriting during move.
- Overwriting or deleting read-only items.
- Drag-and-drop operations.
- Deleting files.
- Deleting folders.
- Interrupting an operation.
- Disconnecting network drives.
- Removing `SUBST` drives.
- Detaching virtual disks.
- Ejecting USB storage.
- Reloading an externally changed editor file.
- Clearing command, folder, or view/edit histories.
- Exiting Far.

These controls allow a high-confirmation interactive profile or a lower-friction expert profile without disabling all safeguards globally.

## Viewer settings

### External viewer

- Swap `F3` and `Alt+F3` so the external viewer is primary.
- Define the external viewer command using Far metasymbols.
- A matching Far file association takes precedence over the generic external viewer command.

### Internal viewer

- Persistent selection.
- Tab width.
- Horizontal-overflow arrows.
- Visible glyph for NUL bytes; the glyph itself is configured in `far:config`.
- Scrollbar (`Ctrl+S`).
- Save file position.
- Save manually selected code page; automatically enabled with saved positions.
- Save bookmarks.
- Maximum text line width from 100 to 100,000 columns, default 10,000.
- Save text/hex/dump view mode.
- Save wrap and word-wrap state.
- Detect binary files and start in dump mode.
- Autodetect code page.
- Default code page.

## Editor settings

### External editor

- Swap `F4` and `Alt+F4` so the external editor is primary.
- Define the external editor command using Far metasymbols.
- A matching Far file association takes precedence.

### Tabs and indentation

- Keep tabs unchanged.
- Expand only newly entered tabs.
- Expand all tabs when opening.
- Tab width.
- Automatic indentation.

### Editing behavior

- Persistent blocks.
- `Del` removes the selected block.
- Show spaces, tabs, and line breaks.
- Permit cursor movement beyond line ends.
- Select found text.
- Place the cursor at the end of a match.
- Scrollbar.
- Line numbers; can be toggled with `Ctrl+F3` while editing.

### Persistence and safety

- Save file positions and manually selected code pages.
- Save bookmarks.
- Allow editing files another process has open for writing.
- Lock modification of read-only files as if `Ctrl+L` were active.
- Warn when opening read-only files.
- Autodetect code page.
- Default code page for new/opened files.

## Plugin manager settings

- Support old Far 1.x OEM/non-Unicode plugins.
- Scan symbolic-link directories while discovering plugins.
- Control file-processing plugin association behavior.
- Show the standard association item alongside plugin handlers.
- Show a chooser even when only one plugin is available.
- Control handling of search-result lists supplied by plugins.
- Control command-prefix processing.

Legacy plugin support exists for compatibility but cannot provide all Far 3 Unicode and API capabilities.

## File-mask groups

Named groups can contain masks and other groups. Use `<name>` anywhere a normal mask is accepted.

Example: `<arc>|*.rar` means the predefined archive group excluding RAR files.

| Action | Key |
|---|---|
| Restore predefined `arc`, `temp`, `exec` groups | `Ctrl+R` |
| Add group | `Ins` |
| Delete group | `Del` |
| Edit group | `Enter`, `F4` |
| Find groups containing a mask | `F7` |

## Advanced `far:config`

The advanced editor covers settings intentionally absent from normal dialogs, including:

- Completion source states.
- copy buffer, time, stream, security, and substitution behavior.
- command executor quoting, arguments, code pages, exclusions, and history.
- editor file-size limits, BOM/EOL defaults, undo memory, read-only locking, and search-result formats.
- viewer zero-byte display and search wrapping.
- mouse-wheel deltas by area.
- tree disabling and cache details.
- transliteration layouts, tables, separators, and flags.
- title formats, cursor sizes, numeric separators, redraw timing, and menu scrollbars.
- macro indicators and hidden-drive policy.

Use the context-sensitive help for each parameter before changing it; many values are implementation controls rather than simple preferences.

