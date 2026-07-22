# M3 Filter Evidence

Date: 2026-06-23

## Implemented Slice

- Versioned `filters.toml` catalogs define reusable filters and named mask groups.
- Provider-neutral predicates compose masks, kinds, sizes, modified-time bounds, read-only, executable, hidden, and ignore metadata.
- Include, exclude, force-include, and force-exclude modes have deterministic precedence.
- Ctrl+Shift+F and F9 open the filter menu; toggles are independent per panel and clearing is explicit.
- Active filter state is visible as `*` in the affected panel border.
- Metadata hydration and filter evaluation run on background listing tasks.

## Automated Evidence

- Filter unit tests prove named mask groups, precedence modes, and size/date/attribute composition.
- Workspace tests prove filter-menu rendering, focused-panel isolation, background refresh, and the visible panel marker.
- Keymap tests prove modified letter bindings such as Ctrl+Shift+F remain distinct from function keys.
- Layered configuration tests include the shipped `filters.toml` document.

## Requirement Status

`FAR-SELECT-003` is verified by reusable named catalogs, composable metadata criteria, independent panel state, and visible runtime feedback.
