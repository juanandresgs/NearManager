# M3 Command Prefix Evidence

Date: 2026-06-23

## Implemented Slice

- The provider and extension public contracts expose default-compatible command-prefix registration.
- The workspace validates syntax and collisions, resolves registered prefixes before shell execution, records them in command history, and leaves unknown or drive-like input unchanged for the shell.
- Provider prefixes resolve provider-neutral locations and navigate through normal paged panel loading.
- Extension prefixes target registered semantic commands with typed string arguments and therefore retain availability, safety, isolation, diagnostics, and semantic effects.
- Wasm and process manifests support validated prefix-to-command mappings.
- F9 and `near.command-prefixes.show` expose effective owners and descriptions.

## Automated Evidence

- Workspace tests prove local provider navigation, exact extension argument delivery, and drive-like shell fallthrough.
- Plugin tests reject mappings to undeclared commands.
- Existing provider and extension implementations compile unchanged because the new trait methods have empty defaults.

## Requirement Status

`FAR-SHELL-004` is verified: registered prefixes route to providers or isolated extensions, associations use ordered typed handlers, and global/local user commands expand typed metasymbols safely.
