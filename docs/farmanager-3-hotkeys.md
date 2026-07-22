# Far Manager 3 Default Hotkey Reference

This reference is normalized from `FarEng.hlf` shipped in official x64 build `3.0.6703.0` dated 2026-06-23. It describes defaults: plugins, macros, console hosts, and Windows-level shortcuts can change or intercept keys.

## Function-key layers in panels

| Key | Plain | `Shift` | `Alt` |
|---|---|---|---|
| `F1` | Context help | Add to archive | Change left drive/panel |
| `F2` | User menu | Extract from archive | Change right drive/panel |
| `F3` | View; calculate folder size | Archive commands | Alternate internal/external viewer |
| `F4` | Edit; folder attributes | Edit new file | Alternate internal/external editor |
| `F5` | Copy | Copy current item only | Print |
| `F6` | Move/rename | Move/rename current item only | Create hard/symbolic link or junction |
| `F7` | Make folder | — | Find file |
| `F8` | Delete | Delete current item only | Command history |
| `F9` | Main menu | Save configuration | Maximize/restore console |
| `F10` | Exit | Repeat/select last menu command | Find folder |
| `F11` | Plugin commands | Toggle sort groups | View/edit history |
| `F12` | Screen list | Selected files first | Folder history |

Additional combinations:

- `Ctrl+Shift+F3`: force the internal viewer, ignoring associations.
- `Ctrl+Shift+F4`: force the internal editor, ignoring associations.
- `Alt+Shift+F9`: configure plugins from panels; viewer/editor settings in those areas.
- `Ctrl+Alt+Enter`: execute the current item or command as administrator.
- `Shift+Enter`: execute separately; on a folder or `..`, open it in Windows Explorer.

## Panel navigation and layout

| Action | Key |
|---|---|
| Change active panel | `Tab` |
| Swap left and right panels | `Ctrl+U` |
| Refresh active panel | `Ctrl+R` |
| Toggle information panel | `Ctrl+L` |
| Toggle quick-view panel | `Ctrl+Q` |
| Toggle tree panel | `Ctrl+T` |
| Hide/show both panels | `Ctrl+O` |
| Temporarily show user screen | hold `Ctrl+Alt+Shift` |
| Hide/show inactive panel | `Ctrl+P` |
| Hide/show left panel | `Ctrl+F1` |
| Hide/show right panel | `Ctrl+F2` |
| Change both panels’ height | `Ctrl+Up`, `Ctrl+Down` |
| Change current panel’s height | `Ctrl+Shift+Up`, `Ctrl+Shift+Down` |
| Change panel widths | `Ctrl+Left`, `Ctrl+Right` when command line is empty |
| Restore default widths | `Ctrl+Numpad5` |
| Restore default heights | `Ctrl+Alt+Numpad5` |
| Toggle function-key bar | `Ctrl+B` |
| Toggle sizes as bytes versus suffixes | `Ctrl+Shift+S` |

Navigation:

- `Enter`: enter folder/archive, execute file, or invoke an association.
- `Ctrl+PgDn`: enter the folder/archive; `Ctrl+Shift+PgDn` forces archive opening instead of an association.
- `Ctrl+PgUp`: parent folder; at a root it can open the drive menu if enabled.
- `Ctrl+\`: filesystem root.
- `Alt+F1`, `Alt+F2`: change the left or right location through drives and plugin panels.
- `Ctrl+Shift+0`…`9`: save the current folder shortcut.
- `RightCtrl+0`…`9`: activate a folder shortcut.

## Panel views

| Key | Default view |
|---|---|
| `LeftCtrl+1` | Brief |
| `LeftCtrl+2` | Medium |
| `LeftCtrl+3` | Full |
| `LeftCtrl+4` | Wide |
| `LeftCtrl+5` | Detailed |
| `LeftCtrl+6` | Descriptions |
| `LeftCtrl+7` | Long descriptions |
| `LeftCtrl+8` | File owners |
| `LeftCtrl+9` | File links |
| `LeftCtrl+0` | Alternative full |

- `Ctrl+H`: toggle hidden and system files.
- `Ctrl+N`: toggle long and short names.
- `Alt+Left`, `Alt+Right`: horizontally scroll long names and descriptions.
- `Alt+Home`, `Alt+End`: move horizontal display to its ends.

## Selection

| Action | Key |
|---|---|
| Toggle current item | `Ins`, right-click |
| Select while moving | `Shift+Up`, `Shift+Down` |
| Select by mask | `Gray +` |
| Deselect by mask | `Gray -` |
| Invert files | `Gray *` |
| Select/deselect same extension | `Ctrl+Gray +`, `Ctrl+Gray -` |
| Invert including folders | `Ctrl+Gray *` |
| Select/deselect same name | `Alt+Gray +`, `Alt+Gray -` |
| Invert files and clear folders | `Alt+Gray *` |
| Select/deselect all files | `Shift+Gray +`, `Shift+Gray -` |
| Restore previous selection | `Ctrl+M` |

The current item is implicitly processed when no explicit selection exists. `Shift+F5`, `Shift+F6`, and `Shift+F8` deliberately process only the item under the cursor even if other files are selected.

## Sorting

| Key | Sort mode |
|---|---|
| `Ctrl+F3` | Name |
| `Ctrl+F4` | Extension |
| `Ctrl+F5` | Last-write time |
| `Ctrl+F6` | Size |
| `Ctrl+F7` | Unsorted |
| `Ctrl+F8` | Creation time |
| `Ctrl+F9` | Access time |
| `Ctrl+F10` | Description |
| `Ctrl+F11` | Owner |
| `Ctrl+F12` | Sort-mode menu |
| `Shift+F11` | Toggle sort groups |
| `Shift+F12` | Toggle selected-first |

The sort menu additionally covers allocation size, hard-link count, stream count/size, full name, change time, and plugin/custom criteria when available. Options include reverse order, numeric sorting, case sensitivity, sort groups, selected first, and directories first.

## Command-line editing

| Action | Key |
|---|---|
| Character left/right | `Left` / `Right`, `Ctrl+S` / `Ctrl+D` |
| Word left/right | `Ctrl+Left`, `Ctrl+Right` |
| Start/end | `Ctrl+Home`, `Ctrl+End` |
| Delete character/left | `Del`, `Backspace` |
| Delete to end | `Ctrl+K` |
| Delete word left/right | `Ctrl+Backspace`, `Ctrl+Del` |
| Copy/paste | `Ctrl+Ins`, `Shift+Ins` |
| Previous/next command | `Ctrl+E`, `Ctrl+X` |
| Clear command line | `Ctrl+Y` |
| Full command history | `Alt+F8` |

At the end of the line, repeated `Ctrl+End` cycles history entries that start with the text already typed.

### Insert panel data into the command line

| Insert | Active/left | Passive/right |
|---|---|---|
| Current filename | `Ctrl+J` or `Ctrl+Enter` | `Ctrl+Shift+Enter` |
| Full filename | `Ctrl+F` | `Ctrl+;` |
| UNC filename | `Ctrl+Alt+F` | `Ctrl+Alt+;` |
| Left/right path | `Ctrl+[` | `Ctrl+]` |
| Left/right UNC path | `Ctrl+Alt+[` | `Ctrl+Alt+]` |
| Active/passive path | `Ctrl+Shift+[` | `Ctrl+Shift+]` |
| Active/passive UNC path | `Alt+Shift+[` | `Alt+Shift+]` |

When the command line is empty, `Ctrl+Ins` copies selected filenames rather than copying command-line text.

## Common panel service keys

| Action | Key |
|---|---|
| Attributes/timestamps | `Ctrl+A` |
| Apply command to selection/current item | `Ctrl+G` |
| Describe selected files | `Ctrl+Z` |
| Wipe | `Alt+Del` |
| Delete without Recycle Bin | `Shift+Del` |
| Copy selected names to clipboard on empty command line | `Ctrl+Ins` |
| Open screen grabber | `Alt+Ins` |

## Fast find

Hold `Alt` and type characters to start incremental panel lookup. The exact activation can be configured and keyboard-layout handling depends on transliteration settings.

- `Ctrl+Enter`: cycle forward through matching items.
- `Ctrl+Shift+Enter`: cycle backward.
- `Enter`: accept the current match.
- `Esc`: cancel.

## Viewer

### Navigation

| Action | Key |
|---|---|
| Line up/down | `Up`, `Down` |
| Page up/down | `PgUp`, `PgDn` |
| Start/end of file | `Home` / `End`, `Ctrl+Home` / `Ctrl+End` |
| Column left/right in unwrapped text | `Left`, `Right` |
| Move 20 columns | `Ctrl+Left`, `Ctrl+Right` |
| Leftmost/rightmost visible range | `Ctrl+Shift+Left`, `Ctrl+Shift+Right` |
| Change bytes per hex row | `Alt+Left`, `Alt+Right` |
| Change hex row width to a multiple of 16 | `Ctrl+Alt+Left`, `Ctrl+Alt+Right` |

### Commands

| Action | Key |
|---|---|
| Help | `F1` |
| Toggle wrapping or change mode contextually | `F2` |
| Toggle character/word wrapping | `Shift+F2` |
| Toggle hex and previous mode | `F4` |
| Choose text/hex/dump mode | `Shift+F4` |
| Switch to editor | `F6` |
| Search | `F7` |
| Continue forward | `Shift+F7`, `Space` |
| Continue backward | `Alt+F7` |
| Toggle OEM/ANSI code pages | `F8` |
| Choose code page | `Shift+F8` |
| Go to position | `Alt+F8` |
| Viewer settings | `Alt+Shift+F9` |
| Close | `Numpad5`, `F3`, `F10`, `Esc` |
| Locate file in active panel | `Ctrl+F10` |
| Plugin commands | `F11` |
| View/edit history | `Alt+F11` |
| Next/previous panel file | `Gray +`, `Gray -` |
| Toggle user screen | `Ctrl+O` |
| Toggle key bar/status line/scrollbar | `Ctrl+B`, `Ctrl+Shift+B`, `Ctrl+S` |
| Undo position change | `Alt+Backspace`, `Ctrl+Z` |
| Copy selection | `Ctrl+Ins`, `Ctrl+C` |
| Clear selection | `Ctrl+U` |

Bookmarks use `RightCtrl+0`…`9` or `Ctrl+Shift+0`…`9` to set and `LeftCtrl+0`…`9` to jump.

## Editor

### Movement and deletion

| Action | Key |
|---|---|
| Character left/right | `Left`, `Right` |
| Word left/right | `Ctrl+Left`, `Ctrl+Right` |
| Scroll without moving logical line | `Ctrl+Up`, `Ctrl+Down` |
| Start/end of line | `Home`, `End` |
| Start/end of file | `Ctrl+Home` / `Ctrl+PgUp`, `Ctrl+End` / `Ctrl+PgDn` |
| Start/end of screen | `Ctrl+N`, `Ctrl+E` |
| Delete character/left | `Del`, `Backspace` |
| Delete line | `Ctrl+Y` |
| Delete to line end | `Ctrl+K` |
| Delete previous/next word | `Ctrl+Backspace`, `Ctrl+T` or `Ctrl+Del` |

### Blocks and clipboard

| Action | Key |
|---|---|
| Stream selection | `Shift+Cursor`, `Ctrl+Shift+Cursor` |
| Column selection | `Alt+Shift+Cursor`, `Alt+gray cursor`, `Ctrl+Alt+gray cursor` |
| Select all / clear selection | `Ctrl+A`, `Ctrl+U` |
| Paste | `Shift+Ins`, `Ctrl+V` |
| Cut | `Shift+Del`, `Ctrl+X` |
| Copy | `Ctrl+Ins`, `Ctrl+C` |
| Append to clipboard | `Ctrl+Gray +` |
| Delete block | `Ctrl+D` |
| Copy/move persistent block to cursor | `Ctrl+P`, `Ctrl+M` |
| Shift block left/right | `Alt+U`, `Alt+I` |

With no selection, `Ctrl+Ins` or `Ctrl+C` selects and copies the current line. `Alt+U` / `Alt+I` indent the current line when no block is selected.

### Editor commands

| Action | Key |
|---|---|
| Save / Save As | `F2`, `Shift+F2` |
| Edit a new file | `Shift+F4` |
| Switch to viewer | `F6` |
| Search / replace | `F7`, `Ctrl+F7` |
| Continue forward/backward | `Shift+F7`, `Alt+F7` |
| Toggle/select code page | `F8`, `Shift+F8` |
| Go to line and column | `Alt+F8` |
| Editor settings | `Alt+Shift+F9` |
| Close | `F10`, `F4`, `Esc` |
| Save and close | `Shift+F10` |
| Locate current file in panel | `Ctrl+F10` |
| Plugin commands | `F11` |
| View/edit history | `Alt+F11` |
| Undo / redo | `Alt+Backspace` or `Ctrl+Z`; `Ctrl+Shift+Z` |
| Lock text modification | `Ctrl+L` |
| Show user screen | `Ctrl+O` |
| Enter next key as character code | `Ctrl+Q` |
| Insert active/passive panel filename | `Shift+Enter`, `Ctrl+Shift+Enter` |
| Insert full edited filename | `Ctrl+F` |
| Toggle key bar/status line | `Ctrl+B`, `Ctrl+Shift+B` |

Bookmarks use the same set/jump convention as the viewer.

## Dialogs

- `Tab`, `Shift+Tab`: next/previous control.
- `Ctrl+Enter`: activate the default action.
- `Home`: first control when the focused control does not consume it.
- `End` or `PgDn`: default control when not consumed.
- `Ctrl+Up`, `Ctrl+Down`: edit-control history.
- `Del`: clear history; `Shift+Del`: delete the current unlocked history entry.
- `Gray +`, `Gray -`, `Gray *`: checked, unchecked, indeterminate.
- `Ctrl+F5`, then arrows: move the dialog.
- `Shift+Enter`: insert active-panel filename.
- `Ctrl+Shift+Enter`: insert passive-panel filename.
- Left-click outside: cancel; right-click outside: accept.

## Menus and lists

- `RAlt` or `Ctrl+Alt+F`: enable/disable filtering.
- `Ctrl+Alt+L`: lock/unlock the filter.
- `Alt+Left`, `Alt+Right`: horizontally scroll all entries.
- `Alt+Shift+Left`, `Alt+Shift+Right`: scroll only the selected entry.
- `Ctrl+Alt+Left`, `Ctrl+Alt+Right`: scroll all entries by 20 characters.
- `Ctrl+Shift+Left`, `Ctrl+Shift+Right`: scroll selected entry by 20 characters.
- `Alt+Home`, `Alt+End`: align all entries left/right.
- `Alt+Shift+Home`, `Alt+Shift+End`: align selected entry left/right.
- `Shift+F5`: toggle fixed columns in menus that provide them, such as editor Find All.

## Search results and histories

Common list behavior includes:

- `Enter`: activate the selected result or history entry.
- `Shift+Enter`: activate without moving a history item to the newest position where supported.
- `Ins`: lock/unlock important history items or mark items in supported result lists.
- `Del` / `Shift+Del`: clear or remove unlocked entries according to the current list.
- `Ctrl+R`: refresh in lists backed by changing system state.
- Filtering keys follow the common menu conventions.

Find-file results additionally support viewing/editing, jumping to the containing folder, and sending the result set to a panel. Available commands are shown in the context-sensitive key bar.

## Change-drive menu

- `Alt+F1`, `Alt+F2`: open for the left/right panel.
- `Ctrl+1`…`Ctrl+9`: toggle disk type, target/network path, label, filesystem, size/free space, removable parameters, plugins, optical-disc parameters, and network parameters.
- `F9`: change drive-menu display settings.
- `Shift+Enter`: open the selected disk root in Explorer.
- `Ctrl+H`: show unmapped volumes.
- `Shift+F6`: change volume label.
- `Ctrl+R`: refresh.
- `Shift+F1`: plugin context help.
- `Shift+F9`: configure the selected plugin.
- `Alt+Shift+F9`: plugin configuration list.

## Screen switching

- `F12`: open the screen list.
- Choose among the panels screen and open viewer/editor screens.
- The list supports normal menu filtering and selection commands.
- `Ctrl+Tab` / `Ctrl+Shift+Tab` may cycle screens depending on area and configuration.

## Screen grabber

Open with `Alt+Ins`.

- `Space`: switch stream/block selection.
- Arrows or click: move cursor.
- `Shift+Arrow` or mouse drag: select.
- `Alt+Shift+Arrow`: grow/shrink selection.
- `Alt+Arrow`: move selection.
- `Enter`, `Ctrl+Ins`, right-click, or double-click: copy.
- `Ctrl+Gray +`: append to clipboard.
- `Ctrl+A`: select the whole screen.
- `Ctrl+U`: clear selection.
- `Ctrl+Shift+Left/Right`: resize by ten columns.
- `Ctrl+Shift+Up/Down`: resize by five rows.
- `Esc`: close.

## Macro recording

- `Ctrl+.`: begin or finish recording in general mode; recorded keys are delivered to plugins.
- `Ctrl+Shift+.`: begin special-mode recording, or finish and open macro options.
- After recording, assign a hotkey and optionally constrain playback by area, command-line state, panel type, current item type, and selection state.
- Macros can replace defaults, so troubleshooting a changed key should include checking LuaMacro scripts and the installed-macro list.

## Help

- `F1`: context help.
- `Shift+F1`: contents.
- `Shift+F2`: plugin help.
- `F7`: search the current help file.
- `Alt+F1` or `Backspace`: previous topic.
- `Tab`, `Shift+Tab`: next/previous link.
- `Enter`: follow link.
- `F5`: zoom/unzoom help.

