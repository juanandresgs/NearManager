# Near Command Prefixes

Near recognizes registered `prefix:arguments` input before sending the command line to the host shell. Prefixes are owned by resource providers or isolated command extensions and use the same semantic navigation and command paths as menus, keys, macros, and the command palette.

## Registration

`ResourceProvider::command_prefixes` publishes names and descriptions. `resolve_command_prefix` receives the exact text after the first colon plus the focused location and returns a provider-neutral destination. The local filesystem provider registers `file:`; absolute native paths, relative paths, and `file://` URIs navigate the focused panel.

`CommandExtension::command_prefixes` maps a prefix to one of the extension's registered command IDs and one declared string argument. Wasm and process plugin manifests may declare `[[prefixes]]` entries with `name`, `description`, `command`, and `argument`. Invocation still passes through the normal typed command registry, extension isolation boundary, diagnostics, and semantic effect handling.

## Resolution Rules

- Prefix names contain at least two characters, begin with an ASCII letter, and then use letters, digits, `_`, or `-`.
- One-letter drive-like input such as `C:work` is never claimed unless it is ordinary shell input; Near intentionally forbids one-letter prefix registration.
- Duplicate names across providers and extensions fail during registration.
- A plugin prefix must target a command declared by that plugin and an argument declared by the resolved command descriptor.
- Unknown prefixes fall through unchanged to the configured command-line executor.
- Prefix invocations enter persistent command history exactly like shell commands.

F9 → Command prefixes, or `near.command-prefixes.show`, lists every effective route, owner, and description.
