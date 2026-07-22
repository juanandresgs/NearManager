# Far Manager 3: Feature, Workflow, Menu, and Hotkey Research

## Scope and evidence

This document describes the Windows Far Manager 3 user experience: core capabilities, interaction model, menus, common workflows, keyboard conventions, configuration surfaces, and extension model. It is based primarily on the English help shipped in the official FarGroup/FarManager repository.

Research snapshot:

- Official repository: <https://github.com/FarGroup/FarManager>
- Source commit: `28443e51d9f7b4b9e5976d1db8246532ccc40205` (2026-06-20)
- Canonical user help: `far/FarEng.hlf.m4`
- Official project description: <https://github.com/FarGroup/FarManager#readme>
- Official macro manual: <https://documentation.help/macroapi/>
- Official plugin API documentation: <https://fargroup.github.io/api/>
- Build-matched hotkey appendix: `docs/farmanager-3-hotkeys.md`
- Startup/profile appendix: `docs/farmanager-3-startup-and-environment.md`
- Bundled-plugin appendix: `docs/farmanager-3-bundled-plugins.md`
- Settings appendix: `docs/farmanager-3-settings.md`
- LuaMacro automation appendix: `docs/farmanager-3-luamacro.md`
- Advanced panel/custom workflow appendix: `docs/farmanager-3-advanced-workflows.md`
- File operations/search/history appendix: `docs/farmanager-3-file-operations-and-search.md`

Far Manager changes continuously, including frequent development builds. Features described here are therefore tied to the snapshot above rather than to an older static “Far 3” release.

## Product model

Far Manager is a keyboard-first, text-mode file and archive manager for Windows. Its main screen combines:

1. Two independently configurable panels.
2. A live command line below the panels.
3. A context-sensitive function-key bar.
4. A top menu opened with `F9`.
5. Built-in viewer, editor, search, history, help, and configuration interfaces.
6. Plugin panels and commands that behave like native parts of the program.
7. Lua-based macros that can automate or replace normal key sequences.

The active panel owns the cursor and most file operations. The other is the passive panel and commonly supplies a source or destination. `Tab` changes the active panel; `Ctrl+U` swaps the panels themselves.

## Supported feature families

### File and folder management

- Navigate local, removable, network, substituted, and plugin-provided locations.
- Copy, move, rename, delete, wipe, and create folders.
- Create hard links, symbolic links, and directory junctions where Windows permits them.
- Change file attributes and timestamps.
- Preserve or control security information, alternate streams, encryption, and overwrite behavior during copy operations.
- Select files by direct marking, wildcard masks, same name, same extension, inversion, filters, or saved selection.
- Apply an arbitrary command to selected items using filename metasymbols.
- Add and use file and folder descriptions.
- Compare the folders shown in the two panels.
- Drag selected items between panels with mouse-based copy or move.

### Panels and navigation

- File, tree, information, and quick-view panel types.
- Ten default file-panel view modes, plus user-defined column layouts.
- Sorting by name, extension, timestamps, size, description, owner, allocation size, hard-link count, streams, full name, and custom/plugin criteria.
- Sort groups, file highlighting, filters, selected-first ordering, directories-first ordering, numeric sorting, case-sensitive sorting, and reverse order.
- Folder shortcuts on digits `0`–`9`.
- Folder history, viewed/edited-file history, and command history.
- Fast incremental filename lookup in a panel.
- Change-drive menu that also exposes plugin entry points and disk metadata.
- Screen switching among panels, viewer, and editor instances.

### Search and discovery

- Find files by mask, text, code page, scope, attributes, size, date, and advanced criteria.
- Search in archives when supported by the active archive plugin.
- Search using plain text or regular expressions.
- Present results in a menu, send results to a panel, jump to the containing folder, view, or edit a result.
- Find folders using the folder tree database.
- Search and replace inside the editor, including regular expressions and replacement groups.
- Search inside the viewer in text or hexadecimal modes.

### Viewer

- Text, hexadecimal, and dump-style viewing modes.
- Automatic and explicit code-page selection.
- Search, wrapping, go-to-position, bookmarks, navigation history, and selection/copy.
- Quick view embeds viewer behavior in a panel.
- Plugin interception allows specialized viewers for supported formats.

### Editor

- Multi-file internal editor with persistent edit positions and undo/redo.
- Stream and column selections.
- Search, replace, regular expressions, “find all,” and style-preserving replacement.
- Code-page and BOM handling, EOL selection, tab settings, auto-indent, persistent blocks, and read-only locking.
- Save As, reload changed files, go to line/column, bookmarks, and session screen switching.
- Plugin integration for syntax coloring, completion, formatting, sorting, case conversion, alignment, and other editing operations.

### Command execution

- A command prompt remains available while panels are visible.
- Windows commands, executables, scripts, and file associations can be launched directly.
- Panel paths and selected filenames can be inserted into the command line with hotkeys.
- Command history supports locked items, editing, deletion, and replay.
- User-defined file associations can define execute, view, edit, and alternate actions.
- Environment variables supplied by Far expose active/passive panel state and profile locations.
- `far:` service commands expose built-in operations such as configuration and version information.

### Extensibility

- Native plugins extend panels, menus, viewer/editor behavior, commands, archives, network access, and configuration.
- Plugins can add command-line prefixes and virtual file systems.
- `F11` opens the plugin commands menu; context determines which plugin commands are available.
- `Alt+Shift+F9` opens plugin configuration; `Shift+F1` opens plugin help where supported.
- LuaMacro provides recorded keyboard macros, scripted macros, events, custom menu items, command prefixes, and panel modules.
- Macros are scoped by interaction area, including shell panels, editor, viewer, dialogs, menus, help, drive menu, quick view, tree, search, and autocompletion.

The source tree snapshot includes plugin projects for ArcLite, AutoWrap, brackets, Compare, DrawLine, EditCase, EMenu, Far commands, FileCase, FTP, LuaMacro, MacroView, MultiArc, Network, Process List, Same Folder, Temporary Panel, and others. Presence in the source tree does not guarantee that every plugin is enabled or packaged in every binary distribution.

## Core interaction conventions

### Active and passive sides

- Commands normally act on the active panel’s current or selected items.
- Copy and move default to the passive panel’s current folder.
- Several key pairs distinguish active versus passive panel data.
- When an information, tree, or quick-view panel is active, directory changes apply to the opposite file panel.

### Current item versus selection

- If files are selected, file operations process the selection.
- If nothing is selected, they process the item under the cursor.
- `Ins`, `Shift+Up`, `Shift+Down`, and right-click toggle selection.
- Numeric keypad `+`, `-`, and `*` mean select group, deselect group, and invert.
- `Ctrl+M` restores the previous selection after an operation or group-selection change.

### Function keys and modifiers

The unmodified function-key layer follows the classic file-manager pattern:

| Key | Main panel action |
|---|---|
| `F1` | Help |
| `F2` | User menu |
| `F3` | View; on a folder, calculate/show size |
| `F4` | Edit |
| `F5` | Copy |
| `F6` | Rename or move |
| `F7` | Create folder |
| `F8` | Delete |
| `F9` | Main menu |
| `F10` | Exit |
| `F11` | Plugin commands |
| `F12` | Screen list |

Modifier layers are systematic rather than arbitrary:

- `Ctrl+F3`…`Ctrl+F12`: panel sorting and sort-mode selection.
- `Alt+F1` / `Alt+F2`: change location for the left/right panel.
- `Alt+F5`: print through a plugin where available.
- `Alt+F6`: create file links.
- `Alt+F7`: find files.
- `Alt+F8`: command history.
- `Alt+F9`: resize/maximize behavior configured by Far/console settings.
- `Alt+F10`: find folder.
- `Alt+F11`: view/edit history.
- `Alt+F12`: folder history.
- `Shift+F1`…`Shift+F10`: alternate/contextual operations, including plugin help/configuration, save-as, and last-menu behavior depending on area.

The key bar at the bottom changes when `Ctrl`, `Alt`, or `Shift` is held, making these layers discoverable. `Ctrl+B` hides or shows the bar.

### Dialog conventions

- `Tab` / `Shift+Tab` move between controls.
- `Ctrl+Enter` activates the default action.
- `Esc` cancels; clicking outside on the left behaves similarly.
- `Enter` accepts; right-clicking outside behaves similarly.
- `Ctrl+Up` / `Ctrl+Down` open or traverse edit-control history.
- `Shift+Del` removes an unlocked history item.
- `Gray +`, `Gray -`, and `Gray *` set checkboxes on, off, or indeterminate.
- `Ctrl+F5`, then arrows, moves a dialog with the keyboard.
- `F1` is context-sensitive help throughout Far and plugin dialogs.

### Menu conventions

- `F9` opens the main menu and initially selects the active panel’s side menu.
- `Left` / `Right` move among top-level menus.
- `Tab` switches the side whose panel-specific menu is being controlled.
- `Shift+F10` executes or selects the last-used menu command.
- `RAlt` or `Ctrl+Alt+F` enables menu/list filtering.
- `Ctrl+Alt+L` locks the current filter.
- Horizontal scrolling is available with `Alt+Left` / `Alt+Right`; modifier variants scroll the current item or jump farther.
- Menu accelerators are shown as highlighted letters and can be activated directly.

## Main menu inventory

### Left and Right menus

Each side menu controls the corresponding panel:

- Brief, Medium, Full, Wide, Detailed, Descriptions, Long descriptions, File owners, File links, Alternative full.
- Info panel, Tree panel, Quick view.
- Sort modes.
- Show long names.
- Panel on/off.
- Re-read.
- Change drive.

### Files menu

- View.
- Edit.
- Copy.
- Rename or move.
- Link.
- Make folder.
- Delete.
- Wipe.
- Add to archive.
- Extract files.
- Archive commands.
- File attributes.
- Apply command.
- Describe files.
- Select group.
- Deselect group.
- Invert selection.
- Restore selection.

Archive items are plugin-backed and may vary with installed archive plugins.

### Commands menu

- Find file.
- History.
- Video mode.
- Find folder.
- File view history.
- Folders history.
- Swap panels.
- Panels on/off.
- Compare folders.
- Edit user menu.
- File associations.
- Folder shortcuts.
- Filter panel.
- Plugin commands.
- Screens list.
- Task list.
- Hotplug devices list.

Some builds, plugins, or Windows environments may add context-sensitive entries.

### Options menu

- System settings.
- Panel settings.
- Tree settings.
- Interface settings.
- Dialog settings.
- Menu settings.
- Command line settings.
- AutoComplete settings.
- Confirmations.
- File panel modes.
- File descriptions.
- Folder descriptions.
- Viewer settings.
- Editor settings.
- Code pages.
- Colors / color groups / themes.
- Files highlighting and sort groups.
- Save setup.
- Plugins configuration.
- Plugins manager settings.
- Groups of file masks.

The exact order and wording can change between builds, but these are the documented configuration surfaces in the snapshot.

## Panel hotkey reference

### Panel layout and visibility

| Action | Key |
|---|---|
| Change active panel | `Tab` |
| Swap panels | `Ctrl+U` |
| Refresh active panel | `Ctrl+R` |
| Toggle info / quick view / tree | `Ctrl+L` / `Ctrl+Q` / `Ctrl+T` |
| Hide/show both panels | `Ctrl+O` |
| Temporarily hide panels | hold `Ctrl+Alt+Shift` |
| Hide inactive panel | `Ctrl+P` |
| Hide left / right panel | `Ctrl+F1` / `Ctrl+F2` |
| Resize panels vertically | `Ctrl+Up` / `Ctrl+Down` |
| Resize current panel vertically | `Ctrl+Shift+Up` / `Ctrl+Shift+Down` |
| Resize panels horizontally | `Ctrl+Left` / `Ctrl+Right` on empty command line |
| Restore width / height | `Ctrl+Numpad5` / `Ctrl+Alt+Numpad5` |
| Toggle key bar | `Ctrl+B` |
| Toggle byte/suffix size display | `Ctrl+Shift+S` |

### Panel views and display

| Action | Key |
|---|---|
| Default views 1–9, 0 | `LeftCtrl+1`…`LeftCtrl+9`, `LeftCtrl+0` |
| Hidden/system files | `Ctrl+H` |
| Long/short names | `Ctrl+N` |
| Scroll long names/descriptions | `Alt+Left` / `Alt+Right` |
| Jump scroll to ends | `Alt+Home` / `Alt+End` |

### Sorting

| Criterion | Key |
|---|---|
| Name | `Ctrl+F3` |
| Extension | `Ctrl+F4` |
| Last write time | `Ctrl+F5` |
| Size | `Ctrl+F6` |
| Unsorted | `Ctrl+F7` |
| Creation time | `Ctrl+F8` |
| Access time | `Ctrl+F9` |
| Description | `Ctrl+F10` |
| Owner | `Ctrl+F11` |
| Sort-mode menu | `Ctrl+F12` |
| Toggle sort groups | `Shift+F11` |
| Selected first | `Shift+F12` |

The sort-mode menu exposes additional criteria and options not assigned to default keys.

### Selection

| Action | Key |
|---|---|
| Toggle current item | `Ins` |
| Extend/toggle while moving | `Shift+Up` / `Shift+Down` |
| Select / deselect by mask | `Gray +` / `Gray -` |
| Invert files | `Gray *` |
| Same extension select/deselect | `Ctrl+Gray +` / `Ctrl+Gray -` |
| Same name select/deselect | `Alt+Gray +` / `Alt+Gray -` |
| Select / deselect all files | `Shift+Gray +` / `Shift+Gray -` |
| Restore previous selection | `Ctrl+M` |

### Navigation and path transfer

- `Enter`: open a folder, execute a file, or invoke the associated/plugin action.
- `Ctrl+PgUp`: parent folder; at a root, optionally open the change-drive menu.
- `Ctrl+PgDn`: enter the folder or archive under the cursor.
- `Alt+F1` / `Alt+F2`: change left/right drive or plugin panel.
- `RightCtrl+0`…`9`: jump to a folder shortcut.
- `Ctrl+Shift+0`…`9`: store a folder shortcut.
- `Ctrl+\`: root folder.
- `Ctrl+Shift+\`: root of the current network share where applicable.
- `Ctrl+Ins`: copy selected/current names to the clipboard if the command line is empty.
- `Ctrl+F`: insert the active-panel filename into the command line.
- `Ctrl+;`: insert the passive-panel filename.
- Modifier variants insert full paths, network paths, or multiple selected names.

## Command-line workflow

Far’s command line and panels are deliberately integrated:

1. Navigate to the working folder in a panel.
2. Type a command without leaving the panel UI.
3. Insert current filenames or paths from either panel rather than retyping them.
4. Execute with `Enter`.
5. Recall with `Alt+F8` or command-line history keys.
6. Use panel selection and “Apply command” when a command must run once per file.

Important conventions:

- `Esc` clears the command line, with configurable behavior.
- `Ctrl+E` / `Ctrl+X` move through command history.
- `Alt+F8` opens full command history.
- `Ctrl+End` accepts completion/history text by progressively copying characters.
- `Ctrl+Space` may complete the current token according to AutoComplete settings.
- `Ctrl+Alt+F` or `RAlt` filters completion and history lists.
- File associations can override normal Windows execution for matching masks.
- Prefixes such as those registered by plugins route a command directly to that plugin.

## Common workflows

### Copy or move files between folders

1. Put the source folder in one panel and destination in the other.
2. Select files with `Ins` or a mask with `Gray +`.
3. Press `F5` to copy or `F6` to move/rename.
4. Confirm or edit the destination path and operation options.
5. Resolve overwrite prompts individually or establish an “all” rule.

The destination is prefilled from the passive panel, which is the central two-panel workflow convention.

### Inspect without opening external applications

1. Press `F3` for the internal viewer.
2. Press `F4` for the internal editor.
3. Press `Ctrl+Q` to dedicate one panel to quick view while navigating in the other.
4. Use `F12` to switch among panels and open editor/viewer screens.

### Search a directory tree

1. Press `Alt+F7`.
2. Enter one or more file masks and optional containing text.
3. Select search scope, code page, archive handling, and advanced filters.
4. Start the search.
5. View/edit a result, jump to it, or place results into a panel.

### Revisit previous locations and files

- `Alt+F12`: folder history.
- `Alt+F11`: viewed/edited-file history.
- `Alt+F8`: command history.
- Mark important history entries with `Ins` so retention policies do not push them out.
- Use `Shift+Enter` where documented to activate a history entry without moving it to the end.

### Work with archives

1. Press `Enter` or `Ctrl+PgDn` on a recognized archive.
2. Navigate it as a plugin panel.
3. Use ordinary `F5`/`F6` operations to extract or copy into supported archive formats.
4. Use Files-menu archive commands for format-specific operations.
5. Return with `Ctrl+PgUp` or normal panel navigation.

Archive behavior depends on ArcLite, MultiArc, or another installed plugin and its configured formats.

### Create and use a user menu

1. Press `F2` to open the local or main user menu.
2. Define commands, labels, separators, submenus, and hotkeys in the menu editor.
3. Use Far metasymbols to substitute current file, selected files, and active/passive paths.
4. Keep project-specific commands in a local menu and reusable commands in the main menu.

### Record a keyboard macro

1. Press `Ctrl+.` to record in general mode or `Ctrl+Shift+.` for special mode.
2. Perform the desired Far key sequence.
3. Press `Ctrl+.` to finish with defaults, or `Ctrl+Shift+.` to configure conditions.
4. Assign a hotkey.
5. Invoke the hotkey in the macro’s interaction area.

General mode sends recorded keys to plugins. Special mode suppresses key delivery to plugins that intercept relevant events. Scripted Lua macros can go beyond replay by inspecting state, calling APIs, and handling events.

## Viewer and editor conventions

### Viewer

- `F7` searches; `Shift+F7` repeats in the reverse direction where applicable.
- `F8` cycles or selects code pages according to configuration.
- `Alt+F8` opens go-to-position.
- `F2` toggles wrapping in the standard viewer key map.
- `Shift+F2` toggles wrap mode details such as word wrapping.
- `F4` toggles text/hex-related view mode.
- `Ctrl+O` hides/shows the surrounding interface where supported.
- `F10` or `Esc` closes the viewer.

### Editor

- `F2` saves.
- `Shift+F2` opens Save As.
- `F7` searches; `Ctrl+F7` replaces.
- `Alt+F7` or the documented search controls repeat or invoke alternate search behavior depending on the current build/key map.
- `Alt+F8` goes to line/column.
- `F8` cycles configured code pages.
- `Ctrl+Z` / `Ctrl+Shift+Z` perform undo/redo.
- `Shift+Arrow` makes a stream selection; modifier variants support column blocks.
- `Ctrl+C`, `Ctrl+X`, `Ctrl+V` and `Ctrl+Ins`, `Shift+Del`, `Shift+Ins` support clipboard operations.
- `F10` or `Esc` closes, prompting if modified.

The shipped help remains the authority for the complete editor/viewer key set because plugins and macros can intentionally redefine keys.

## Configuration model

### Normal settings dialogs

The Options menu covers the supported user-facing settings:

- System behavior: delete, copy, links, scan symbolic links, recycle bin, history, and execution rules.
- Panels: status line, hidden files, auto-update, network paths, selection behavior, and layout.
- Tree: caching and navigation behavior.
- Interface: clock, key bar, menu bar, screen behavior, title, drive-change handling, and visual options.
- Dialogs and menus: history, persistent blocks, mouse behavior, wrapping, and filtering.
- Command line and completion.
- Viewer and editor.
- Confirmations.
- Colors, highlighting, sort groups, file masks, descriptions, and custom panel modes.
- Plugin manager and individual plugin configuration.

“Save setup” persists the current configuration when automatic saving is disabled.

### Advanced `far:config`

`far:config` exposes settings intentionally omitted from normal dialogs. The shipped help documents advanced keys for:

- Command execution and history rules.
- Clipboard/path quoting behavior.
- copy buffer, time, security, streams, and substitution rules.
- editor limits, BOM/EOL defaults, character-code display, read-only locking, and undo storage.
- mouse-wheel deltas.
- panel tree and dot-file behavior.
- macro playback indicators.
- transliteration tables and keyboard layouts.
- cursor sizes, number formatting, title formats, and redraw timing.
- policy-controlled hidden drives and other Windows-specific integration.

Advanced settings should be changed only after consulting their individual `far:config` help topics.

### Profiles and portability

Far separates executable files from profile data. Command-line switches and environment variables can redirect the roaming and local profiles, enabling portable or isolated configurations. Important concepts include:

- Main profile: persistent configuration, histories, macros, and user data.
- Local profile: machine-local state and caches.
- Plugin directories: executable extensions discovered at startup or explicitly loaded.
- Macro directories: LuaMacro scripts and modules under the profile.

Exact paths depend on installation and launch switches; consult `far.exe /?` or the shipped “Command line switches” help topic for the build in use.

## File masks, filters, and regular expressions

Far uses masks across selection, associations, highlighting, filters, search, and plugin configuration.

- Multiple include masks can be separated using commas or semicolons.
- Exclusions use the documented include/exclude syntax.
- Mask groups allow reusable named sets.
- Filters can combine masks with attributes, size, and date constraints.
- Regular expressions are supported in search and editor replacement, with Far’s documented syntax and replacement references.
- Named masks and regular expressions are more maintainable than repeating large ad hoc expressions across settings.

## Mouse support

Although keyboard use is primary, mouse behavior is integrated:

- Click to activate panels, choose items, and operate menus/dialogs.
- Right-click toggles file selection under the default panel rule.
- Drag between panels to copy; hold `Shift` or toggle during drag to move.
- Middle-click acts like `Enter` with the same keyboard modifiers.
- Wheel scrolls panels, viewer, editor, help, menus, and dropdown lists with area-specific behavior.
- `Alt+Ins` opens the screen grabber for stream or rectangular text capture from the console.

## Help and discoverability

- `F1`: context-sensitive help.
- `Shift+F1`: help contents.
- `Shift+F2`: plugin help list.
- `F7`: search the current help file.
- `Alt+F1` or `Backspace`: return to the previous help topic.
- `F5`: zoom/unzoom the help window.

Far’s help is not merely introductory documentation: it is the version-matched reference for key bindings, settings, dialogs, plugin conventions, and advanced configuration.

## Important caveats

- Far Manager 3 is Windows software. Far2l is a related port derived from older Far code and must not be treated as identical to Far Manager 3.
- Plugins can add, hide, or replace commands and can provide virtual panels with different semantics.
- Macros can redefine standard hotkeys, so an individual installation may differ substantially from defaults.
- Windows Terminal, console hosts, remote sessions, keyboard layouts, and global OS hotkeys can intercept combinations before Far receives them.
- Archive, FTP, network, syntax-highlighting, and other plugin-backed features depend on the plugins packaged and enabled in a particular distribution.
- The official English help is the best evidence for core behavior; third-party shortcut lists often omit context, use older Far versions, or mix Far with Far2l.

## Research completeness map

Covered in this baseline:

- Product mental model and interaction conventions.
- Core supported feature families.
- Main-menu structure.
- Primary panel, navigation, selection, sorting, function-key, dialog, and menu hotkeys.
- Common end-to-end workflows.
- Viewer/editor/search/archive/history/user-menu/macro behavior.
- Configuration and extensibility model.

Expanded in dedicated appendices:

- Command-line switches, profile rules, and Far environment variables.
- Default viewer and editor hotkeys.
- Search/history, drive-menu, menu/list, dialog, help, and screen-grabber conventions.
- Bundled plugin-by-plugin feature inventory for build 6703.
- Core settings dialogs and their supported options.
- LuaMacro loading, registration, conditions, events, extensions, API namespaces, and design conventions.
- Tree/info/quick-view panels, custom modes, filters, highlighting, descriptions, user menus, associations, and metasymbol workflows.
- Copy/move/delete/link/attribute semantics, search scopes/results, regex conventions, histories, tasks, devices, and elevation.

Optional developer-level follow-ups, outside this user/operator research scope:

- Plugin-specific content-column field catalogs beyond the core column grammar.
- Function-by-function LuaMacro and LuaFAR API signatures beyond the workflow-level reference.
- Version-delta notes between stable and current development builds.

The authoritative topic checklist for those expansions is in `docs/farmanager-3-topic-inventory.md`.
The requirement-by-requirement completion audit is in `docs/farmanager-3-coverage-audit.md`.
