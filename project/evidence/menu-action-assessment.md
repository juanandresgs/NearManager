# Near Menu Action Assessment

Date: 2026-07-05

## Conclusion

Two discovered failures were systemic rather than cosmetic. Near treated command registration as proof that a menu action was currently usable, and it treated the presence of five category names as proof that the hierarchy matched Far. The latter was false: F9 opened an extra category chooser and the submenu ordering was Near-specific.

The static menu surface now has three automated guarantees:

1. The 58 shipped static commands, their exact category order, and 50 fixed nested commands must match `specs/menu-actions.toml`; config-driven options must identify their versioned source catalog.
2. Every item is activated through the real menu route, including Enter and accelerators.
3. Activation must produce its effect or an explicit denial; disabled items may not swallow input silently.

F9 now opens the active panel side menu directly, renders the five-category bar, traverses categories with Left/Right, and switches panel menus with Tab. `FAR-MENU-001` remains partial until the operator-only macOS and Linux workflow evidence is recorded.

`View` and `Edit` additionally have resource- and policy-aware applicability. Internal `View` no longer aliases `Open`, so invoking View on a directory cannot navigate as a side effect. Files open the viewer/editor; parents, containers, unreadable resources, read-only resources, and missing external resolvers are denied explicitly.

The real tmux/PTTY workflow now proves the shipped binary behavior: F3 on `..` renders the navigation-only denial, F3 on `cargo.toml` renders file content in the internal viewer, and F4 renders the same file in the internal editor.

Activation through the real menu route prevents a completely dead or silently disabled menu option, but it is not by itself semantic proof that every workflow is correct. The matrix distinguishes focused workflow coverage from the route-level smoke gate.

## Static Menu Matrix

| Menu | Action | Command | Current assessment | Automated depth |
|---|---|---|---|---|
| Main | Left | `near.menu.left` | PASS: opens and targets left panel menu | hierarchy workflow |
| Main | Files | `near.menu.files` | PASS: opens Files menu | hierarchy workflow |
| Main | Commands | `near.menu.commands` | PASS: opens Commands menu | hierarchy workflow |
| Main | Options | `near.menu.options` | PASS: opens Options menu | hierarchy workflow |
| Main | Right | `near.menu.right` | PASS: opens and targets right panel menu | hierarchy workflow |
| Panel | Change location | `near.provider.choose` | PASS: opens provider/location chooser | real-route activation gate |
| Panel | Configure layouts | `near.panel.view-mode.menu` | PASS: opens every configured mode | interaction conformance |
| Panel | Sort modes | `near.collection.sort.menu` | PASS: opens sort menu and changes ordering | focused workflow |
| Panel | File panel filter | `near.filters.show` | PASS: opens filter menu and applies per-panel state | focused workflow |
| Panel | Tree panel | `near.panel.toggle-tree` | PASS: changes panel presentation | real-route activation gate |
| Panel | Information panel | `near.panel.toggle-information` | PASS: changes panel presentation | real-route activation gate |
| Panel | Quick view | `near.panel.toggle-quick-view` | PASS: opens peer preview and rejects stale loads | focused workflow |
| Panel | Re-read | `near.panel.refresh` | PASS: requests provider refresh visibly | real-route activation gate |
| Files | View | `near.resource.view` | PASS: file opens viewer; invalid targets are disabled/denied | focused applicability + workflow |
| Files | Edit | `near.resource.edit` | PASS: writable file opens editor; invalid targets are disabled/denied | focused applicability + workflow |
| Files | Copy | `near.resource.copy-to-peer` | PASS: creates operation preview/task | operation workflow |
| Files | Move | `near.resource.move-to-peer` | PASS: creates operation preview/task | operation workflow |
| Files | Rename | `near.resource.rename` | PASS: opens rename dialog and executes templates | focused workflow |
| Files | Create link | `near.resource.link` | PASS: opens typed link dialog | real-route activation gate + operation evidence |
| Files | Attributes | `near.resource.attributes` | PASS: opens attributes dialog | real-route activation gate + operation evidence |
| Files | New folder | `near.fs.create-directory` | PASS: opens create-directory dialog | real-route activation gate + filesystem evidence |
| Files | Create archive | `near.archive.create` | PASS: opens archive workflow when provider supports it | archive workflow |
| Files | Trash | `near.resource.trash` | PASS: protected targets denied; eligible targets preview reversible trash | deletion workflow |
| Files | Restore last Trash | `near.resource.restore-last-trash` | PASS when a completed Trash result is recorded; otherwise disabled with an explicit reason | deletion restoration workflow |
| Files | Delete permanently | `near.resource.delete` | PASS: opens irreversible confirmation path | real-route activation gate + deletion evidence |
| Files | Wipe files | `near.resource.wipe` | PASS: opens wipe options for eligible files | real-route activation gate + deletion evidence |
| Files | Describe | `near.resource.description` | PASS: opens description editor | description workflow |
| Commands | Find files | `near.search.start` | PASS: opens search dialog and streams results | search workflow |
| Commands | Save search panel | `near.search.keep-panel` | PASS or explicit denial when no result panel exists | search workflow |
| Commands | Saved panels | `near.search.panels` | PASS: opens saved panels or explicit empty message | real-route activation gate + search workflow |
| Commands | Temporary panels | `near.temp-panel.list` | PASS: opens ten mutable reference slots | temporary-panel workflow |
| Files | Selection commands | `near.selection.menu` | PASS: opens extended group-selection menu | interaction conformance |
| Files | Apply command | `near.operation.apply-command` | PASS or disabled when no source exists | operation workflow |
| Commands | History | `near.history.menu` | PASS: opens view/edit history | history evidence |
| Commands | Folder history | `near.location.history-show` | PASS: opens searchable folder history | history evidence |
| Commands | Command history | `near.command-line.history-show` | PASS: opens searchable command history | history evidence |
| Commands | Swap panels | `near.workspace.swap-peers` | PASS: exchanges the left and right surfaces | hierarchy workflow |
| Commands | User menu | `near.user-menu.global` | PASS: opens configured global automation or explicit state | user-menu evidence |
| Commands | Local user menu | `near.user-menu.local` | PASS: opens configured local automation or explicit state | user-menu evidence |
| Commands | Macros | `near.macro.manage` | PASS: manage/play/edit/bind/delete workflow | focused workflow |
| Commands | Terminal tabs | `near.terminal.menu` | PASS: opens persistent terminal tab, pane, zoom, and close actions | focused terminal workspace workflow |
| Commands | Tasks | `near.demo.tasks` | PASS: opens task history/progress surface | task evidence |
| Commands | Removable devices | `near.devices.show` | PASS: opens device list or explicit empty/degraded state | adapter contract; physical-device operator evidence remains platform-only |
| Commands | Screen list | `near.screen.list` | PASS: lists panels, editors, and user screen | focused workflow |
| Options | Settings | `near.settings.show` | PASS: searchable effective settings/provenance surface | focused workflow |
| Options | Highlighting | `near.highlighting.report` | PASS: opens effective rule/sort-group report | focused workflow |
| Options | Colors and themes | `near.theme.show` | PASS or explicit denial without editable runtime theme | theme workflow |
| Commands | File associations | `near.resource.associations` | PASS or disabled/denied without current resource/resolver | focused association workflow |
| Options | Command prefixes | `near.command-prefixes.show` | PASS: opens provider/extension prefix report | extension workflow |
| Options | Folder descriptions | `near.folder-description.view` | PASS: opens description or explicit missing state | focused workflow |
| Options | Edit folder description | `near.folder-description.edit` | PASS: opens/creates editable description | focused workflow |
| Options | Extensions | `near.extensions.show` | PASS: opens installed extension catalog | extension workflow |
| Options | Help | `near.help.context` | PASS: opens context-sensitive generated help | help workflow |
| Options | About | `near.about.show` | PASS: opens platform information | real-route activation gate |

## Nested Option Matrix

Every fixed nested-menu command is listed individually. Config-driven rows are instantiated once per entry in their versioned catalogs.

| Menu | Option | Command | Verified outcome |
|---|---|---|---|
| Sort | Name | `near.collection.sort.name` | PASS: reorders by name |
| Sort | Extension | `near.collection.sort.extension` | PASS: reorders by extension |
| Sort | Modified | `near.collection.sort.modified` | PASS: reorders by modified time |
| Sort | Size | `near.collection.sort.size` | PASS: reorders by size |
| Sort | Created | `near.collection.sort.created` | PASS: reorders by creation time |
| Sort | Accessed | `near.collection.sort.accessed` | PASS: reorders by access time |
| Sort | Kind | `near.collection.sort.kind` | PASS: reorders by resource kind |
| Sort | Owner | `near.collection.sort.owner` | PASS: reorders by owner |
| Sort | Permissions | `near.collection.sort.permissions` | PASS: reorders by permissions |
| Sort | Unsorted | `near.collection.sort.unsorted` | PASS: restores provider order |
| Sort | Reverse order | `near.collection.sort.toggle-reverse` | PASS: toggles descending order |
| Sort | Numeric names | `near.collection.sort.toggle-numeric` | PASS: toggles numeric name comparison |
| Sort | Selected first | `near.collection.sort.toggle-selected-first` | PASS: toggles selected-first grouping |
| Sort | Directories first | `near.collection.sort.toggle-directories-first` | PASS: toggles directory grouping |
| Sort | Highlighting groups | `near.collection.sort.toggle-groups` | PASS: toggles highlighting sort groups |
| Panel mode | Configured mode | `near.panel.view-mode.set` | PASS: applies the selected shipped/custom mode |
| Provider | Configured root/location | `near.provider.navigate` | PASS: navigates the explicitly targeted panel |
| Selection | Select by mask | `near.selection.select-mask` | PASS: opens include/exclude mask dialog |
| Selection | Unselect by mask | `near.selection.unselect-mask` | PASS: opens mask removal dialog |
| Selection | Same extension | `near.selection.same-extension` | PASS: selects matching extensions |
| Selection | Same name | `near.selection.same-name` | PASS: selects matching stems |
| Selection | Invert | `near.selection.invert` | PASS: inverts selectable entries |
| Selection | Save | `near.selection.save` | PASS: stores resource identities |
| Selection | Restore | `near.selection.restore` | PASS: restores surviving identities |
| Selection | Compare folders | `near.selection.compare-folders` | PASS: opens comparison policy dialog |
| Theme | Preset | `near.theme.preview` | PASS: previews selected configured theme |
| Theme | Semantic roles | `near.theme.roles` | PASS: opens role list |
| Theme | Commit preview | `near.theme.commit` | PASS: commits rollback baseline |
| Theme | Rollback preview | `near.theme.rollback` | PASS: restores committed baseline |
| Theme | Role | `near.theme.edit` | PASS: opens selected role editor |
| Filters | Configured filter | `near.filters.toggle` | PASS: toggles selected filter for focused panel |
| Filters | Clear | `near.filters.clear` | PASS: clears active filters or explains none are active |
| History | Viewed files | `near.history.viewed-show` | PASS: opens viewed-resource history |
| History | Edited files | `near.history.edited-show` | PASS: opens edited-resource history |
| Macros | Configured macro | `near.macro.actions` | PASS: opens actions for selected macro |
| Macro actions | Play | `near.macro.play` | PASS: replays through validation |
| Macro actions | Edit | `near.macro.edit` | PASS: opens macro editor |
| Macro actions | Bind | `near.macro.bind` | PASS: opens binding editor |
| Macro actions | Diagnose | `near.macro.diagnose` | PASS: renders conditions and safety diagnosis |
| Macro actions | Delete | `near.macro.delete` | PASS: opens deletion confirmation |
| Associations | Configured handler | `near.resource.association-run` | PASS: queues the selected semantic handler |
| User menu | Configured entry | `near.user-menu.run` | PASS: queues selected typed automation |
| Editor conflict | Reload | `near.editor.external-reload` | PASS: reloads external version |
| Editor conflict | Compare | `near.editor.external-compare` | PASS: opens read-only comparison |
| Editor conflict | Keep local | `near.editor.external-keep-local` | PASS: overwrites using explicit stale-version policy |
| Editor encoding | Confirm lossy save | `near.editor.lossy-save-confirmed` | PASS: saves only after explicit conversion approval |
| Screens | Panels | `near.screen.panels` | PASS: returns to panel workspace |
| Screens | Editor | `near.screen.editor` | PASS: activates selected editor index |
| Screens | User screen | `near.screen.terminal` | PASS: activates persistent terminal screen |
| Terminal tabs | New | `near.terminal.new` | PASS: creates and activates an independent persistent PTY tab |
| Terminal tabs | Next | `near.terminal.next` | PASS: cycles retained terminal identity and output |
| Terminal tabs | Previous | `near.terminal.previous` | PASS: cycles retained terminal identity and output in reverse |
| Terminal tabs | Place left | `near.terminal.place-left` | PASS: projects active terminal into the left peer pane |
| Terminal tabs | Place right | `near.terminal.place-right` | PASS: projects active terminal into the right peer pane |
| Terminal tabs | Hide | `near.terminal.hide` | PASS: restores the ordinary dual-panel composition without stopping tabs |
| Terminal tabs | Zoom | `near.terminal.open` | PASS: zooms and restores the exact previous pane composition |
| Terminal tabs | Close | `near.terminal.close` | PASS: applies per-session close policy to the active tab |
| Terminal tabs | Select retained tab | `near.terminal.select` | PASS: activates the selected stable terminal identity |
| Generated panels | Saved result | `near.search.open-panel` | PASS: reopens selected retained result panel |

### Dynamic Catalog Sources

The following shipped documents generate per-entry menu options and are validated by their domain parsers and focused workflows:

- `specs/panel-modes.toml`
- `specs/theme.toml`
- `specs/theme-terminal-native.toml`
- `specs/theme-high-contrast.toml`
- `specs/filters.toml`
- `specs/user-menu.toml`
- `specs/macros.toml`
- `specs/handlers.toml`
- `specs/handlers-linux.toml`
- `specs/handlers-windows.toml`

## Dynamic/Nested Menu Matrix

| Surface family | Assessment | Evidence level | Remaining risk |
|---|---|---|---|
| Sort modes | PASS | list-navigation conformance and sort workflows | none known |
| Panel view modes | PASS | list-navigation conformance and mode application | terminal-size variants remain operator coverage |
| Provider locations | PASS | provider navigation and retained-state tests | real remote providers remain adapter-specific |
| Selection actions | PASS | collection selection model/render/application tests | numeric keypad behavior depends on terminal protocol |
| File associations | PASS | ordered alternatives and named selection test | launching native GUI apps remains platform/operator evidence |
| User menus | PASS | typed automation persistence/execution evidence | user-authored shell commands are intentionally external |
| Filters | PASS | per-panel filter workflow | none known |
| View/Edit history | PASS | history surfaces and reopen actions | stale external resources produce explicit failures |
| Macro manager/actions | PASS | full lifecycle focused test | terminal recordings remain operator evidence |
| Themes/semantic roles | PASS | preview/edit/commit/rollback workflow | color fidelity varies by terminal capability |
| Extensions | PASS | catalog, settings, command, help, and generated-panel tests | third-party process plugins remain trust-boundary specific |
| Editor external-change/lossy-save | PASS | editor conflict and encoding workflows | native external editors remain platform/operator evidence |
| Screens | PASS | panels/editor/terminal switching workflow | none known |
| Saved generated panels | PASS | search and extension result retention tests | stale providers produce explicit state |
| Temporary panels | PARTIAL | slot, copy-as-reference, contextual removal, and semantic-render tests | list import/export, stale refresh, exact reveal cursor, and operator evidence remain |
| Terminal tabs | PASS | tab registry, pane focus, zoom restoration, public consumer, and tmux workflow | arbitrary split trees and restart persistence remain experimental |

## Evidence Index

| Scope | Authoritative automated evidence |
|---|---|
| Public reusable menu behavior | `near-demo::tests::public_menu_contract_dispatches_disabled_items_for_application_denials` |
| Every static menu entry through Enter | `workspace::tests::every_static_menu_action_activates_to_an_effect_or_explicit_denial` |
| Fixed nested menu entries through Enter | `workspace::tests::fixed_nested_menu_actions_activate_to_an_effect_or_explicit_denial` |
| Static catalog completeness | `workspace::tests::static_menu_catalog_matches_the_versioned_action_manifest`; `tools/validate_menu_actions.py` |
| View/Edit applicability and internal surfaces | `workspace::tests::file_menu_only_enables_view_and_edit_when_the_current_resource_can_complete_them` |
| Built-binary F3/F4 behavior | `tools/test_tmux_terminal_workflows.py` |
| Open/navigation/archive behavior | `workspace::tests::filesystem_provider_drives_navigation_parent_and_view_workflows`; `workspace::tests::archives_open_as_panels_extract_and_create_through_workspace_commands` |
| Copy/move/rename/link/attributes | operation, drag, rename, link, and attribute workflow tests in `workspace::tests` |
| Trash/delete/wipe | deletion intent, confirmation, execution, and mount-safety tests plus `workspace::tests::permanent_delete_and_wipe_require_two_step_high_impact_confirmation` |
| Search/generated panels | `workspace::tests::recursive_search_streams_actionable_source_resources_without_blocking_input`; extension generated-panel tests |
| Selection | collection model/render tests and `workspace::tests::selection_menu_masks_and_saved_sets_drive_panel_selection` |
| Histories | `workspace::tests::viewed_and_edited_histories_persist_filter_lock_clear_and_reopen`; command/folder history surface tests |
| Macros/user menus | `workspace::tests::macro_manager_edits_binds_diagnoses_replays_deletes_and_persists`; `workspace::tests::configured_user_menu_entries_activate_through_the_menu_route` |
| Terminal/screens/tasks | `workspace::tests::embedded_terminal_opens_from_the_far_binding`; user-screen and task-surface tests |
| Settings/themes/highlighting | typed settings, theme preview/commit/rollback, and highlighting report tests |
| Associations/descriptions/extensions | association selection, description edit/view, extension catalog/settings/command tests |
| Devices | `workspace::tests::removable_device_disconnect_is_capability_gated_and_audited`; physical device operation remains operator-only |

## What This Discovery Changes

- A command being registered and dispatchable is only wiring evidence, not functional evidence.
- Menu availability must be evaluated against the current resource, provider capabilities, configured policy, and required adapters.
- `Open`, `View`, and `Edit` are separate intents. Reusing one implementation for another is unsafe when navigation and document viewing differ.
- Every new static menu action must enter the versioned catalog and pass the real-route activation test.
- Every significant action still needs a focused scenario that proves the promised filesystem, task, provider, or document result; the generic gate only catches dead or silently inert actions.

## Remaining Operator-Only Evidence

The user should not need to manually discover dead options. Manual qualification remains appropriate only for behavior that automation cannot faithfully simulate: real removable-device eject/unmount, launching native GUI associations/editors, terminal-specific rendering/input fidelity, and platform package integration.
