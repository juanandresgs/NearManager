# M3 User Menu Evidence

Date: 2026-06-23

## Implemented Slice

- `near-handlers` defines schema-versioned global and local menu catalogs with typed structured-argv and explicitly marked shell templates.
- Focused, peer, selected, panel-location, and temporary-list metasymbols expand from provider-neutral resource metadata.
- Selected values preserve argument boundaries; temporary files contain resource URIs and are attached to the external invocation lifecycle for cleanup.
- `near-ui` exposes F2 global and Shift+F2 local menus, predicate availability, invocation-mode descriptions, normal command dispatch, and F9 menu access.
- `near-fm` loads the layered `user-menu.toml` document through CLI, environment, platform, user, trusted-workspace, and plugin-default configuration layers.

## Automated Evidence

- Handler tests prove exact structured argv expansion, provider-neutral temporary-list contents, cleanup registration, explicit shell mode, and hostile-name quoting.
- Terminal-session tests prove registered temporary files are removed after external execution and the terminal is restored.
- The complete workspace compiles with the shipped global and local menu entries.

## Requirement Status

`FAR-AUTO-003` is verified. `FAR-SHELL-004` is partial because associations and typed user commands are implemented while registered command-prefix routing remains outstanding.
