# Far Manager 3 File Operations, Search, and History

This reference describes the operational details behind Far’s most common workflows in official build `3.0.6703`.

## Copy, move, and rename

### Two-panel workflow

1. Navigate the active panel to the source.
2. Navigate the passive panel to the destination.
3. Select items, or leave everything unselected to process the cursor item.
4. Press `F5` to copy or `F6` to move/rename.
5. Review the prefilled destination and options.
6. Confirm conflicts and operation policy.

`Shift+F5` and `Shift+F6` process only the cursor item regardless of selection.

### Destination semantics

- A path ending with `\` explicitly requests folder creation when needed.
- Moving a folder to an existing folder places it inside that folder.
- Moving to a nonexistent destination renames/moves it to that exact path.
- Multiple destinations can be supplied where the dialog supports lists.
- Plugin panels can participate when their plugin implements get/put/move operations.

### Copy options and tradeoffs

- Use Far’s internal copier or Windows `CopyFileEx` according to System settings.
- Internal copy can handle sparse files intelligently; Windows copy can preserve extended attributes differently.
- Copying files open for writing is possible when enabled but may capture inconsistent contents.
- Symbolic-link handling can copy the link or process its target depending on options and operation context.
- Security descriptors, alternate data streams, encryption, timestamps, and attributes can trigger warnings or be governed by advanced copy settings.
- Filters can restrict which items in a selected tree are processed.
- Background/pre-scan progress options trade startup delay for total progress and time estimates.

### Overwrite handling

When a destination exists, choices include actions such as overwrite, skip, rename, append where supported, or apply a choice to all remaining conflicts. Available choices depend on whether the operation is copy or move and on source/destination types.

Far can present metadata for both source and destination to make the decision. Read-only destination handling has a separate confirmation policy.

## Deletion and wiping

| Operation | Key | Selection behavior |
|---|---|---|
| Normal deletion | `F8` | Selection, otherwise cursor item. |
| Cursor-item deletion | `Shift+F8` | Ignores selection. |
| Permanent deletion | `Shift+Del` | Bypasses Recycle Bin. |
| Wipe | `Alt+Del` | Secure-style overwrite/truncate/rename/delete sequence. |

Normal deletion uses the Recycle Bin only when enabled and supported by the local disk. Wiping overwrites file contents with zero or the configured wipe byte, truncates, renames temporarily, and deletes. It cannot guarantee erasure from every storage technology, snapshot, journal, backup, or SSD remapping layer.

## Links and reparse points

Open link creation with `Alt+F6`.

### Hard links

- Multiple directory entries reference the same file data.
- Must remain on the same volume.
- Deleting one name does not remove data until all hard links are deleted.
- Far can display and sort by hard-link count.

### Directory junctions

- Redirect a directory path to another local directory.
- Cannot target network folders.
- Supported on NTFS and useful for compatibility with applications unaware of symbolic links.

### Symbolic links

- Can point to files or directories.
- Can use relative paths.
- Can target non-local locations where Windows permits.
- Creation rights depend on Windows version, security policy, elevation, and developer mode.

Quick View and the attributes dialog display reparse targets when available. Recursive size, tree, search, and copy behavior can change substantially when symbolic-link scanning is enabled.

## Attributes and timestamps

Open with `Ctrl+A`.

Attribute checkboxes use group-aware tri-state semantics:

- Checked: set for all selected items.
- Clear: remove from all selected items.
- Indeterminate: mixed state; leave unchanged unless explicitly changed.

Filesystem-dependent attributes include compressed, encrypted, sparse, temporary, offline, reparse point, not indexed, integrity stream, and no-scrub data. Compression and encryption are mutually exclusive on NTFS.

Far can edit:

- Last-write time.
- Creation time.
- Last-access time.
- Change time.

Leave a field blank to preserve it. **Blank** clears all fields for partial-component editing, **Current** fills current time, and **Original** restores the original values for a single item. **System properties** opens the Windows properties dialog.

## Find File

Open with `Alt+F7`.

### Name and content criteria

- One or more Far masks separated by commas.
- Optional containing text.
- Case-sensitive match.
- Whole-word match using configurable separators.
- Fuzzy Unicode-aware matching that ignores diacritics and certain equivalent representations.
- Hexadecimal byte sequence.
- “Not containing” inversion.
- Search for matching folder names as well as files.

Hex search disables text-only choices such as case, words, fuzzy matching, and code-page decoding.

### Encodings

Choose:

- One specific code page.
- All standard code pages plus favorites.
- A custom checked set of code pages.

Standard multi-encoding search includes ANSI, OEM, UTF-8, UTF-16LE, and UTF-16BE. Favorite code pages are shared with editor/viewer code-page menus.

### Scope

- All non-removable drives.
- Local non-removable/non-network drives.
- Directories in `%PATH%` without recursion.
- A selected drive from its root.
- Current folder recursively.
- Current folder only.
- Selected folders.
- Plugin-emulated filesystems where supported.

### Traversal options

- Search recognized archives; significantly slower and not recursive into nested archives.
- Follow symbolic links.
- Search NTFS alternate data streams.
- Apply reusable filters.

### Advanced criteria

- Search only within the first specified portion of each file.
- Size ranges using binary uppercase units (`K`, `M`, `G`, `T`, `P`, `E`) or decimal lowercase units.
- Date and time ranges.
- Attribute conditions.
- Custom result columns using the same size/date/owner/link/stream concepts as panel modes.

## Search results workflow

Search remains interactive while it is running:

- **New search**: begin another search.
- **Go to**: stop and position a panel on the selected result.
- **View**: inspect a result while search continues in the background.
- **Panel**: send the last result set to a temporary panel.
- **Stop**: halt but keep current results.
- **Cancel**: close.

Result keys:

- `F3`, `Alt+F3`, `Numpad5`: configured viewer behavior.
- `Ctrl+Shift+F3`: force internal viewer.
- `F4`, `Alt+F4`: configured editor behavior.
- `Ctrl+Shift+F4`: force internal editor.

Saving edits to files from plugin-emulated search results may become Save As because the plugin cannot write the source directly.

### Results-as-panel convention

Sending results to a panel is a major Far workflow:

1. Search broadly.
2. Turn the matches into a normal-looking temporary panel.
3. Sort, filter, select, view, edit, copy, delete, or run Apply Command on the set.
4. Preserve each item’s original path while working with them as one collection.

## Find Folder

Open with `Alt+F10`. It searches the cached folder tree rather than file contents.

- Type incrementally to locate a folder.
- Move among matches.
- Confirm to change the panel folder.
- Refresh the tree cache when external changes make it stale.

This is faster than a full filesystem content search but depends on current `tree3.far` data.

## Compare folders

The built-in command requires two file panels. It marks:

- Items existing on only one side.
- The newer copy when matching names differ by size/time.

It compares name, size, and timestamp only, does not compare contents, and does not recurse. Use the bundled Compare plugin for recursion, content comparison, timezone tolerance, or whitespace/EOL normalization.

## Regular expressions

Far supports regular expressions in Find File, editor search/replace, associations, filters, and other mask-aware contexts when regex mode is offered.

Supported concepts include:

- Character classes and negated classes.
- Quantifiers and lazy variants.
- Groups and alternation.
- Numbered and named captures.
- Anchors and word boundaries.
- Lookahead/lookbehind where documented.
- Options controlling case and multiline behavior.

Editor replacement can reference captures and transform or preserve case according to replacement options. Association commands can reference captures with `%RegexGroupN` or `%RegexGroup{Name}`.

Because Far’s regex syntax and flags are version-matched, use the built-in `Regular expressions` and `Regular expressions in replace` help topics for complex expressions rather than assuming exact PCRE behavior.

## Histories

### Command history (`Alt+F8`)

| Action | Key |
|---|---|
| Execute | `Enter` |
| Execute separately | `Shift+Enter` |
| Execute elevated | `Ctrl+Alt+Enter` |
| Copy to command line | `Ctrl+Enter` |
| Clear unlocked entries | `Del` |
| Lock/unlock | `Ins` |
| Delete current unlocked entry | `Shift+Del` |
| Copy text without closing | `Ctrl+C`, `Ctrl+Ins` |
| Show details | `F3` |

From the command line, `Ctrl+E` / `Ctrl+X` move through previous/next commands.

### View/edit history (`Alt+F11`)

- `Enter`: reopen using its previous view/edit context.
- `F3` / `Numpad5`: viewer.
- `F4`: editor.
- `Ctrl+Enter`: copy pathname to command line.
- `Ctrl+R`: remove missing entries; remote paths may make refresh slow.
- `Shift+Enter`: open without moving the entry to the newest position.
- `Ins`: lock against clearing/refresh removal.

### Folder history (`Alt+F12`)

- `Enter`: open in active panel.
- `Ctrl+Shift+Enter`: open in passive panel.
- `Ctrl+Enter`: copy path to command line.
- `Ctrl+R`: remove missing entries.
- `Shift+Enter`: open without reordering.
- `Ins`: lock.

Persistence for all three histories is independently configurable.

## Task and device lists

### Task list

Open from Commands or with `Ctrl+W`, including from viewer/editor.

- `Enter`: switch to a task window.
- `Del`: terminate a task immediately.
- `Ctrl+R`: refresh.
- `F2`: switch between window caption and executable path.

The bundled ProcList plugin provides a deeper process-oriented virtual panel.

### Hotplug devices

- Select a device and press `Del` to request safe removal.
- `Ctrl+R` refreshes the list.

Availability and device categories depend on Windows hardware APIs.

## Elevation

When an operation fails for access rights and policy allows, Far can retry through an elevated helper. Policies distinguish modifications, read-only access, and additional privileges that may bypass ACL checks.

Elevation does not change the entire existing Far process into an administrator session; it brokers the requested operation. Commands can also be explicitly launched elevated with `Ctrl+Alt+Enter`.

