# Far Manager 3 LuaMacro Automation

LuaMacro is Far Manager 3’s principal automation subsystem. This reference is based on the official macro-system manual stored in source commit `28443e51d9f7b4b9e5976d1db8246532ccc40205` and the LuaMacro plugin shipped with build `3.0.6703`.

## Automation layers

LuaMacro provides several related mechanisms:

1. **Recorded keyboard macros**: quick key-sequence capture created inside Far.
2. **Regular macros**: Lua or MoonScript definitions with conditions, priorities, state inspection, and arbitrary actions.
3. **Event handlers**: code run for editor, viewer, dialog, console, shutdown, and other plugin events.
4. **Plugin-menu items**: scripted entries added to Far’s plugin command menus.
5. **Command-line prefixes**: scripted commands invoked as `prefix:arguments`.
6. **Panel modules**: virtual panels implemented in script.
7. **Content columns and custom sorting**: scripted metadata and ordering for file panels.
8. **External modules**: Lua libraries loaded by scripts.

Recorded macros are convenient for experiments and short sequences. The official manual recommends regular macros for maintainable long-term automation.

## Macro storage and loading

### Recorded keyboard macros

- Stored as generated Lua files under `%FARPROFILE%\Macros\internal`.
- Far creates, edits, and deletes these files; manual editing is discouraged.
- Saved explicitly through macro control or automatically when “Auto save setup” is enabled.
- Each has exactly one area and one key.
- Only generic `Ctrl`, `Alt`, and `Shift` modifiers are supported; left/right-specific modifiers and regular-expression keys are not.
- A recorded keyboard macro has higher priority than regular macros for the same key and area.

### Regular macrofiles

Regular macros and event handlers are read recursively from Lua `*.lua` and MoonScript `*.moon` files. Search roots are chosen in this order:

1. Paths explicitly supplied to a load command or function.
2. `MacroPath` in `luamacro.ini`.
3. `%FARPROFILE%\Macros\scripts`.

Far does not modify script trees. One file can define any number of macros and events.

For each root, `_macroinit.lua` runs first when present. Other files in that root have undefined ordering. Roots themselves load in the listed sequence.

Each macrofile receives:

- Its full pathname.
- Its execution counter for the current LuaMacro session.

This permits file-relative imports, initialization, and reload-aware state.

## Regular macro registration

A regular macro is registered with the global `Macro` function:

```lua
Macro {
  area = "Shell Info Tree",
  key = "CtrlF11 ShiftHome",
  description = "Example",
  flags = "NoPluginPanels EmptyCommandLine",
  filemask = "*.txt,*.cpp",
  priority = 50,
  sortpriority = 50,
  selected = true,
  condition = function(key, data)
    return Far.Height > 30
  end,
  action = function(data)
    far.Message("Executed", "Example")
  end,
  id = "F0109446-AA63-4873-AEC3-17AEE993AA53",
}
```

### Registration fields

| Field | Purpose |
|---|---|
| `area` | One or more whitespace-separated execution areas. |
| `key` | One or more keys, or a regular expression surrounded by `/`. Optional for startup macros. |
| `description` | Human-readable text shown in macro lists and conflict selection. |
| `flags` | Whitespace-separated preconditions and playback behavior. |
| `filemask` | Editor/viewer filename restriction using Far mask syntax. |
| `priority` | Conflict priority from 0 to 100; default 50. |
| `sortpriority` | Position in the macro-selection menu; default 50. |
| `selected` | Initially select this macro in a conflict menu. |
| `condition` | Dynamic eligibility and optional dynamic priority. |
| `action` | Function executed after all checks pass. |
| `id` | Stable identifier, normally a GUID string. |

### Key conventions

- Key names concatenate modifiers and key, such as `CtrlShiftF3`.
- `Ctrl` matches either `LCtrl` or `RCtrl`; `Alt` matches either side.
- Left/right-specific modifiers are available in regular macros.
- A regular-expression key must preserve canonical modifier ordering: Ctrl, Alt, Shift.
- Multiple keys in one registration are whitespace-separated.

### Conflict resolution

More than one regular macro may share an area/key:

1. Conditions and static constraints remove ineligible macros.
2. Highest priority wins.
3. If multiple macros retain equal highest priority, Far shows a selection menu.
4. `sortpriority` and `selected` influence that menu.

Startup macros all run independently; priority does not suppress them and their relative execution order is undefined.

## Conditions and flags

The recorded-macro settings dialog exposes the same central condition model used by scripted macros.

### Panel type and item state

Conditions can target active and passive panels independently:

- File panel versus plugin panel.
- File versus folder under the cursor.
- Selection present versus absent.

### Other state

- Empty versus non-empty command line.
- Selection block present/absent in editor, viewer, command line, or dialog input.
- Screen output enabled/disabled during playback.
- Run immediately after Far starts.
- Editor/viewer file mask.

Common scripted flag names encode positive and negative forms, including empty/non-empty command line, file/plugin panels, files/folders, active/passive selection, editor/viewer selection, and suppression of key delivery to plugins.

All applicable conditions must pass before execution.

### Dynamic `condition`

`condition(key, data)` receives the triggering key and a stable copy of the registration table.

- `false`, `nil`, or no result: skip the macro.
- A number: execute using that number as dynamic priority.
- Any other truthy result: execute using static `priority`.

Auto-start macros receive `nil` as the key.

## Macro areas

| Area | Context |
|---|---|
| `Shell` | Normal file or plugin panels. |
| `Info` | Information panel. |
| `QView` | Quick-view panel. |
| `Tree` | Tree panel. |
| `Search` | Fast find in panels. |
| `FindFolder` | Find-folder interface. |
| `Viewer` | Internal viewer. |
| `Editor` | Internal editor. |
| `Dialog` | Dialogs. |
| `Menu` | General menus and lists. |
| `MainMenu` | `F9` top menu. |
| `UserMenu` | `F2` user menu. |
| `Disks` | Change-drive menu. |
| `Shell.AutoCompletion` | Panel command-line completion list. |
| `Dialog.AutoCompletion` | Dialog completion list. |
| `Help` | Help viewer. |
| `Other` | Screen grabber and miscellaneous areas. |
| `Common` | Fallback macros considered in every area. |

Recorded macro area is determined where recording starts. Regular macros can list multiple areas.

## Event handlers

Register an event with the global `Event` function:

```lua
Event {
  group = "EditorEvent",
  description = "Track editor events",
  filemask = "*.txt,*.cpp",
  priority = 50,
  condition = function(...) return true end,
  action = function(...) end,
  id = "F0109446-AA63-4873-AEC3-17AEE993AA53",
}
```

Supported groups documented by the manual:

- `DialogEvent`
- `EditorEvent`
- `EditorInput`
- `ExitFAR`
- `ViewerEvent`
- `ConsoleInput`

The condition and action receive the same arguments as the corresponding LuaFAR exported event function. Handlers run from highest priority to lowest; dynamic condition results can alter priority.

`ExitFAR` also runs when LuaMacro is unloaded or macros are reloaded. It receives a Boolean distinguishing a reload from normal exit/unload.

## Adding plugin-menu items

Scripts can register items visible in context-sensitive plugin menus. A menu item can define:

- Areas where it appears.
- Display text and description.
- GUID identity.
- Conditions controlling visibility or enablement.
- An action function.

This is the preferred way to expose discoverable script commands through `F11` without consuming a global hotkey.

## Command-line prefixes

Scripts can register a prefix so a user can type:

```text
prefix:arguments
```

The handler receives the text after the colon and can perform arbitrary scripted work. Prefixes are useful for parameterized commands, script launchers, queries, and bridges to external tools.

Prefix names should avoid one-letter drive-like names and collisions with installed plugins.

## Scripted panel modules

LuaMacro can create virtual panels whose items and behavior are provided by Lua. A panel module can implement:

- Open/close lifecycle.
- Current directory and navigation.
- Item enumeration.
- Get/put/delete/mkdir operations where meaningful.
- Titles, formats, flags, key bars, and panel modes.
- Input and event processing.
- Custom data attached to panel items.

This supports database browsers, search/result panels, API resources, task lists, and other non-filesystem collections while preserving normal Far panel conventions.

## Content columns and custom sorting

Scripts can add computed content columns to file panels. A provider can calculate metadata for an item and expose named fields for custom view modes, filtering, or sorting.

The `Panel` library also supports:

- `CustomSortMenu`
- `LoadCustomSortMode`
- `SetCustomSortMode`

Custom sorting can inspect item metadata and provide ordering unavailable in core Far.

## Macro API namespaces

### Global operations

- `Keys(...)`: send Far key names as input.
- `exit(...)`: terminate the current macro with optional return values.

### `mf`

General macro utilities include:

- Asynchronous calls and posted macros: `acall`, `postmacro`.
- Exit handlers: `AddExitHandler`.
- Load/save/delete/evaluate macro data: `mload`, `msave`, `mdelete`, `eval`.
- Inspect and enumerate scripts: `GetMacroCopy`, `EnumScripts`.
- Open Far/user menus: `mainmenu`, `usermenu`.
- Serialization: `serialize`, `deserialize`.
- Console output: `printconsole`.

### Context/state tables

- `Area`: Boolean indicators for the current macro area.
- `APanel`, `PPanel`: active/passive panel state.
- `Panel`: panel operations and custom sorting.
- `BM`: editor/viewer bookmark stack and navigation.
- `CmdLine`: command-line state and editing.
- `Dlg`: dialog state and controls.
- `Drv`: drive menu state.
- `Editor`: editor state and operations.
- `Far`: dimensions, configuration access, and Far-wide state.
- `Help`: help state.
- `Menu`: current menu/list state and selection.
- `Mouse`: coordinates, button, modifiers, and event flags.
- `Object`: current UI object state.
- `Plugin`: plugin invocation.
- `Viewer`: viewer state and operations.

These high-level macro namespaces coexist with the lower-level LuaFAR plugin API.

## Plugin calls

### `Plugin.Call`

Calls a plugin by GUID and transfers Lua values through Far’s macro-value representation. It may be asynchronous when the plugin opens interactive UI; in that case the macro continues after receiving `true`.

### `Plugin.SyncCall`

Uses the same value model but always waits for the plugin call to return. Use it when subsequent macro logic depends on returned data or completion.

Values supported across the boundary include nil, Boolean, number/integer, UTF strings, binary strings, pointers, and arrays, subject to the called plugin’s contract.

## Command-line operations

LuaMacro exposes management commands through its plugin prefix, including loading/reloading macrofiles and executing Lua expressions or scripts. The exact commands are documented in the installed macro manual and may evolve; they are intended for development, debugging, and explicit reload workflows.

A typical development cycle is:

1. Create a script under `%FARPROFILE%\Macros\scripts`.
2. Register macros/events/menu items.
3. Reload macros with the LuaMacro command prefix.
4. Inspect errors and the macro browser.
5. Test in the intended areas and states.
6. Assign stable IDs and descriptions before long-term use.

## Design conventions

- Prefer a regular macro over a long recorded sequence.
- Use conditions instead of embedding fragile “escape back to a known UI” key sequences.
- Prefer plugin-menu items for discoverability and hotkeys for frequent actions.
- Give macros stable GUIDs and descriptive names.
- Avoid relying on macrofile order except `_macroinit.lua` and ordered roots.
- Use `postmacro` when an action must occur after the current UI event completes.
- Use synchronous plugin calls only when completion or return values are required.
- Keep interactive code out of event handlers that run frequently.
- Restrict editor/viewer macros with file masks when they are language- or format-specific.
- Test with both file and plugin panels and with active/passive selection states.

## Troubleshooting

When a key behaves unexpectedly:

1. Check whether a recorded keyboard macro overrides it.
2. Check regular macros in the current area and `Common`.
3. Consider equal-priority conflicts and conditions.
4. Verify the macro’s panel/file/selection/command-line flags.
5. Reload scripts and inspect syntax/runtime errors.
6. Start Far with `-m` to prove whether macros are responsible.
7. If keys still differ, test without plugins using empty `-p` and check terminal/Windows interception.

## Official manual coverage

The official manual also documents:

- Lua versus the legacy macro language.
- Introspection and script enumeration.
- Variable persistence.
- Function restrictions in asynchronous contexts.
- Editor change subscriptions.
- Lua module unloading behavior.
- Lua, LPeg, and MoonScript runtime details.
- Worked examples for selecting a word, invoking drive menus, comparing file dates, date-named folders, and running scripts from the editor.

