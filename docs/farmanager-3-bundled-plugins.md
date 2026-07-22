# Far Manager 3 Build 6703 Bundled Plugins

This inventory is based on the portable x64 archive `Far30b6703.x64.20260623.7z` (SHA-256 `a648c004ef6e570bd9267e7646d288cd8cbd02e753333dcdadeba39ef30df40b`). The inspected archive is not redistributed in this repository. The inventory distinguishes the plugins actually packaged in that archive from the much larger third-party Far ecosystem.

## Packaged plugin set

| Plugin | Main supported workflow |
|---|---|
| Align | Align selected text in the internal editor. |
| ArcLite | Browse, extract, create, and update archives using 7-Zip technology; supports archive-panel and command-prefix workflows. |
| AutoWrap | Automatically or manually wrap text in the editor. |
| Brackets | Find matching brackets/quotes and select text between them, including common, adjacent, and double-character delimiters. |
| Compare | Advanced two-panel comparison with recursion, depth limits, selection-only mode, time tolerances, size/content comparison, and whitespace/EOL options. |
| DrawLine | Draw single/double pseudographic lines in the editor and handle intersections. |
| EMenu | Invoke Windows Explorer shell context menus for current or selected panel items using text or GUI presentation. |
| EditCase | Convert the case of selected editor text or the current word. |
| FarCmds | Additional Far service commands exposed through plugin/menu interfaces. |
| FarColorer | Syntax highlighting and related editor language services based on Colorer. |
| FileCase | Batch-convert filename and extension case, optionally recursively and with mixed-case safeguards. |
| HlfViewer | Preview Far HLF help files from the editor or command line. |
| LuaMacro | Recorded and scripted Lua macros, event handlers, menu extensions, command prefixes, and automation APIs. |
| NetBox | Remote connection and file-transfer panel, commonly used for SSH/SFTP/SCP and other supported protocols. |
| Network | Browse Windows network resources, map/unmap drives, use alternate credentials, and access servers through the `net:` prefix. |
| ProcList | Process-list virtual panel with process details, termination, local priority changes, and remote-machine inspection. |
| SameFolder | Set the passive panel to the active panel’s folder, or set a chosen panel to the opposite panel’s folder from the drive menu. |
| TmpPanel | Build a temporary virtual panel containing files from unrelated folders and process them as one group; accepts file lists and command-line input. |

## ArcLite

ArcLite supplies the archive-as-panel workflow:

1. Open a recognized archive with `Enter` or `Ctrl+PgDn`.
2. Navigate entries as if they were files and folders.
3. Extract with ordinary copy operations or the Files menu.
4. Add/update content by copying into supported archive panels or using archive commands.
5. Configure compression, update, extraction, and self-extracting archive options.

The exact readable/writable formats depend on the bundled `7z.dll` and ArcLite configuration.

## Compare

Compared with Far’s built-in “Compare folders,” the plugin can:

- Recurse into subfolders with a maximum depth.
- Compare only selected items.
- Compare timestamps with full or two-second precision.
- Ignore plausible timezone offsets in 15-minute multiples.
- Compare size and byte content.
- Ignore line-ending differences or all whitespace during content comparison.
- Mark differences on both panels and optionally announce that no differences exist.

Content and recursive comparisons require filesystem panels rather than arbitrary plugin panels.

## EMenu

EMenu bridges Far selections to Windows Shell context-menu handlers.

- Works from file, tree, temporary, and network-browser panels.
- Can show a console text menu or native GUI menu.
- Can act on multiple selected files/folders.
- Treats the `..` entry as the current folder.
- Provides prefixes such as `rclk:`, `rclk_txt:`, `rclk_gui:`, `rclk_cmd:`, and `rclk_item:`.

This makes Explorer-only actions available without leaving Far, though shell extensions can have their own UI, elevation, or stability behavior.

## Network

The Network plugin supports Windows network browsing and mapped-drive management:

- `F5`: map the selected resource to the next available drive letter.
- `F6`: map while choosing a letter.
- `Shift+F5` / `Shift+F6`: create temporary connections.
- `F8`: disconnect an existing mapping.
- `F4`: open a resource using explicitly supplied credentials.
- `net:`, `net:server`, or `net:\\server`: open the browser from the command line.
- `cd \\server` inside the plugin: switch to another server.

## ProcList

ProcList models processes as a virtual panel:

- `F3`: view detailed process information.
- `F8`: terminate selected processes.
- `Enter`: switch to the process window where possible.
- `F6`: inspect a remote machine.
- `Shift+F6`: return to the local machine.
- `Shift+F1` / `Shift+F2`: lower/raise local process priority class.
- `Shift+F3`: view details with overridden defaults.
- `Alt+Shift+F9`: configure the plugin.

Termination is immediate and can lose unsaved data.

## Temporary Panel

TmpPanel is a core Far convention for “results as files”:

1. Collect arbitrary paths from different directories into one virtual panel.
2. Sort, select, view, edit, copy, move, delete, or run commands against the collection.
3. Populate the panel from file lists, other plugins, macros, or command-line prefixes.
4. Keep original filesystem locations while presenting a unified working set.

This pattern is useful for search results, build artifacts, playlists, deployment sets, and cross-directory batch operations.

## Editor utility plugins

- **Align**: aligns selected text according to plugin options.
- **AutoWrap**: wraps paragraphs or editor input to configured margins.
- **Brackets**: finds matching delimiters or selects their contents.
- **DrawLine**: enters a line-drawing mode; cursor keys draw, `F2` changes single/double style, `F10` exits.
- **EditCase**: case conversion for current word or selection.
- **FarColorer**: language-aware syntax coloring and related editor behavior.
- **HlfViewer**: renders HLF source while authoring Far/plugin help.

These are accessed primarily through `F11` in the editor, plugin-assigned hotkeys, or macros.

## LuaMacro

LuaMacro is both a plugin and Far’s principal automation layer. It supports:

- Recorded keyboard macros.
- Lua and MoonScript macro files.
- Area-specific hotkeys.
- Conditional execution based on panel, item, selection, and command-line state.
- Event handlers.
- Added plugin-menu items.
- Added command-line prefixes.
- Scripted panel modules.
- Calls into Far, panel, editor, viewer, dialog, menu, mouse, bookmark, drive, and plugin APIs.

Macro areas include Common, Shell, Info, QView, Tree, Search, FindFolder, Viewer, Editor, Dialog, Menu, MainMenu, UserMenu, Disks, Help, Other, and shell/dialog AutoCompletion.

## Plugin conventions

- `F11`: context-sensitive plugin commands.
- `Alt+Shift+F9`: plugin configuration list.
- `Shift+F9`: configure the selected plugin in plugin-aware menus.
- `Shift+F1`: selected plugin’s help.
- Plugin command prefixes route command-line input directly to a plugin.
- Plugin panels should be treated as virtual filesystems: ordinary panel keys often work, but unsupported operations may be disabled or have plugin-specific meaning.
- Plugin help is version-matched and should be preferred over third-party descriptions.

## Not guaranteed by the bundle

Far’s official help lists examples from the wider ecosystem—registry editors, mail/news clients, spell checkers, database tools, media-tag editors, games, and many more. Those are examples of what the API permits, not promises that the portable build includes them.

Similarly, FTP appears prominently in historical Far descriptions, but the build-6703 portable package’s remote-transfer emphasis includes NetBox; users should inspect the actual `Plugins` folder rather than assume a historical default set.
