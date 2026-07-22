# Near Semantic Macros

Near macros record semantic command invocations, never terminal bytes or default keys. A macro therefore survives key rebinding, keyboard layout changes, enhanced terminal protocols, and presentation changes.

## Record Format

`MacroDocument` schema version 2 contains inspectable `SemanticMacro` records. Version 1 documents remain readable; bindings are rejected when mislabeled as version 1. Each macro includes:

- stable ID and title;
- an optional canonical key binding;
- explicit trusted or untrusted classification;
- macro-level context conditions;
- ordered steps containing `CommandInvocation` IDs and typed arguments;
- optional step-level context conditions.

Conditions can require active context IDs, capabilities, a current resource, or a peer surface.

## Replay Contract

`MacroEngine` uses `MacroHost`, which must provide the current context, validate registration and availability, return the command safety class, and invoke through the normal dispatcher. Macros therefore use the same availability, confirmation, and operation-planning paths as keys and menus.

Untrusted macros may replay read-only and reversible commands. Confirmable commands require a trusted macro and an enabling replay policy. Destructive and privileged commands are denied by default even for trusted macros.

## Far Workflow

`Ctrl+.` starts or stops recording. The recorder captures resolved semantic invocations after keymap lookup. `Ctrl+Shift+.` replays the last macro. Rebinding a recorded key changes future keyboard behavior but not the stored command.

`Ctrl+Alt+.` opens the macro manager. It lists every configured or recorded macro with its binding, step count, and condition summary. Per-macro actions replay, edit title/trust/context/capability/resource/peer conditions, assign or remove a validated key binding, diagnose every step, or explicitly delete the macro. Bound macro keys are resolved before ordinary keymap commands and still pass through macro context, availability, safety, and normal command dispatch.

`near.macro.show-last` renders versioned TOML through the command palette. Saved macros load from layered `macros.toml`; edits are atomically written to the explicit `--macros` path or the user configuration root. Recording replaces the stable `near.macro.last-recording` entry so it immediately participates in management and persistence.

Replay stops on the first unavailable, unauthorized, or failed command and reports the exact step. Conditional skips are counted separately.

Diagnostics report macro-level availability plus each step's condition result, current command availability, safety class, and authorization decision without invoking the macro.
