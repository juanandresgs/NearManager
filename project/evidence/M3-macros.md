# M3 Semantic Macro Evidence

Date: 2026-06-23

## Implemented Slice

- `near-macros` defines versioned semantic records, canonical bindings, typed command steps, context conditions, trust, replay policy, recorder, host validation, replay reports, non-executing diagnostics, and an atomic TOML catalog store.
- `near-app` reexports the macro contracts.
- `near-fm` records after keymap resolution, replays through the normal registry and dispatcher, loads saved macros through layered configuration, persists edits to the selected user macro document, and exposes inspectable TOML.
- `Ctrl+Alt+.` opens management for listing, replaying, editing trust and conditions, binding, diagnosing, and deleting macros. Far bindings retain `Ctrl+.` recording and `Ctrl+Shift+.` last replay.

## Automated Evidence

- Recorder tests prove records contain command IDs and typed arguments rather than key events.
- Rebinding tests record movement, replace every `Down` binding, prove the old key no longer moves, then replay successfully.
- Host tests prove unavailable commands stop before invocation.
- Safety tests prove untrusted confirmable commands and trusted destructive commands are denied by default.
- Workspace tests prove untrusted Trash replay opens no operation surface and reports authorization failure.
- Management workflow tests prove catalog rendering, condition edits, canonical binding execution, diagnostics, deletion, and persistence through the injected store.
- Store tests prove schema-2 bindings round trip through atomic TOML persistence while schema-1 fixtures remain readable and cannot silently claim binding semantics.
- Application configuration tests prove layered schema-1 macro documents migrate explicitly to schema 2 before deserialization, while real external-editor PTY handoff tests continue starting the shipped binary successfully.

## Requirement Status

`REQ-MACRO-001` and `FAR-AUTO-004` are verified by semantic recording, rebinding-independent replay, editable conditions and trust, canonical bindings, per-step availability and authorization diagnostics, atomic catalog persistence, explicit deletion, layered saved records, and inspectable TOML.
