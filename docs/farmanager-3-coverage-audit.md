# Far Manager 3 Research Coverage Audit

## Authoritative evidence

- Official portable build inspected locally: `Far30b6703.x64.20260623.7z` (SHA-256 `a648c004ef6e570bd9267e7646d288cd8cbd02e753333dcdadeba39ef30df40b`; not redistributed in this repository).
- Shipped version: `3.0.6703.0 x64`, dated 2026-06-23.
- Shipped English core help: `FarEng.hlf` from that archive.
- Official source snapshot: commit `28443e51d9f7b4b9e5976d1db8246532ccc40205`, dated 2026-06-20.
- Official LuaMacro manual: `enc/enc_lua/macroapi_manual.en.tsi` from that source snapshot.
- Bundled plugin help and files from the downloaded archive.

The audit treats the objective as user/operator research. Exhaustive C++ Plugin API and function-by-function LuaFAR developer reference are separate software-development documentation, not user-facing Far Manager workflows.

## Requirement audit

| Requirement | Evidence | Result |
|---|---|---|
| Full feature families | Core file/panel/search/viewer/editor/command/extensibility inventory in `docs/farmanager-3-research.md`; detailed operations in `docs/farmanager-3-file-operations-and-search.md`. | Proven for build 6703 user-facing core. |
| Workflows | Two-panel operations, archives, search-to-panel, history, menus, associations, filters, catalogs, automation, deployment, and audit recipes across the main guide and advanced/operations appendices. | Proven. |
| Menus | Left/right, Files, Commands, and Options inventories; plugin, user, drive, filter, history, screen, and context-menu conventions. | Proven. |
| Hotkeys | Build-matched panel, command-line, viewer, editor, dialog, menu, drive, history, search, help, grabber, and macro keys in `docs/farmanager-3-hotkeys.md`. | Proven for defaults; customization caveat documented. |
| Conventions | Active/passive panels, cursor versus selection, modifier layers, virtual/plugin panels, metasymbol quoting, filter priority, history locking, and context-sensitive key behavior. | Proven. |
| Supported panels | File, tree, information, quick view, temporary/plugin panels and custom views. | Proven. |
| File operations | Copy, move, rename, delete, wipe, links, attributes, timestamps, streams/security caveats, and overwrite policy. | Proven. |
| Search | Masks, text, fuzzy, hex, encodings, scopes, archives, links, streams, filters, advanced criteria, result actions, and temporary panels. | Proven. |
| Viewer and editor | Modes, navigation, search/replace, code pages, selections, bookmarks, persistence, settings, and exact keys. | Proven. |
| Command execution | Command line, history, path/name insertion, associations, user menus, Apply Command, prompts, environment, prefixes, and elevation. | Proven. |
| Customization | Panel modes/column grammar, colors/themes/styles, highlighting, sort groups, filters, descriptions, confirmations, and advanced settings. | Proven. |
| Startup and profiles | All documented switches, target behavior, plugin loading, roaming/local profiles, exports/imports, and Far environment variables. | Proven. |
| Plugins | Actual build-6703 plugin inventory, capabilities, plugin-menu conventions, virtual panel behavior, and bundle-versus-ecosystem caveat. | Proven for downloaded distribution. |
| Macros and automation | Recording, script loading, regular macros, areas, keys, conflicts, flags/conditions, events, menu items, prefixes, panels, content columns, API namespaces, and troubleshooting. | Proven at user/workflow and automation-author level. |
| Windows integration | Registered types, Explorer context menus, network drives, tasks/processes, hotplug removal, console/VT rendering, elevation, and filesystem-specific behavior. | Proven. |
| Version specificity | Every exact-reference appendix identifies build 6703 or the matching source commit; change/customization caveats are explicit. | Proven. |

## Topic-family audit

The official help topic families are represented as follows:

- Orientation, command-line switches, key reference: main guide, hotkeys, startup appendix.
- Plugins: bundled-plugin and LuaMacro appendices.
- Panels: main guide and advanced-workflows appendix.
- Main menus and file operations: main guide and file-operations appendix.
- Search, histories, task/device lists: file-operations appendix.
- User menu, associations, masks, filters, descriptions, highlighting, themes: advanced-workflows and settings appendices.
- Viewer/editor/code pages: hotkeys, settings, and main guide.
- Drives, screens, command execution, environment, regex, elevation: hotkeys, startup, and file-operations appendices.
- Macros and advanced configuration: LuaMacro and settings appendices.

No major user-facing help family lacks a corresponding research section.

## Known variability, not missing research

- Macros can override default hotkeys.
- Plugins can add commands, panels, settings, columns, and file-format support.
- Terminals and Windows can intercept or fail to render some keys, colors, or styles.
- Filesystem and Windows-version capabilities affect links, attributes, streams, elevation, and device operations.
- Development builds evolve rapidly; the evidence is explicitly fixed to build 6703.

## Optional developer-level follow-ups

These are outside the user/operator objective but could be researched separately:

- Every C++ Plugin API structure and callback.
- Every LuaFAR function signature and flag constant.
- Every third-party plugin in PlugRinG.
- Historical change-by-change comparison of all Far 3 builds.

## Conclusion

The document set satisfies the requested user-facing research scope for Far Manager 3 build 6703: features, supported workflows, menus, default hotkeys, conventions, configuration, bundled extensions, and automation capabilities are all covered with build-matched primary evidence.
