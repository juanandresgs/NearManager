# Far Manager 3 Visual Coverage

## Purpose

The source index in `assets/farmanager-ux/manifest.csv` complements the interaction research by identifying concrete screen states rather than only describing them. It covers broad legacy product imagery, recent official-project issue attachments, a complete remote-session walkthrough, and historical references. Third-party media is not redistributed in this repository.

## Coverage Matrix

| UX area | Coverage | Best references | Notes |
|---|---|---|---|
| Dual file panels and key bar | Strong | `screenshots/third-party/07-evg-main-panels.png`, `screenshots/official-community/1090-theme-readability-01.png`, `screenshots/official-community/672-keybar-01.png` | Includes a clean Far 3 panel view and recent theme/key-bar details. |
| Panel variants | Strong | `screenshots/manufacturer/06-info-panel.png`, `07-quick-view.png`, `10-tree-panel.png` | Information, quick-view, and tree panel modes are visible. |
| File search | Strong | `screenshots/manufacturer/01-file-search-overview.png`, `05-file-search-results.png` | Search form and result navigation are represented. |
| Internal editor | Strong | `screenshots/manufacturer/03-editor.png`, `screenshots/official-community/1044-editor-line-numbers-01.png`, `940-editor-find-all-01.png` | Includes editing, line numbers, Find All, and result navigation. |
| Internal viewer | Strong | `screenshots/manufacturer/04-viewer.png`, `screenshots/official-community/1107-viewer-editor-layout-01.gif` | Text/hex viewing and recent viewer layout behavior are represented. |
| Copy operation | Partial | `screenshots/official-community/1038-copy-progress-01.jpg`, `screenshots/historical/legacy-gallery/04-gallery-copy-edit.png` | Progress is visible; the modern destination/options dialog remains missing. |
| Elevation | Strong | `screenshots/manufacturer/12-elevation.png` | Shows the administrative-operation prompt. |
| Help | Strong | `screenshots/manufacturer/11-help.png` | Internal hypertext help viewer is visible. |
| Color and theme configuration | Strong | `screenshots/manufacturer/08-color-setup.png`, `screenshots/official-community/1025-theme-menu-01.png`, `1090-theme-readability-02.png` | Covers older color setup and recent theme selection/readability. |
| Panel mode configuration | Strong | `screenshots/manufacturer/09-panel-mode-setup.png` | Column and panel-mode configuration is visible. |
| Save As and code pages | Strong | `screenshots/official-community/913-save-as-bom-01.png`, `977-codepage-warning-01.png` | Encoding selection, BOM option, and unsupported-character warning are covered. |
| Dialog focus and controls | Strong | `screenshots/official-community/893-dialog-focus-01.gif` | Demonstrates button focus and dialog interaction states. |
| Command line/prefix state | Partial | `screenshots/official-community/1048-command-prefix-01.gif` | Shows command-prefix processing but not command history or completion menus. |
| Drive/plugin selection | Strong | `screenshots/workflow-guides/netbox/netbox.png` | Shows the `Alt+F1` drive menu with the NetBox plugin entry. |
| SFTP/NetBox workflow | Strong | `screenshots/workflow-guides/netbox/` | Six ordered frames cover plugin selection, session editing, saving, password entry, and connected state. |
| FTP/network panels | Strong | `screenshots/manufacturer/02-ftp-panel.png`, `screenshots/official-community/1039-network-panel-01.png` | Includes legacy FTP and recent network-panel behavior. |
| Multimedia plugin | Historical | `screenshots/historical/far-manager-multimedia-viewer.png` | Demonstrates a graphical plugin hosted inside Far. |
| Historical evolution | Strong | `screenshots/historical/far-manager-1-80.png`, `far-manager-2.png`, `legacy-gallery/` | Provides Far 1.x, Far 2.x, and older gallery comparisons. |

## NetBox Workflow Sequence

1. Open the drive menu and select NetBox: `screenshots/workflow-guides/netbox/netbox.png`.
2. Open session editing: `screenshots/workflow-guides/netbox/edit-session.png`.
3. Enter host and protocol settings: `screenshots/workflow-guides/netbox/session-settings.png`.
4. Save the named session: `screenshots/workflow-guides/netbox/save-session-as.png`.
5. Enter credentials: `screenshots/workflow-guides/netbox/entering-password.png`.
6. Work in the connected remote panel: `screenshots/workflow-guides/netbox/ftp-connection-is-ready.png`.

## Remaining Capture Gaps

The corpus is broad enough to reconstruct Far's principal visual language and several end-to-end workflows, but a genuinely exhaustive current-build set still requires first-party capture on Windows. Highest-priority missing states are:

- The current `F9` top menu and each Files, Commands, Options, and Right/Left submenu.
- Current copy/move destination, delete confirmation, make-folder, attributes, link, and selection dialogs.
- `F11` plugin menu, `F2` user menu, file associations, and history menus.
- ArcLite archive navigation and extraction, temporary panel, process list, and hotplug panels.
- Macro recording/playback indicators, macro settings, and `far:config`.
- Keyboard navigation transitions, multi-selection, drag/drop, completion, and error recovery sequences.
- Clean default-theme captures at standard 80×25, 120×30, and wide terminal sizes from the latest build.

## Source Quality Guidance

Use `official-community` sources first for recent control geometry and behavior, `third-party/07-evg-main-panels.png` for a clean Far 3 overview, `manufacturer` sources for breadth, and `historical` sources only when studying continuity or plugin concepts. Resolve each identifier through `assets/farmanager-ux/manifest.csv` and verify its source terms before downloading or reusing it.
