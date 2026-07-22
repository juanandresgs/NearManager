# Near External Handler Architecture

Near treats external tools as semantic handlers selected by ordered resource rules. A handler is not a shell command string: it is a versioned rule containing supported actions, a shared `ResourcePredicate`, and an explicit invocation mode.

## Rule Model

`HandlerDocument` schema version 1 contains ordered `HandlerRule` records. Each rule defines:

- a stable namespaced handler ID;
- semantic actions such as view, edit, execute, inspect, or open;
- the same typed metadata predicate used by search and filtering;
- either a structured argv template or an explicit shell template.

The first rule whose action and predicate match remains the default. The registry also returns every matching rule in configured order and can resolve one named alternative without changing that order. `Enter` resolves the semantic `Open` action for ordinary files; it never aliases the internal viewer. `F3` and `F4` retain independent View and Edit policies. The F9 File Associations menu groups matching view, edit, and execute alternatives and visibly labels structured argv versus `EXPLICIT SHELL`. Diagnostics retain every evaluated rule, selection reason, and ordered alternative.

Content predicates are rejected for handlers because safe selection must not silently perform hidden resource reads. A future content-aware resolver would need an explicit asynchronous contract.

## Structured Arguments

Structured argv is the default. Each argument is a typed atom:

- literal;
- resource URI;
- resource display name;
- exact native path;
- exact native parent path.

Every atom becomes one operating-system argument. Native paths remain `OsString` values, so spaces, newlines, metacharacters, quotes, and non-UTF-8 bytes are never reparsed or split.

## Explicit Shell Mode

Shell evaluation requires `mode = "shell"`. Near invokes the selected shell with `-lc`, marks the invocation as `ExplicitShell`, shell-quotes all known resource substitutions, rejects non-Unicode path substitution, and fails closed on unknown placeholders.

The Far workspace displays `EXPLICIT SHELL` before terminal handoff. Structured handlers display `structured argv`. Shell mode is therefore both opt-in in configuration and visible at execution time.

## macOS Integration

`near-fm` loads handler configuration from:

1. `NEAR_HANDLERS` when set;
2. `~/Library/Application Support/near/handlers.toml` when present;
3. the shipped `specs/handlers.toml` fallback.

The local filesystem adapter supplies exact native path values and metadata to the generic registry. `near.handler.diagnostics` explains view, edit, and execute matching for the focused resource through the normal command palette. The shipped macOS document reserves `/usr/bin/open <path>` for Open so Launch Services chooses the configured application; `/usr/bin/open -W -t` remains the first View/Edit text alternative. Linux Open uses `xdg-open`. Windows Open uses `explorer.exe`, while Notepad remains a View/Edit option.

External execution still uses the fail-safe suspend-and-run terminal path. Handler resolution changes selection and argument construction, not terminal restoration guarantees.
