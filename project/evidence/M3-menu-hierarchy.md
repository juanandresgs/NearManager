# M3 Menu Hierarchy Evidence

Date: 2026-07-05

## Implemented Slice

- F9 opens the active Left or Right panel menu directly beneath a persistent five-category bar on the physical top screen row.
- Submenus are anchored beneath their selected category instead of being rendered as centered modal dialogs; pointer hit testing shares that geometry.
- Left and Right traverse the five categories; Tab and Shift+Tab switch panel-specific menus.
- Category contents follow the Far help inventory and preserve canonical ordering before Near-specific extensions.
- Each category remains backed by normal typed command invocations.
- Left and Right explicitly target their named panel before panel commands are shown.
- Visible `[X]` accelerators activate enabled entries; ordinary menu text filtering remains available.
- The central command registry determines enabled state and supplies concise unavailable reasons.
- A versioned action catalog covers static commands and exact ordered command sequences for all five categories.
- Every static action is activated through the real menu route and must produce its effect or an explicit denial; disabled actions may not swallow Enter or accelerators.
- View/Edit availability additionally reflects the current resource, provider capability, configured policy, and external resolver.

## Automated Evidence

- `menu::tests::accelerators_activate_enabled_items_and_ignore_disabled_items` proves accelerator dispatch and disabled-item behavior.
- `workspace::tests::f9_opens_the_active_panel_menu_and_switches_far_categories` snapshots the top-row bar, row-1 anchored dropdown, pointer category switching, direct-open behavior, five-category traversal, Tab switching, and exact panel targeting.
- `workspace::tests::main_menu_order_matches_the_versioned_far_layout` rejects category-order drift against `specs/menu-actions.toml`.
- `workspace::tests::canonical_main_menus_have_unique_accelerators` rejects ambiguous menu accelerators.
- `workspace::tests::file_menu_disables_unavailable_actions_with_registry_reasons` proves unavailable resource actions stay in the menu with an explanation and cannot activate.
- `workspace::tests::surface_gallery_routes_filtering_navigation_and_text_input` proves nested menu filtering still routes text correctly.
- `workspace::tests::static_menu_catalog_matches_the_versioned_action_manifest` prevents shipped menu actions from escaping the audit catalog.
- `workspace::tests::every_static_menu_action_activates_to_an_effect_or_explicit_denial` rejects inert actions and silently swallowed disabled actions.
- `workspace::tests::fixed_nested_menu_actions_activate_to_an_effect_or_explicit_denial` applies the same route-level contract to sort, panel-mode, provider-location, and selection options.
- `workspace::tests::file_menu_only_enables_view_and_edit_when_the_current_resource_can_complete_them` proves View/Edit applicability and their viewer/editor outcomes.
- `near-demo::tests::public_menu_contract_dispatches_disabled_items_for_application_denials` proves a non-file-manager consumer can inspect menus and surface application-owned denial reasons.
- `tools/test_tmux_terminal_workflows.py` proves the built Near binary reports F3 denial on `..` and opens a real file through F3/F4 in the viewer/editor.

The full assessment is recorded in `project/evidence/menu-action-assessment.md`.

## Requirement Status

`FAR-MENU-001` remains partial until `OP-MENU-001` is recorded on the required macOS and Linux terminal matrix. Automated structure is implemented; operator workflow proof is still mandatory.
