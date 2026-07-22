# Far Manager 3 Advanced Panels and Custom Workflows

This appendix covers the less obvious workflows that make Far more than a two-pane copy utility: specialized panels, reusable filters, view-mode design, associations, user menus, metasymbols, descriptions, and batch-command construction.

## Tree panel

The tree panel shows the current drive’s directory hierarchy and can replace either side of the normal file-panel pair.

- Open/toggle with `Ctrl+T`.
- Move the cursor to navigate; with Auto change folder enabled, the opposite panel follows immediately, otherwise press `Enter`.
- Hold `Alt` and type to fast-find a folder; `Ctrl+Enter` advances to the next match.
- `Gray +` / `Gray -` jump to the next/previous branch at the same level.
- `Ctrl+R` rebuilds stale tree information after external filesystem changes.

Far stores drive trees in `tree3.far` at the drive root. For read-only drives, cache data is stored under `Tree.Cache` beside Far. The cache is created only after Tree Panel or Find Folder is first used and is subject to the configured folder-count threshold.

## Information panel

The information panel summarizes the opposite panel’s context.

Sections include:

- Computer and user network identities.
- Current drive type, filesystem, network target, capacity, free space, volume label, and serial number.
- Physical, virtual, and pagefile memory status.
- Current folder’s description file, with built-in viewer/editor access.
- Plugin-supplied information for an opposite plugin panel.
- AC and battery status.

Controls:

- `Ctrl+L`: toggle information panel.
- `Ctrl+F12`: choose visible sections.
- `+`, `-`, `*` in the section menu: show, hide, or toggle a section.
- `Ctrl+Shift+S`: switch capacities between byte values and scaled suffixes.
- `F3` / left-click on folder description: view it.
- `F4` / right-click: edit or create it.

InfoPanel settings choose power/CD details and computer/user identity formats, including NetBIOS, DNS, physical versus cluster identities, SAM-compatible names, UPNs, canonical names, GUIDs, and directory-style names.

## Quick-view panel

Quick view dedicates one panel to previewing the current item in the opposite panel.

- `Ctrl+Q`: toggle.
- For files, it embeds internal-viewer behavior and supports viewer search, encoding, and navigation commands.
- For folders, it calculates logical size, allocation size, file/subfolder counts, cluster size, and slack-inclusive real size.
- For reparse points, it shows the target path.
- `Ctrl+Shift+S`: switch size formatting.

Totals can exceed intuitive size because symbolic-link traversal and multiple hard links affect counting.

## Custom file-panel modes

Far provides ten default modes mapped to `LeftCtrl+1`…`9` and `LeftCtrl+0`, but each can be redesigned.

A mode controls:

- File-list column types.
- Column widths.
- Status-line column types and widths.
- Full-screen versus half-screen presentation.
- Name alignment.
- Name case transformation for display.
- Whether folders appear in uppercase or lowercase.
- Whether extensions are displayed in uppercase or lowercase.

Repeating equal column groups creates newspaper-like vertical “stripes,” allowing compact multi-column lists.

Available properties include filename, extension, size, allocation size, timestamps, attributes, description, owner, hard-link count, stream information, and plugin-provided content fields. Modifiers control formatting such as human-readable sizes, date/time portions, name/extension separation, and width behavior.

Display case never changes real filenames.

### Column grammar

| Type | Meaning and modifiers |
|---|---|
| `N` | Name. `M` shows selection marks; `MD` makes them dynamic; `O` removes paths; `R` right-aligns truncated names; `RF` right-aligns all names; `N` modifier suppresses extension in this column. Modifiers combine, e.g. `NMR`. |
| `X` | Extension; `R` right-aligns it. |
| `S` | Logical size. |
| `P` | Allocation size. |
| `G` | Total stream size. |
| `D` | Last-write date. |
| `T` | Last-write time. |
| `DM` | Last-write date and time. |
| `DC` | Creation date and time. |
| `DA` | Last-access date and time. |
| `DE` | Change date and time. |
| `A` | Attributes. |
| `Z` | Description. |
| `O` | Owner; `L` includes domain. |
| `LN` | Hard-link count. |
| `F` | Stream count. |

Size types `S`, `P`, and `G` accept:

- `C`: digit grouping from Windows regional settings.
- `T`: decimal units (1000) rather than binary units (1024), with lowercase suffixes.
- `F`: compact decimal fraction, such as `1.00 K`.
- `E`: remove the space before the unit.

Combined date/time types accept `B` for brief Unix-like format and `M` for month names.

Attribute letters are `N` none, `R` read-only, `H` hidden, `S` system, `D` directory, `A` archive, `T` temporary, `$` sparse, `L` reparse, `C` compressed, `O` offline, `I` not indexed, `E` encrypted, `V` integrity stream, `?` virtual, `X` no-scrub data, `P` pinned, and `U` unpinned. Increase the default six-character attribute width to reveal more flags.

Width rules:

- `0` means default width; for Name, Description, and Owner it means automatic width.
- Suffix a width with `%` to allocate that percentage of remaining space after fixed columns.
- If percentage totals exceed 100%, Far scales them proportionally.
- Keep at least one automatic-width column for layouts that adapt to terminal resizing.
- Increasing time width enables 12-hour format, then seconds and milliseconds.
- Increasing date width by two enables four-digit years.
- Owner, link, and stream columns can slow directory enumeration.

### Useful custom modes

- **Build output**: name, size, modified time, full path/content metadata.
- **Media inventory**: filename plus plugin-provided content columns.
- **Storage audit**: allocation size, logical size, hard-link count, streams.
- **Deployment view**: filename, attributes, modification time, owner.
- **Description catalog**: wide description with a narrow filename column.

## Highlighting and sort groups

Highlight rules classify items by masks and attributes, then assign:

- Normal filename color.
- Selected filename color.
- Cursor filename color.
- Selected-under-cursor color.
- An optional textual mark.
- Optional continuation into lower-priority rules.

Rule list controls:

| Action | Key |
|---|---|
| Add | `Ins` |
| Duplicate | `F5` |
| Delete | `Del` |
| Edit | `Enter`, `F4` |
| Restore defaults | `Ctrl+R` |
| Reorder | `Ctrl+Up`, `Ctrl+Down` |

Rules are evaluated top-to-bottom. The first match stops processing unless “Continue processing” is enabled.

Attribute conditions are tri-state:

- Required.
- Forbidden.
- Ignored.

NTFS/ReFS-specific attributes include compressed, encrypted, not indexed, sparse, temporary, reparse point, integrity stream, and no-scrub data.

Sort groups use similar masks to force semantic groups into configured positions while normal sorting applies inside each group. Toggle group sorting with `Shift+F11`.

## Filters

Filters are reusable predicates for panels, Find File, and copy/move operations. They can combine:

- Masks.
- Attributes.
- Date ranges.
- Size ranges.
- Other filter-dialog criteria.

The filter menu has saved user filters and automatically generated masks from the current panel.

| Action | Key |
|---|---|
| Add/edit/duplicate/delete filter | `Ins` / `F4` / `F5` / `Del` |
| Reorder | `Ctrl+Up`, `Ctrl+Down` |
| Include | `Space` or `+` |
| Exclude | `-` |
| High-priority include/exclude | `I`, `X` |
| Clear current | `Backspace` |
| Clear all | `Shift+Backspace` |

If positive entries exist, only matches pass. Negative entries reject matches. `I` and `X` have higher priority for resolving overlaps. An active panel filter is shown by `*` after the sort-mode letter.

## File descriptions

Descriptions associate free-form text with files through a description-list file in the folder.

- `Ctrl+Z`: describe selected/current items.
- Default panel modes 6 and 7 display descriptions.
- Configure accepted description filenames, update policy, hidden attribute, alignment, and read-only handling.
- Updates can be disabled, limited to description-aware views, or always enabled.
- Far can update descriptions during ordinary copy, move, rename, and delete operations.
- Subfolder descriptions are not updated by operations that process a recursive tree.
- Encoding can default to OEM or ANSI and optionally save as UTF-8; an existing UTF-8 BOM always controls decoding.

Folder descriptions are a separate list of wildcard filenames displayed by the information panel.

## User menu

Press `F2` to open a user-defined command hierarchy.

Far chooses among:

1. A local `FarMenu.ini` in the current folder.
2. The user-specific main menu in the profile.
3. A global menu beside Far, which overrides the user-specific main menu when present.

Controls:

- `Shift+F2`: switch local/main menu.
- `Backspace`: use the parent folder’s menu.
- `Ins`: add item or submenu.
- `F4`: edit item or submenu.
- `Del`: delete.
- `Ctrl+Up` / `Ctrl+Down`: reorder.
- `Alt+F4`: edit the menu as text.
- `Shift+F4`: edit when an item has claimed `F4` as its hotkey.
- `Shift+F10`: close all open menu levels.

Hotkeys may be digits, letters, or `F1`…`F24`. Use `--` as the hotkey to create a separator.

Item titles and command lines support Far metasymbols, so menu entries can describe and operate on panel state dynamically.

## File associations

Far associations bind a mask or regular expression to six independently configurable actions:

- `Enter` execute.
- `Ctrl+PgDn` alternate execute/open.
- `F3` view.
- `Alt+F3` alternate view.
- `F4` edit.
- `Alt+F4` alternate edit.

Association list controls are `Ins`, `F4`, `Del`, `Ctrl+Up`, and `Ctrl+Down`.

Multiple matching associations produce a chooser. Conditional shell commands such as `IF EXIST` and `IF DEFINED` can hide unusable choices. Regular-expression masks expose numbered or named captures as `%RegexGroupN` and `%RegexGroup{Name}` in command templates.

If no Far execute association matches and “Use Windows registered types” is enabled, Windows Shell association is the fallback.

## Metasymbols

Metasymbols are shared by file associations, user menus, and Apply Command.

| Symbol | Expansion |
|---|---|
| `!!` | Literal `!`. |
| `!` | Long filename without extension. |
| `!~` | Short filename without extension. |
| `` !` `` | Long extension. |
| `` !`~ `` | Short extension. |
| `!.!` | Long filename with extension. |
| `!-!` | Short filename with extension. |
| `!+!` | Short name, restoring a lost long name after execution where possible. |
| `!@@!` | Temporary file containing selected long names. |
| `!$!` | Temporary file containing selected short names. |
| `!&` | Selected filenames as command arguments. |
| `!&~` | Selected short filenames. |
| `!:` | Current drive or UNC share root. |
| `!\` | Current path. |
| `!/` | Short current path. |
| `!=\` | Current path with symbolic links resolved. |
| `!=/` | Short resolved path. |
| `!?!` | Current file description. |

Far does not add quoting automatically. Templates must quote expansions when the target program requires it:

```text
program.exe "!.!"
```

### Panel selectors

Selectors change which panel subsequent symbols reference:

- `!^`: active panel.
- `!##`: passive panel.
- `![`: left panel.
- `!]`: right panel.

They act as toggles until another selector appears, allowing templates that combine paths and names from several panels.

### Selected-list modifiers

Temporary list-file symbols support:

- `Q`: quote names.
- `S`: use `/` path separators.
- `F`: full paths.
- `A`: ANSI encoding.
- `U`: UTF-8.
- `W`: UTF-16LE.

Direct selected-name lists support `Q` to quote and `q` not to quote.

### User prompts

```text
!?Title?Initial value!
```

prompts immediately before execution. Multiple prompts can appear in one command. Named histories make entered values reusable in later prompts and as `%HistoryName`/`%UserVarN` substitutions.

Example:

```text
grep !?Search for:?! !?In:?*.*! | Far.exe -v -
```

## Apply Command

Apply Command runs a template once for every selected item, or the current item if nothing is selected.

Examples:

```text
type !.!
explorer /select,"!.!"
tool.exe "!\!.!"
```

Use it when each file needs a separate process invocation. Use `!&` or a generated list file when one process should receive the whole selection at once.

## Workflow patterns

### Compare the same file across panels

1. Put two versions’ folders in the panels.
2. Create a user-menu or association command that combines active filename with both paths.
3. Use panel selectors so the same active name is looked up on the passive side.
4. Guard the command with `IF EXIST` to offer it only when both files exist.

### Project-local tool menu

1. Place `FarMenu.ini` in the project root.
2. Add build, test, format, deploy, and log-view entries.
3. Use prompts for target/configuration arguments.
4. Use `Far.exe -v -` at the end of a pipeline to inspect output inside Far.
5. Navigate into the project and press `F2`; the local menu appears automatically.

### Filtered deployment copy

1. Create filters for deployable extensions, excluded temporary directories, dates, and attributes.
2. Open the copy dialog with `F5`.
3. Apply the filter and review the destination/options.
4. Keep the filter saved for repeat deployments.

### Storage and metadata audit

1. Create a custom view mode showing size, allocation size, links, streams, owner, and timestamps.
2. Add highlighting rules for compressed, encrypted, sparse, reparse, and large files.
3. Use numeric size sorting and sort groups.
4. Send interesting items to a temporary panel for batch processing.

### Description-driven catalog

1. Enable description updates.
2. Use `Ctrl+Z` to annotate items.
3. Switch to Description or Long Description mode.
4. Use description sorting and search/filter tools to treat a folder as a lightweight catalog.

## Colors and themes

The Colors menu assigns foreground, background, and text style to UI groups or applies a theme.

Built-in themes:

- **Default**: console-palette indices, whose actual RGB values depend on the terminal palette.
- **Default (RGB)**: fixed device-independent RGB values approximated to palette colors when true color is unavailable.
- **Custom themes**: community theme files under `%FARHOME%\Addons\Colors\Interface`.

The color picker supports:

- Standard 16-color Windows/DOS palette.
- ANSI 256-color palette: 16 standard colors, a 6×6×6 color cube, and a 24-step grayscale ramp.
- Full 24-bit RGB space.
- Alpha/transparency and inheritance from the previous logical layer.
- Terminal default foreground/background.
- A custom RGB favorites palette and the Windows system picker.

Foreground styles include bold, italic, overline, strikeout, faint, blink, inverse, invisible, and single/double/curly/dotted/dashed underline.

Only the 16-color palette is universally reliable. ANSI-256, RGB, alpha blending, and advanced styles depend on the terminal and normally require “Use Virtual Terminal for rendering” in Interface settings.
