# Near User Menus

Near models Far-style user menus as versioned typed automation, not interpolated command strings. `F2` opens the global menu and `Shift+F2` opens the local menu. Both are also available from the F9 main menu and normal command registry.

## Configuration

`specs/user-menu.toml` uses `schema = 1` and contains ordered `[[global]]` and `[[local]]` entries. Each entry has a stable `id`, visible activation `key`, label, description, optional provider-neutral resource predicate, and one invocation template.

Structured `mode = "argv"` entries name a program and typed argument atoms. Supported atoms are literal text, focused URI/name/location, peer URI/name/location, selected URIs/names, and a temporary resource list. Multi-value selected atoms become separate argv items, preserving exact argument boundaries.

Explicit `mode = "shell"` entries name a shell and script. Placeholders use `${focused.uri}`, `${focused.name}`, `${focused.location}`, `${peer.uri}`, `${peer.name}`, `${peer.location}`, `${selected.uris}`, `${selected.names}`, and `${temp.list}`. Every substituted value is shell-quoted and the UI labels the invocation `EXPLICIT SHELL`.

## Selection and Temporary Lists

Selected metasymbols use every selected non-parent entry in panel order. If nothing is selected, the focused resource is used. Temporary lists contain provider-neutral resource URIs, one per line, so remote and virtual providers remain representable. The file is registered on `ExternalInvocation` and removed after the child process exits, including unsuccessful exit statuses.

## Safety and Diagnostics

Entries whose predicates do not match remain visible but disabled. Missing focused, peer, or selected values fail before process launch. Duplicate IDs within one scope, unsupported schemas, invalid predicates, content predicates, and unknown shell placeholders fail during loading or resolution. Global and local scopes may intentionally reuse an ID.
