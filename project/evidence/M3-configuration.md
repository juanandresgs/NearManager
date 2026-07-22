# M3 Layered Configuration Evidence

Date: 2026-06-23

## Implemented Slice

- `near-config` implements schema-versioned TOML layers, recursive merge semantics, deterministic six-level precedence, plugin tie-breaking, workspace trust, leaf provenance, migrations, typed deserialization, and atomic reload rollback.
- `near-app` reexports the configuration contracts for all Near applications.
- `near-fm` resolves keymap, theme, confirmations, and handlers through the same engine.
- Far supports explicit CLI document overrides, environment automation, XDG/macOS user roots, platform defaults, plugin defaults, and trusted workspace files.
- The Settings command displays every effective field's winning source, line, column, and layer.

## Automated Evidence

- Six-layer tests shuffle input order and prove CLI wins with the correct origin.
- Multiple plugin layers prove priority and source-name tie breaking is deterministic.
- Untrusted workspace layers are rejected.
- The schema-0 fixture migrates through an explicit registered step and records migration provenance.
- Invalid reload tests prove the previous effective value remains active and the diagnostic includes source and line.
- Typed deserialization tests prove merged documents remain consumable by application schemas.
- Far CLI integration tests prove an override document wins and its exact file is reported as the `Cli` origin.
- Existing keymap rebinding tests continue proving effective configuration changes behavior without application-code changes.

## Requirement Status

`REQ-CONFIG-001` is verified by deterministic built-in/platform/plugin/user/workspace/CLI precedence, atomic last-valid reload behavior, versioned migration fixtures, and file/line/field/winning-origin diagnostics.
