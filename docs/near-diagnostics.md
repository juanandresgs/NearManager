# Near diagnostics

Near records backend-independent structured events for command dispatch, background tasks, providers, operations, extensions, and terminal sessions. Every event has a correlation identifier and may name a parent correlation, allowing a task result to be traced back to the command that started it.

`FarWorkspace::diagnostic_export` returns schema-versioned `near_core::DiagnosticExport` data. The export includes the Near package version, the capabilities active in the current action context, and the ordered event journal. `DiagnosticExport::to_pretty_json` provides a portable JSON representation for support bundles and test evidence.

## Privacy defaults

Diagnostic field names are checked case-insensitively. Any field whose name contains `path`, `content`, `token`, `secret`, `credential`, or `password` is replaced with `<redacted>` before the event enters the journal. Callers should still prefer counts, identifiers, capability names, result classes, and timing buckets over user data.

Near does not record resource contents or native paths in its built-in diagnostic events. Redaction occurs at journal insertion rather than export time, so in-memory inspection and every export format receive the same protected data.

## Correlation model

- A command begins a root correlation.
- Tasks spawned while that command is active use the command as their parent.
- Provider and operation completion events share the task correlation and parent.
- Plugin invocation creates a child correlation beneath its command.
- Terminal sessions use a session correlation from entry through restoration.

Tests cover command-to-task-to-operation tracing, provider completion, plugin parentage, terminal session lifetime, JSON metadata, and sensitive-field redaction.
